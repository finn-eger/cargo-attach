use std::{
    error::Error,
    fs::read_to_string,
    os::unix::process::CommandExt,
    process::{Command, ExitCode},
};

use cargo_metadata::MetadataCommand;
use toml::Table;
use walkdir::WalkDir;

fn main() -> ExitCode {
    if let Err(error) = attach() {
        // Trim trailing newline from Cargo's error messages.
        let error = format!("{error}");
        let error = error.trim();

        eprintln!("error: {error}");

        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}

fn attach() -> Result<(), Box<dyn Error>> {
    let metadata = MetadataCommand::new().exec()?;

    let package = metadata
        .root_package()
        .ok_or("could not determine which package to use binaries from")?;

    let target_names = package
        .targets
        .iter()
        .filter(|t| t.is_bin() || t.is_example())
        .map(|t| &t.name)
        .collect::<Vec<_>>();

    let cargo_config_path = &package.manifest_path.with_file_name(".cargo/config.toml");
    let cargo_config: Table = read_to_string(cargo_config_path)
        .map_err(|_| format!("could not read {cargo_config_path}"))?
        .parse()
        .map_err(|_| "could not parse {cargo_config_path}")?;

    let target_triple = cargo_config
        .get("build")
        .and_then(|v| v.as_table())
        .and_then(|t| t.get("target"))
        .and_then(|v| v.as_str());

    // Have fun!
    let probe_args = cargo_config
        .get("target")
        .and_then(|v| v.as_table())
        .map(|t| t.values())
        .and_then(|v| {
            v.filter_map(|t| t.get("runner").and_then(|v| v.as_str()))
                .filter_map(|r| r.strip_prefix("probe-rs run "))
                .map(|a| a.split_whitespace().collect::<Vec<_>>())
                .collect::<Vec<_>>()
                .try_into()
                .ok()
        })
        .map(|[a]: [_; 1]| a);

    let mut target_dir = metadata.target_directory.to_owned();

    if let Some(target_triple) = target_triple {
        target_dir.push(target_triple);
    }

    let executable = WalkDir::new(target_dir)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
        .filter(|e| target_names.iter().any(|x| x.as_str() == e.file_name()))
        .max_by_key(|e| e.metadata().unwrap().modified().unwrap())
        .ok_or(format!("no executable found for package {}", package.name))?;

    Err(Command::new("probe-rs")
        .arg("attach")
        .args(probe_args.as_deref().unwrap_or_default())
        .arg(executable.path())
        .exec())?
}
