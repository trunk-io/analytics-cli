use std::{collections::HashMap, io::BufReader};

use bundle::{
    extract_files_from_tarball as extract_files_from_tarball_impl, parse_meta as parse_meta_impl,
    parse_meta_from_tarball as parse_meta_from_tarball_impl, BindingsVersionedBundle, BundleFiles,
};
use codeowners::{BindingsOwners, CodeOwners};
use context::{env, junit, meta, repo};
use prost::Message;
use pyo3::{exceptions::PyTypeError, prelude::*};
use pyo3_stub_gen::{define_stub_info_gatherer, derive::gen_stub_pyfunction};

mod py_bytes_read;

use py_bytes_read::PyBytesReader;

define_stub_info_gatherer!(stub_info);

#[gen_stub_pyfunction]
#[pyfunction]
fn env_parse(env_vars: HashMap<String, String>) -> Option<env::parser::CIInfo> {
    let mut env_parser = env::parser::EnvParser::new();
    env_parser.parse(&env_vars);

    env_parser
        .into_ci_info_parser()
        .map(|ci_info_parser| ci_info_parser.info_ci_info())
}

#[gen_stub_pyfunction]
#[pyfunction]
fn env_validate(ci_info: env::parser::CIInfo) -> env::validator::EnvValidation {
    env::validator::validate(&ci_info)
}

#[gen_stub_pyfunction]
#[pyfunction]
fn env_validation_level_to_string(
    env_validation_level: env::validator::EnvValidationLevel,
) -> String {
    env_validation_level.to_string()
}

#[gen_stub_pyfunction]
#[pyfunction]
fn branch_class_to_string(branch_class: env::parser::BranchClass) -> String {
    String::from(branch_class.to_string())
}

#[gen_stub_pyfunction]
#[pyfunction]
fn ci_platform_to_string(ci_platform: env::parser::CIPlatform) -> String {
    String::from(ci_platform.to_string())
}

#[gen_stub_pyfunction]
#[pyfunction]
fn junit_parse(xml: Vec<u8>) -> PyResult<Vec<junit::bindings::BindingsReport>> {
    let mut junit_parser = junit::parser::JunitParser::new();
    if let Err(e) = junit_parser.parse(BufReader::new(&xml[..])) {
        let collected_errors = collect_parse_errors(&junit_parser);
        if !collected_errors.is_empty() {
            return Err(PyTypeError::new_err(format!(
                "{}\n{}",
                e.to_string(),
                collected_errors
            )));
        }
        return Err(PyTypeError::new_err(e.to_string()));
    }

    let collected_errors = collect_parse_errors(&junit_parser);
    if !collected_errors.is_empty() {
        return Err(PyTypeError::new_err(collected_errors));
    }

    Ok(junit_parser
        .into_reports()
        .into_iter()
        .map(junit::bindings::BindingsReport::from)
        .collect())
}

#[gen_stub_pyfunction]
#[pyfunction]
fn bin_parse(bin: Vec<u8>) -> PyResult<Vec<junit::bindings::BindingsReport>> {
    let test_result = proto::test_context::test_run::TestResult::decode(bin.as_slice())
        .map_err(|err| PyTypeError::new_err(err.to_string()))?;
    Ok(vec![junit::bindings::BindingsReport::from(test_result)])
}

#[gen_stub_pyfunction]
#[pyfunction]
fn junit_validate(
    report: junit::bindings::BindingsReport,
) -> junit::bindings::BindingsJunitReportValidation {
    junit::bindings::BindingsJunitReportValidation::from(junit::validator::validate(&report.into()))
}

#[gen_stub_pyfunction]
#[pyfunction]
fn junit_validation_level_to_string(
    junit_validation_level: junit::validator::JunitValidationLevel,
) -> String {
    match junit_validation_level {
        junit::validator::JunitValidationLevel::Valid => "VALID".to_string(),
        junit::validator::JunitValidationLevel::SubOptimal => "SUBOPTIMAL".to_string(),
        junit::validator::JunitValidationLevel::Invalid => "INVALID".to_string(),
    }
}

#[gen_stub_pyfunction]
#[pyfunction]
fn junit_validation_type_to_string(
    junit_validation_type: junit::validator::JunitValidationType,
) -> String {
    match junit_validation_type {
        junit::validator::JunitValidationType::Report => "Report".to_string(),
        junit::validator::JunitValidationType::TestSuite => "TestSuite".to_string(),
        junit::validator::JunitValidationType::TestCase => "TestCase".to_string(),
    }
}

#[gen_stub_pyfunction]
#[pyfunction]
fn repo_validate(bundle_repo: repo::BundleRepo) -> repo::validator::RepoValidation {
    repo::validator::validate(&bundle_repo)
}

#[gen_stub_pyfunction]
#[pyfunction]
fn repo_validation_level_to_string(
    repo_validation_level: repo::validator::RepoValidationLevel,
) -> String {
    repo_validation_level.to_string()
}

#[gen_stub_pyfunction]
#[pyfunction]
pub fn parse_meta_from_tarball(
    py: Python<'_>,
    reader: PyObject,
) -> PyResult<BindingsVersionedBundle> {
    let py_bytes_reader = PyBytesReader::new(reader.into_bound(py))?;
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    let versioned_bundle = rt
        .block_on(parse_meta_from_tarball_impl(py_bytes_reader))
        .map_err(|err| PyTypeError::new_err(err.to_string()))?;
    Ok(BindingsVersionedBundle(versioned_bundle))
}

