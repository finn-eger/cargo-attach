use std::{convert::Infallible, os::unix::process::CommandExt, path::PathBuf, process::Command};

use cargo_metadata::{MetadataCommand, camino::Utf8Path};
use walkdir::WalkDir;

use crate::args::Args;

mod args;
mod conf;

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

pub fn attach(args: Args) -> Result<Infallible> {
    if args.release && args.debug {
        return Err("the release and debug flags may not be used together".into());
    }

    if args.bin.is_some() && args.example.is_some() {
        return Err("the bin and example options may not be used together".into());
    }

    let metadata = (MetadataCommand::new().no_deps().exec())
        // Trim trailing newlines from Cargo's errors.
        .map_err(|e| e.to_string().trim().to_owned())?;

    let Some(package) = metadata.root_package() else {
        return Err("could not determine which package to use binaries from".into());
    };

    let config = if args.probe_args.is_empty() || args.target.is_none() {
        conf::load_config(package)?
    } else {
        None
    };

    let binary_names = if let Some(binary) = args.bin.or(args.example) {
        vec![binary]
    } else {
        let targets = package.targets.iter();
        let binaries = targets.filter(|t| t.is_bin() || t.is_example());

        binaries.map(|t| t.name.to_owned()).collect()
    };

    let build_mode = if args.release {
        Some("release".to_owned())
    } else if args.debug {
        Some("debug".to_owned())
    } else {
        None
    };

    let build_target = if args.target.is_none() {
        if let Some(config) = &config {
            conf::find_build_target(config)
        } else {
            None
        }
    } else {
        args.target
    };

    let probe_args = if args.probe_args.is_empty() {
        if let Some(config) = &config {
            conf::find_probe_args(config, build_target.as_ref())?
        } else {
            vec![]
        }
    } else {
        args.probe_args
    };

    let Some(executable) = find_executable(
        &metadata.target_directory,
        binary_names,
        build_mode,
        build_target,
    ) else {
        return Err(format!("no matching executable found for package {}", package.name).into());
    };

    let error = Command::new("probe-rs")
        .arg("attach")
        .args(probe_args)
        .arg(executable)
        .exec();

    Err(error.into())
}

fn find_executable(
    base: &Utf8Path,
    binary_names: Vec<String>,
    build_mode: Option<String>,
    build_target: Option<String>,
) -> Option<PathBuf> {
    let target_files = WalkDir::new(base)
        .max_depth(4)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file());

    let executables = target_files
        .filter(|e| binary_names.iter().any(|x| x.as_str() == e.file_name()))
        .filter(|e| {
            let path = e.path().strip_prefix(base).unwrap().parent().unwrap();

            let matches_build_mode = build_mode
                .as_deref()
                .is_none_or(|m| path.iter().any(|c| c == m));

            let matches_target_triple = build_target
                .as_deref()
                .is_none_or(|t| path.iter().any(|c| c == t));

            matches_build_mode && matches_target_triple
        });

    executables
        .max_by_key(|e| e.metadata().unwrap().modified().unwrap())
        .map(|e| e.into_path())
}
