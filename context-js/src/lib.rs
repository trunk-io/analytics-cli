use std::{collections::HashMap, io::BufReader};
use futures::{future::Either, io::BufReader as BufReaderAsync, stream::TryStreamExt};
use js_sys::Uint8Array;
use wasm_bindgen::prelude::*;
use wasm_streams::{readable::sys, readable::ReadableStream};

use bundle::BundlerUtil;
use context::{env, junit, repo};

#[wasm_bindgen]
pub fn env_parse(env_vars: js_sys::Object) -> Result<Option<env::parser::CIInfo>, JsError> {
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
    let mut env_parser = env::parser::EnvParser::new();
    if env_parser.parse(&env_vars).is_err() {
        let error_message = env_parser
            .errors()
            .into_iter()
            .map(|e| e.to_string())
            .collect::<Vec<String>>()
            .join("\n");
        return Err(JsError::new(&error_message));
    }

    let ci_info_class = env_parser
        .into_ci_info_parser()
        .map(|ci_info_parser| ci_info_parser.info_ci_info());

    Ok(ci_info_class)
}

#[wasm_bindgen]
pub fn parse_branch_class(
    value: &str,
    pr_number: Option<usize>,
) -> Result<env::parser::BranchClass, JsError> {
    env::parser::BranchClass::try_from((value, pr_number)).map_err(|e| JsError::new(&e.to_string()))
}

#[wasm_bindgen]
pub fn env_validate(ci_info: &env::parser::CIInfo) -> env::validator::EnvValidation {
    env::validator::validate(ci_info)
}

#[wasm_bindgen]
pub fn junit_parse(xml: Vec<u8>) -> Result<Vec<junit::bindings::BindingsReport>, JsError> {
    let mut junit_parser = junit::parser::JunitParser::new();
    if junit_parser.parse(BufReader::new(&xml[..])).is_err() {
        let error_message = junit_parser
            .errors()
            .into_iter()
            .map(|e| e.to_string())
            .collect::<Vec<String>>()
            .join("\n");
        return Err(JsError::new(&error_message));
    }

    Ok(junit_parser
        .into_reports()
        .into_iter()
        .map(junit::bindings::BindingsReport::from)
        .collect())
}

#[wasm_bindgen]
pub fn junit_validate(
    report: &junit::bindings::BindingsReport,
) -> junit::validator::JunitReportValidation {
    junit::validator::validate(&report.clone().into())
}

#[wasm_bindgen]
pub fn repo_validate(bundle_repo: repo::BundleRepo) -> repo::validator::RepoValidation {
    repo::validator::validate(&bundle_repo)
}

#[wasm_bindgen()]
pub async fn parse_meta_from_tarball(input: sys::ReadableStream) -> Result<BundlerUtil, JsError> {
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

    BundlerUtil::parse_meta_from_tarball(buf_reader)
        .await
        .map_err(|err| JsError::new(&err.to_string()))
}
