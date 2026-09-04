use std::{ffi::OsStr, fs, path::Path, path::PathBuf, process::Command};

use lazy_static::lazy_static;
use serde::Deserialize;

use crate::{
    types::legacy_schema::{ActionTestPlanRunSummaries, ActionsInvocationRecord},
    types::schema::Tests,
};

#[derive(Debug, Deserialize)]
pub struct TestResultsSummary {
    /// Seconds since the Unix epoch.
    #[serde(rename = "startTime")]
    pub start_time: Option<f64>,
}

pub fn xcresulttool_get_test_results_summary<T: AsRef<OsStr>>(
    path: T,
) -> anyhow::Result<TestResultsSummary> {
    xcresulttool_min_version_check()?;

    let output = xcrun(&[
        "xcresulttool".as_ref(),
        "get".as_ref(),
        "test-results".as_ref(),
        "summary".as_ref(),
        "--path".as_ref(),
        path.as_ref(),
    ])?;

    serde_json::from_str::<TestResultsSummary>(&output)
        .map_err(|e| anyhow::anyhow!("failed to parse json from xcresulttool output: {}", e))
}

/// `None` when `name` ships in neither Xcode nor the Command Line Tools.
/// Locate a developer tool. `xcrun` is the only way to find one inside an Xcode toolchain,
/// but on Linux the Swift toolchain puts `sourcekit-lsp` on `PATH` and there is no `xcrun`.
pub fn find_program(name: &str) -> Option<PathBuf> {
    if cfg!(target_os = "macos")
        && let Some(path) = xcrun_find(name)
    {
        return Some(path);
    }
    which_program(name)
}

fn which_program(name: &str) -> Option<PathBuf> {
    which_program_in(name, &std::env::var_os("PATH")?)
}

fn which_program_in(name: &str, path_var: &OsStr) -> Option<PathBuf> {
    std::env::split_paths(path_var)
        .map(|directory| directory.join(name))
        .find(|candidate| is_executable_file(candidate))
}

/// A non-executable file of the right name is not the program, and spawning it would fail
/// later with something far less obvious than "not found".
fn is_executable_file(path: &Path) -> bool {
    let Ok(metadata) = fs::metadata(path) else {
        return false;
    };
    if !metadata.is_file() {
        return false;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        metadata.permissions().mode() & 0o111 != 0
    }
    #[cfg(not(unix))]
    true
}

pub fn xcrun_find(name: &str) -> Option<PathBuf> {
    if !cfg!(target_os = "macos") {
        return None;
    }
    let output = Command::new("xcrun").args(["--find", name]).output().ok()?;
    if !output.status.success() {
        return None;
    }
    let path = PathBuf::from(String::from_utf8(output.stdout).ok()?.trim());
    if path.as_os_str().is_empty() {
        None
    } else {
        Some(path)
    }
}

pub fn xcresulttool_get_test_results_tests<T: AsRef<OsStr>>(path: T) -> anyhow::Result<Tests> {
    xcresulttool_min_version_check()?;

    let output = xcrun(&[
        "xcresulttool".as_ref(),
        "get".as_ref(),
        "test-results".as_ref(),
        "tests".as_ref(),
        "--path".as_ref(),
        path.as_ref(),
    ])?;

    serde_json::from_str::<Tests>(&output)
        .map_err(|e| anyhow::anyhow!("failed to parse json from xcresulttool output: {}", e))
}

pub fn xcresulttool_get_object<T: AsRef<OsStr>>(
    path: T,
) -> anyhow::Result<ActionsInvocationRecord> {
    let mut args: Vec<&OsStr> = vec![
        "xcresulttool".as_ref(),
        "get".as_ref(),
        "object".as_ref(),
        "--format".as_ref(),
        "json".as_ref(),
        "--path".as_ref(),
        path.as_ref(),
    ];

    if xcresulttool_min_version_check().is_ok() {
        args.push("--legacy".as_ref());
    }

    let output = xcrun(&args)?;

    serde_json::from_str::<ActionsInvocationRecord>(&output)
        .map_err(|e| anyhow::anyhow!("failed to parse json from xcresulttool output: {}", e))
}

