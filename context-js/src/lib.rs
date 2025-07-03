use std::{collections::HashMap, io::BufReader};

use bundle::{
    parse_internal_bin_and_meta_from_tarball as parse_internal_bin_and_meta,
    parse_internal_bin_from_tarball as parse_internal_bin,
    parse_meta_from_tarball as parse_tarball_meta, FileSetTestRunnerReport, VersionedBundle,
    VersionedBundleWithBindingsReport,
};
use context::{env, junit, meta::id::gen_info_id as gen_info_id_impl, repo};
use futures::{future::Either, io::BufReader as BufReaderAsync, stream::TryStreamExt};
use js_sys::Uint8Array;
use prost::Message;
use wasm_bindgen::prelude::*;
use wasm_streams::{readable::sys, readable::ReadableStream};

#[wasm_bindgen]
pub fn bin_parse(bin: Vec<u8>) -> Result<Vec<junit::bindings::BindingsReport>, JsError> {
    let test_result = proto::test_context::test_run::TestResult::decode(bin.as_slice())
        .map_err(|err| JsError::new(&err.to_string()))?;
    Ok(vec![junit::bindings::BindingsReport::from(test_result)])
}

#[wasm_bindgen]
pub fn env_parse(
    env_vars: js_sys::Object,
    stable_branches: Vec<String>,
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

    let stable_branches_ref: &[&str] = &stable_branches
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
    stable_branches: Vec<String>,
    pr_number: Option<usize>,
    gitlab_merge_request_event_type: Option<env::parser::GitLabMergeRequestEventType>,
) -> env::parser::BranchClass {
    let stable_branches_ref: &[&str] = &stable_branches
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
    test_runner_report: Option<FileSetTestRunnerReport>,
) -> junit::bindings::BindingsJunitReportValidation {
    junit::bindings::BindingsJunitReportValidation::from(junit::validator::validate(
        &report.clone().into(),
        test_runner_report.map(junit::junit_path::TestRunnerReport::from),
        &repo::BundleRepo::default(),
    ))
}

#[wasm_bindgen]
pub fn repo_validate(bundle_repo: repo::BundleRepo) -> repo::validator::RepoValidation {
    repo::validator::validate(&bundle_repo)
}

async fn get_buf_reader_from_stream(
    input: sys::ReadableStream,
) -> BufReaderAsync<impl futures::io::AsyncRead> {
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

    BufReaderAsync::new(async_read)
}

#[wasm_bindgen()]
pub async fn parse_meta_from_tarball(
    input: sys::ReadableStream,
) -> Result<VersionedBundle, JsError> {
    let buf_reader = get_buf_reader_from_stream(input).await;

    parse_tarball_meta(buf_reader)
        .await
        .map_err(|err| JsError::new(&err.to_string()))
}

#[wasm_bindgen()]
pub async fn parse_internal_bin_from_tarball(
    input: sys::ReadableStream,
) -> Result<Vec<junit::bindings::BindingsReport>, JsError> {
    let buf_reader = get_buf_reader_from_stream(input).await;
    let test_result = parse_internal_bin(buf_reader)
        .await
        .map_err(|err| JsError::new(&err.to_string()))?;

    Ok(vec![junit::bindings::BindingsReport::from(test_result)])
}

#[wasm_bindgen()]
pub async fn parse_internal_bin_and_meta_from_tarball(
    input: sys::ReadableStream,
) -> Result<VersionedBundleWithBindingsReport, JsError> {
    let buf_reader = get_buf_reader_from_stream(input).await;

    let (test_result, versioned_bundle) = parse_internal_bin_and_meta(buf_reader)
        .await
        .map_err(|err| JsError::new(&err.to_string()))?;

    Ok(VersionedBundleWithBindingsReport {
        bindings_report: vec![junit::bindings::BindingsReport::from(test_result)],
        versioned_bundle,
    })
}

#[wasm_bindgen]
// trunk-ignore(clippy/too_many_arguments)
pub fn gen_info_id(
    org_url_slug: String,
    repo_full_name: String,
    file: Option<String>,
    classname: Option<String>,
    parent_fact_path: Option<String>,
    name: Option<String>,
    info_id: Option<String>,
    variant: String,
) -> String {
    gen_info_id_impl(
        &org_url_slug,
        &repo_full_name,
        file.as_deref(),
        classname.as_deref(),
        parent_fact_path.as_deref(),
        name.as_deref(),
        info_id.as_deref(),
        &variant,
    )
}
