use std::{
    error::Error,
    os::unix::process::CommandExt,
    process::{Command, ExitCode},
};

use cargo_metadata::MetadataCommand;
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

    let target_dir = &metadata.target_directory;

    let executable = WalkDir::new(target_dir)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
        .filter(|e| target_names.iter().any(|x| x.as_str() == e.file_name()))
        .max_by_key(|e| e.metadata().unwrap().modified().unwrap())
        .ok_or(format!("no executable found for package {}", package.name))?;

    let _ = Command::new("probe-rs")
        .arg("attach")
        .arg(executable.path())
        .exec();

    Ok(())
}