pub fn xcresulttool_get_object_id<T: AsRef<OsStr>, U: AsRef<OsStr>>(
    path: T,
    id: U,
) -> anyhow::Result<ActionTestPlanRunSummaries> {
    let mut args: Vec<&OsStr> = vec![
        "xcresulttool".as_ref(),
        "get".as_ref(),
        "object".as_ref(),
        "--format".as_ref(),
        "json".as_ref(),
        "--id".as_ref(),
        id.as_ref(),
        "--path".as_ref(),
        path.as_ref(),
    ];

    if xcresulttool_min_version_check().is_ok() {
        args.push("--legacy".as_ref());
    }

    let output = xcrun(&args)?;

    serde_json::from_str::<ActionTestPlanRunSummaries>(&output)
        .map_err(|e| anyhow::anyhow!("failed to parse json from xcresulttool output: {}", e))
}

const LEGACY_FLAG_MIN_VERSION: usize = 22608;
fn xcresulttool_min_version_check() -> anyhow::Result<()> {
    let version = xcresulttool_version()?;
    if version <= LEGACY_FLAG_MIN_VERSION {
        return Err(anyhow::anyhow!(
            "xcresulttool version {} is not supported, please upgrade to a version higher than {}",
            version,
            LEGACY_FLAG_MIN_VERSION
        ));
    }
    Ok(())
}

fn xcresulttool_version() -> anyhow::Result<usize> {
    let version_raw = xcrun(&["xcresulttool", "version"])?;

    lazy_static! {
        // regex to match version where the output looks like "xcresulttool version 22608, format version 3.49 (current)"
        static ref RE: regex::Regex = regex::Regex::new(r"xcresulttool version (\d+)").unwrap();
    }
    let version_parsed = RE
        .captures(&version_raw)
        .and_then(|capture_group| capture_group.get(1))
        .and_then(|version| version.as_str().parse::<usize>().ok());

    if let Some(version) = version_parsed {
        Ok(version)
    } else {
        Err(anyhow::anyhow!("failed to parse xcresulttool version"))
    }
}

fn xcrun<T: AsRef<OsStr>>(args: &[T]) -> anyhow::Result<String> {
    if !cfg!(target_os = "macos") {
        return Err(anyhow::anyhow!("xcrun is only available on macOS"));
    }
    let output = Command::new("xcrun").args(args).output()?;
    let data = if output.status.code() == Some(0) {
        output.stdout
    } else {
        output.stderr
    };
    let result = String::from_utf8(data)?;
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    // The Linux Swift toolchain puts `sourcekit-lsp` on `PATH` with no `xcrun` to ask, so the
    // fallback is the only way the declaration path can find a server there.
    #[test]
    fn a_program_on_path_is_found_without_xcrun() {
        let temp_dir = tempfile::tempdir().unwrap();
        let elsewhere = tempfile::tempdir().unwrap();
        let program = temp_dir.path().join("pretend-lsp");
        fs::write(&program, b"#!/bin/sh\n").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&program, fs::Permissions::from_mode(0o755)).unwrap();
        }
        let path_var = std::env::join_paths([elsewhere.path(), temp_dir.path()]).unwrap();

        assert_eq!(
            which_program_in("pretend-lsp", &path_var),
            Some(program.clone())
        );
        assert_eq!(which_program_in("not-installed", &path_var), None);
    }

    #[cfg(unix)]
    #[test]
    fn a_file_without_the_executable_bit_is_not_the_program() {
        use std::os::unix::fs::PermissionsExt;
        let temp_dir = tempfile::tempdir().unwrap();
        let program = temp_dir.path().join("pretend-lsp");
        fs::write(&program, b"not executable").unwrap();
        fs::set_permissions(&program, fs::Permissions::from_mode(0o644)).unwrap();
        let path_var = std::env::join_paths([temp_dir.path()]).unwrap();

        assert_eq!(which_program_in("pretend-lsp", &path_var), None);
    }
}
