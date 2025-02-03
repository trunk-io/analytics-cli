use std::{collections::HashMap, io::BufReader};

use bundle::{parse_meta_from_tarball as parse_tarball, VersionedBundle};
use context::{env, junit, repo};
use futures::{future::Either, io::BufReader as BufReaderAsync, stream::TryStreamExt};
use js_sys::Uint8Array;
use wasm_bindgen::prelude::*;
use wasm_streams::{readable::sys, readable::ReadableStream};

#[wasm_bindgen]
pub fn env_parse(
    env_vars: js_sys::Object,
    stable_branches: Option<Vec<String>>,
) -> Option<env::parser::CIInfo> {
    let env_vars: HashMap<String, String> = js_sys::Object::entries(&env_vars)
        .iter()
        .filter_map(|entry| {
            let key_value_tuple = js_sys::Array::from(&entry);
            let key = key_value_tuple.get(0);
            let value = key_value_tuple.get(1);
            if let (Some(k), Some(v)) = (key.as_string(), value.as_string()) {
                Some((k, v))
            } else {
                None
            }
        })
        .collect();

    let stable_branches_unwrapped = stable_branches.unwrap_or_default();
    let stable_branches_ref: &[&str] = &stable_branches_unwrapped
        .iter()
        .map(String::as_str)
        .collect::<Vec<&str>>();

    let mut env_parser = env::parser::EnvParser::new();
    env_parser.parse(&env_vars, stable_branches_ref);

    env_parser
        .into_ci_info_parser()
        .map(|ci_info_parser| ci_info_parser.info_ci_info())
}

#[wasm_bindgen]
pub fn parse_branch_class(
    value: &str,
    pr_number: Option<usize>,
    gitlab_merge_request_event_type: Option<env::parser::GitLabMergeRequestEventType>,
    stable_branches: Option<Vec<String>>,
) -> env::parser::BranchClass {
    let stable_branches_unwrapped = stable_branches.unwrap_or_default();
    let stable_branches_ref: &[&str] = &stable_branches_unwrapped
        .iter()
        .map(String::as_str)
        .collect::<Vec<&str>>();

    env::parser::BranchClass::from((
        value,
        pr_number,
        gitlab_merge_request_event_type,
        stable_branches_ref,
    ))
}

#[wasm_bindgen]
pub fn env_validate(ci_info: &env::parser::CIInfo) -> env::validator::EnvValidation {
    env::validator::validate(ci_info)
}

#[wasm_bindgen]
pub fn junit_parse(xml: Vec<u8>) -> Result<junit::bindings::BindingsParseResult, JsError> {
    let mut junit_parser = junit::parser::JunitParser::new();
    junit_parser
        .parse(BufReader::new(&xml[..]))
        .map_err(|e| JsError::new(&e.to_string()))?;

    let issues_flat = junit_parser.issues_flat();
    let mut parsed_reports = junit_parser.into_reports();

    let report = if let (1, Some(parsed_report)) = (parsed_reports.len(), parsed_reports.pop()) {
        Some(junit::bindings::BindingsReport::from(parsed_report))
    } else {
        None
    };

    Ok(junit::bindings::BindingsParseResult {
        report,
        issues: issues_flat,
    })
}

#[wasm_bindgen]
pub fn junit_validate(
    report: &junit::bindings::BindingsReport,
) -> junit::bindings::BindingsJunitReportValidation {
    junit::bindings::BindingsJunitReportValidation::from(junit::validator::validate(
        &report.clone().into(),
    ))
}

#[wasm_bindgen]
pub fn repo_validate(bundle_repo: repo::BundleRepo) -> repo::validator::RepoValidation {
    repo::validator::validate(&bundle_repo)
}

#[wasm_bindgen()]
pub async fn parse_meta_from_tarball(
    input: sys::ReadableStream,
) -> Result<VersionedBundle, JsError> {
    let readable_stream = ReadableStream::from_raw(input);

    // Many platforms do not support readable byte streams
    // https://github.com/MattiasBuelens/wasm-streams/issues/19#issuecomment-1447294077
    let async_read = match readable_stream.try_into_async_read() {
        Ok(async_read) => Either::Left(async_read),
        Err((_err, body)) => Either::Right(
            body.into_stream()
                .map_ok(|js_value| js_value.dyn_into::<Uint8Array>().unwrap_throw().to_vec())
                .map_err(|_js_error| {
                    std::io::Error::new(std::io::ErrorKind::Other, "failed to read")
                })
                .into_async_read(),
        ),
    };

    let buf_reader = BufReaderAsync::new(async_read);

    parse_tarball(buf_reader)
        .await
        .map_err(|err| JsError::new(&err.to_string()))
}
