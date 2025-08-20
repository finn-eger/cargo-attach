use std::{
    convert::Infallible,
    error::Error,
    fs::read_to_string,
    io::ErrorKind,
    os::unix::process::CommandExt,
    path::PathBuf,
    process::{Command, ExitCode},
};

use argh::FromArgs;
use cargo_metadata::{MetadataCommand, Package, camino::Utf8Path};
use toml::Table;
use walkdir::WalkDir;

type Result<T> = std::result::Result<T, Box<dyn Error>>;

fn main() -> ExitCode {
    let args = argh::cargo_from_env();

    let Err(error) = actual_main(args);

    eprintln!("error: {error}");

    ExitCode::FAILURE
}

#[derive(FromArgs, Debug)]
#[doc = "Run `probe-rs attach` with the most recently modified binary or
example for the current package."]
struct Args {
    #[argh(switch, short = 'r')]
    #[doc = "only consider release builds"]
    release: bool,

    #[argh(switch, short = 'd')]
    #[doc = "only consider debug builds"]
    debug: bool,

    #[argh(option, arg_name = "TRIPLE")]
    #[doc = "only consider builds for the given target triple"]
    target: Option<String>,

    #[argh(option, arg_name = "NAME")]
    #[doc = "attach to the named binary"]
    bin: Option<String>,

    #[argh(option, arg_name = "NAME")]
    #[doc = "attach to the named example"]
    example: Option<String>,

    #[argh(positional, greedy)]
    #[doc = "arguments to pass to probe-rs"]
    probe_args: Vec<String>,
}

fn actual_main(args: Args) -> Result<Infallible> {
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
        load_config(package)?
    } else {
        None
    };

    let filters = resolve_filters(&args, &package, &config)?;

    let probe_args = if args.probe_args.is_empty() {
        find_probe_args(&config)?
    } else {
        args.probe_args
    };

    let Some(executable) = find_executable(&metadata.target_directory, filters) else {
        return Err(format!("no matching executable found for package {}", package.name).into());
    };

    let error = Command::new("probe-rs")
        .arg("attach")
        .args(probe_args)
        .arg(executable)
        .exec();

    Err(error.into())
}

fn load_config(package: &Package) -> Result<Option<Table>> {
    let path = package.manifest_path.with_file_name(".cargo/config.toml");

    let file = match read_to_string(path) {
        Ok(f) => Some(f),
        Err(e) if e.kind() == ErrorKind::NotFound => None,
        Err(_) => return Err("could not read {path}".into()),
    };

    if let Some(file) = file {
        let table = file
            .parse::<Table>()
            .map_err(|_| "could not parse {path}")?;

        Ok(Some(table))
    } else {
        Ok(None)
    }
}

fn find_probe_args(config: &Option<Table>) -> Result<Vec<String>> {
    let runners = config
        .as_ref()
        .and_then(|t| t.get("target"))
        .and_then(|v| v.as_table())
        .map(|t| t.values())
        .map(|v| v.filter_map(|v| v.get("runner").and_then(|v| v.as_str())));

    let probe_args = if let Some(runners) = runners {
        runners
            .filter_map(|r| r.strip_prefix("probe-rs run "))
            .collect::<Vec<_>>()
            .try_into()
            .ok()
            .map(|[r]: [_; 1]| r)
            .map(|r| shlex::split(r))
            .unwrap_or_default()
            .ok_or("could not parse probe-rs arguments")?
    } else {
        vec![]
    };

    Ok(probe_args)
}

struct Filters {
    names: Vec<String>,
    build_mode: Option<String>,
    build_target: Option<String>,
}

fn resolve_filters(args: &Args, package: &Package, config: &Option<Table>) -> Result<Filters> {
    let build_mode = if args.release {
        Some("release".into())
    } else if args.debug {
        Some("debug".into())
    } else {
        None
    };

    let build_target = match &args.target {
        Some(t) => Some(t.as_str()),
        None => config
            .as_ref()
            .and_then(|t| t.get("build"))
            .and_then(|v| v.as_table())
            .and_then(|t| t.get("target"))
            .and_then(|v| v.as_str()),
    }
    .map(|s| s.to_owned());

    let names = match args.bin.clone().or(args.example.clone()) {
        Some(t) => vec![t],
        None => package
            .targets
            .iter()
            .filter(|t| t.is_bin() || t.is_example())
            .map(|t| t.name.to_owned())
            .collect(),
    };

    let filters = Filters {
        names,
        build_mode,
        build_target,
    };

    Ok(filters)
}

fn find_executable(
    base: &Utf8Path,
    Filters {
        names,
        build_mode,
        build_target,
    }: Filters,
) -> Option<PathBuf> {
    let target_files = WalkDir::new(&base)
        .max_depth(4)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file());

    let execs = target_files
        .filter(|e| names.iter().any(|x| x.as_str() == e.file_name()))
        .filter(|e| {
            let path = e.path().strip_prefix(&base).unwrap().parent().unwrap();

            let matches_build_mode = build_mode
                .as_deref()
                .is_none_or(|m| path.iter().any(|c| c == m));

            let matches_target_triple = build_target
                .as_deref()
                .is_none_or(|t| path.iter().any(|c| c == t));

            matches_build_mode && matches_target_triple
        });

    execs
        .max_by_key(|e| e.metadata().unwrap().modified().unwrap())
        .map(|e| e.into_path())
}
