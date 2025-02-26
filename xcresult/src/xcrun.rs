use std::{ffi::OsStr, process::Command};

use lazy_static::lazy_static;

use crate::{
    types::legacy_schema::{ActionTestPlanRunSummaries, ActionsInvocationRecord},
    types::schema::Tests,
};

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
