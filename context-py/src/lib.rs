use std::{collections::HashMap, io::BufReader};

use bundle::{parse_meta_from_tarball as parse_tarball, BindingsVersionedBundle};
use codeowners::CodeOwners;
use context::{env, junit, repo};
use pyo3::{exceptions::PyTypeError, prelude::*};
use pyo3_stub_gen::{define_stub_info_gatherer, derive::gen_stub_pyfunction};

mod py_bytes_read;

use py_bytes_read::PyBytesReader;

define_stub_info_gatherer!(stub_info);

#[gen_stub_pyfunction]
#[pyfunction]
fn env_parse(env_vars: HashMap<String, String>) -> PyResult<Option<env::parser::CIInfo>> {
    let mut env_parser = env::parser::EnvParser::new();
    if env_parser.parse(&env_vars).is_err() {
        let error_message = env_parser
            .errors()
            .into_iter()
            .map(|e| e.to_string())
            .collect::<Vec<String>>()
            .join("\n");
        return Err(PyTypeError::new_err(error_message));
    }

    let ci_info_class = env_parser
        .into_ci_info_parser()
        .map(|ci_info_parser| ci_info_parser.info_ci_info());

    Ok(ci_info_class)
}

#[gen_stub_pyfunction]
#[pyfunction]
fn env_validate(ci_info: env::parser::CIInfo) -> env::validator::EnvValidation {
    env::validator::validate(&ci_info)
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
pub fn parse_meta_from_tarball(
    py: Python<'_>,
    reader: PyObject,
) -> PyResult<BindingsVersionedBundle> {
    let py_bytes_reader = PyBytesReader::new(reader.into_bound(py))?;
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    let versioned_bundle = rt
        .block_on(parse_tarball(py_bytes_reader))
        .map_err(|err| PyTypeError::new_err(err.to_string()))?;
    Ok(BindingsVersionedBundle(versioned_bundle))
}

#[gen_stub_pyfunction]
#[pyfunction]
fn codeowners_parse(codeowners_bytes: Vec<u8>) -> CodeOwners {
    CodeOwners::parse(codeowners_bytes)
}

#[pymodule]
fn context_py(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<env::parser::CIPlatform>()?;
    m.add_class::<env::parser::BranchClass>()?;
    m.add_class::<env::validator::EnvValidationLevel>()?;
    m.add_function(wrap_pyfunction!(env_parse, m)?)?;
    m.add_function(wrap_pyfunction!(env_validate, m)?)?;

    m.add_class::<junit::bindings::BindingsReport>()?;
    m.add_class::<junit::bindings::BindingsTestSuite>()?;
    m.add_class::<junit::bindings::BindingsTestCase>()?;
    m.add_class::<junit::bindings::BindingsTestCaseStatusStatus>()?;
    m.add_class::<junit::bindings::BindingsNonSuccessKind>()?;
    m.add_class::<junit::bindings::BindingsJunitReportValidation>()?;
    m.add_class::<junit::validator::JunitReportValidationFlatIssue>()?;
    m.add_class::<junit::validator::JunitValidationLevel>()?;
    m.add_class::<junit::validator::JunitValidationType>()?;
    m.add_function(wrap_pyfunction!(junit_parse, m)?)?;
    m.add_function(wrap_pyfunction!(junit_validate, m)?)?;
    m.add_function(wrap_pyfunction!(junit_validation_level_to_string, m)?)?;
    m.add_function(wrap_pyfunction!(junit_validation_type_to_string, m)?)?;

    m.add_class::<repo::BundleRepo>()?;
    m.add_class::<repo::RepoUrlParts>()?;
    m.add_class::<repo::validator::RepoValidationLevel>()?;
    m.add_function(wrap_pyfunction!(repo_validate, m)?)?;

    m.add_class::<codeowners::CodeOwners>()?;
    m.add_function(wrap_pyfunction!(codeowners_parse, m)?)?;

    m.add_function(wrap_pyfunction!(parse_meta_from_tarball, m)?)?;
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
