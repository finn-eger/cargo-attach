use std::{fs::read_to_string, io::ErrorKind, str::FromStr};

use cargo_metadata::Package;
use cargo_platform::Platform;
use toml::Table;

use crate::Result;

/// Load `.cargo/config.toml`, if it exists.
pub(crate) fn load_config(package: &Package) -> Result<Option<Table>> {
    let path = package.manifest_path.with_file_name(".cargo/config.toml");

    let file = match read_to_string(&path) {
        Ok(file) => file,
        Err(err) if err.kind() == ErrorKind::NotFound => return Ok(None),
        Err(_) => return Err(format!("could not read {path}").into()),
    };

    let Ok(table) = file.parse() else {
        return Err(format!("could not parse {path}").into());
    };

    Ok(Some(table))
}

/// Find build target in configuration.
pub(crate) fn find_build_target(config: &Table) -> Option<String> {
    let build = config.get("build").and_then(|v| v.as_table());
    let target = build.and_then(|t| t.get("target")).and_then(|v| v.as_str());

    target.map(|s| s.to_owned())
}

/// Find arguments to `probe-rs run` in runner configurations.
pub(crate) fn find_probe_args(config: &Table, target: Option<&String>) -> Result<Vec<String>> {
    let Some(targets) = config.get("target").and_then(|v| v.as_table()) else {
        return Ok(vec![]);
    };

    let runners = targets
        .iter()
        .filter(|(selector, _)| {
            let Some(target) = &target else {
                return true;
            };

            let Ok(platform) = Platform::from_str(selector) else {
                return true;
            };

            platform.matches(target, &[])
        })
        .filter_map(|(_, v)| v.get("runner").and_then(|v| v.as_str()))
        .filter_map(|r| r.strip_prefix("probe-rs run "))
        .collect::<Vec<_>>();

    if runners.len() > 1 {
        return Err("found more than one runner configuration".into());
    }

    let Ok([runner]) = TryInto::<[_; 1]>::try_into(runners) else {
        return Ok(vec![]);
    };

    let Some(probe_args) = shlex::split(runner) else {
        return Err("could not parse probe-rs arguments".into());
    };

    Ok(probe_args)
}