#[gen_stub_pyfunction]
#[pyfunction]
pub fn extract_files_from_tarball(py: Python<'_>, reader: PyObject) -> PyResult<BundleFiles> {
    let py_bytes_reader = PyBytesReader::new(reader.into_bound(py))?;
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    let bundle_files = rt
        .block_on(extract_files_from_tarball_impl(py_bytes_reader))
        .map_err(|err| PyTypeError::new_err(err.to_string()))?;
    Ok(bundle_files)
}

#[gen_stub_pyfunction]
#[pyfunction]
pub fn parse_meta(meta_bytes: Vec<u8>) -> PyResult<BindingsVersionedBundle> {
    let versioned_bundle =
        parse_meta_impl(meta_bytes).map_err(|err| PyTypeError::new_err(err.to_string()))?;
    Ok(BindingsVersionedBundle(versioned_bundle))
}

#[gen_stub_pyfunction]
#[pyfunction]
fn meta_validate(
    meta_context: meta::bindings::BindingsMetaContext,
) -> meta::validator::MetaValidation {
    meta::validator::validate(&meta::bindings::BindingsMetaContext::into(meta_context))
}

#[gen_stub_pyfunction]
#[pyfunction]
fn meta_validation_level_to_string(
    meta_validation_level: meta::validator::MetaValidationLevel,
) -> String {
    meta_validation_level.to_string()
}

#[gen_stub_pyfunction]
#[pyfunction]
fn codeowners_parse(codeowners_bytes: Vec<u8>) -> PyResult<BindingsOwners> {
    let codeowners = CodeOwners::parse(codeowners_bytes);
    match codeowners.owners {
        Some(owners) => Ok(BindingsOwners(owners)),
        None => Err(PyTypeError::new_err("Failed to parse CODEOWNERS file")),
    }
}

#[pymodule]
fn context_py(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<env::parser::CIInfo>()?;
    m.add_class::<env::parser::CIPlatform>()?;
    m.add_class::<env::parser::BranchClass>()?;
    m.add_class::<env::validator::EnvValidationLevel>()?;
    m.add_function(wrap_pyfunction!(env_parse, m)?)?;
    m.add_function(wrap_pyfunction!(env_validate, m)?)?;
    m.add_function(wrap_pyfunction!(env_validation_level_to_string, m)?)?;
    m.add_function(wrap_pyfunction!(branch_class_to_string, m)?)?;
    m.add_function(wrap_pyfunction!(ci_platform_to_string, m)?)?;

    m.add_class::<junit::bindings::BindingsReport>()?;
    m.add_class::<junit::bindings::BindingsTestSuite>()?;
    m.add_class::<junit::bindings::BindingsTestCase>()?;
    m.add_class::<junit::bindings::BindingsTestRerun>()?;
    m.add_class::<junit::bindings::BindingsTestCaseStatusStatus>()?;
    m.add_class::<junit::bindings::BindingsNonSuccessKind>()?;
    m.add_class::<junit::bindings::BindingsJunitReportValidation>()?;
    m.add_class::<junit::validator::JunitReportValidationFlatIssue>()?;
    m.add_class::<junit::validator::JunitValidationLevel>()?;
    m.add_class::<junit::validator::JunitValidationType>()?;
    m.add_function(wrap_pyfunction!(junit_parse, m)?)?;
    m.add_function(wrap_pyfunction!(bin_parse, m)?)?;
    m.add_function(wrap_pyfunction!(junit_validate, m)?)?;
    m.add_function(wrap_pyfunction!(junit_validation_level_to_string, m)?)?;
    m.add_function(wrap_pyfunction!(junit_validation_type_to_string, m)?)?;

    m.add_class::<repo::BundleRepo>()?;
    m.add_class::<repo::RepoUrlParts>()?;
    m.add_class::<repo::validator::RepoValidationLevel>()?;
    m.add_function(wrap_pyfunction!(repo_validate, m)?)?;
    m.add_function(wrap_pyfunction!(repo_validation_level_to_string, m)?)?;

    m.add_class::<meta::bindings::BindingsMetaContext>()?;
    m.add_class::<meta::validator::MetaValidation>()?;
    m.add_class::<meta::validator::MetaValidationLevel>()?;
    m.add_function(wrap_pyfunction!(extract_files_from_tarball, m)?)?;
    m.add_function(wrap_pyfunction!(parse_meta_from_tarball, m)?)?;
    m.add_function(wrap_pyfunction!(parse_meta, m)?)?;
    m.add_function(wrap_pyfunction!(meta_validate, m)?)?;
    m.add_function(wrap_pyfunction!(meta_validation_level_to_string, m)?)?;

    m.add_class::<codeowners::BindingsOwners>()?;
    m.add_function(wrap_pyfunction!(codeowners_parse, m)?)?;

    Ok(())
}

fn collect_parse_errors(parser: &junit::parser::JunitParser) -> String {
    parser
        .errors()
        .into_iter()
        .map(|e| e.to_string())
        .collect::<Vec<String>>()
        .join("\n")
}
