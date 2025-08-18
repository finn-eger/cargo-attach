use std::{
    error::Error,
    fs::read_to_string,
    os::unix::process::CommandExt,
    process::{Command, ExitCode},
};

use argh::FromArgs;
use cargo_metadata::{MetadataCommand, camino::Utf8PathBuf};
use toml::Table;
use walkdir::WalkDir;

fn main() -> ExitCode {
    let args = argh::cargo_from_env();

    if let Err(error) = attach(args) {
        // Trim trailing newline from Cargo's error messages.
        let error = format!("{error}");
        let error = error.trim();

        eprintln!("error: {error}");

        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
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
}

fn attach(args: Args) -> Result<(), Box<dyn Error>> {
    if args.release && args.debug {
        Err("the release and debug flags may not be used together")?;
    }

    let metadata = MetadataCommand::new().exec()?;

    let package = metadata
        .root_package()
        .ok_or("could not determine which package to use binaries from")?;

    let build_mode = args
        .release
        .then_some("release")
        .or(args.debug.then_some("debug"));

    let target_names = package
        .targets
        .iter()
        .filter(|t| t.is_bin() || t.is_example())
        .map(|t| &t.name)
        .collect::<Vec<_>>();

    let cargo_config_path = package.manifest_path.with_file_name(".cargo/config.toml");
    let mut cargo_config = CargoConfig::Unloaded(cargo_config_path);

    let target_triple = match args.target {
        Some(t) => Some(t),
        None => cargo_config
            .get_or_load()?
            .get("build")
            .and_then(|v| v.as_table())
            .and_then(|t| t.get("target"))
            .and_then(|v| v.as_str())
            .map(|s| s.to_owned()),
    };

    // Have fun!
    let probe_args = cargo_config
        .get_or_load()?
        .get("target")
        .and_then(|v| v.as_table())
        .map(|t| t.values())
        .and_then(|v| {
            v.filter_map(|t| t.get("runner").and_then(|v| v.as_str()))
                .filter_map(|r| r.strip_prefix("probe-rs run "))
                .collect::<Vec<_>>()
                .try_into()
                .ok()
        })
        .map(|[r]: [_; 1]| r)
        .and_then(|a| shlex::split(a))
        .ok_or("could not parse probe-rs arguments")?;

    let mut target_dir = metadata.target_directory.clone();

    if let Some(target_triple) = target_triple {
        target_dir.push(target_triple);
    }

    let executable = WalkDir::new(&target_dir)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
        .filter(|e| {
            build_mode.is_none_or(|m| {
                e.path()
                    .strip_prefix(&target_dir)
                    .is_ok_and(|p| p.starts_with(m))
            })
        })
        .filter(|e| target_names.iter().any(|x| x.as_str() == e.file_name()))
        .max_by_key(|e| e.metadata().unwrap().modified().unwrap())
        .ok_or(format!(
            "no matching executable found for package {}",
            package.name
        ))?;

    Err(Command::new("probe-rs")
        .arg("attach")
        .args(probe_args)
        .arg(executable.path())
        .exec())?
}

enum CargoConfig {
    Unloaded(Utf8PathBuf),
    Loaded(Table),
}

impl CargoConfig {
    fn get_or_load(&mut self) -> Result<&Table, Box<dyn Error>> {
        match self {
            Self::Loaded(table) => Ok(table),
            Self::Unloaded(path) => {
                *self = Self::Loaded(
                    read_to_string(&path)
                        .map_err(|_| format!("could not read {path}"))?
                        .parse()
                        .map_err(|_| "could not parse {p}")?,
                );

                match self {
                    Self::Loaded(table) => Ok(table),
                    _ => unreachable!(),
                }
            }
        }
    }
}
