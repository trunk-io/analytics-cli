use std::{collections::HashMap, io::BufReader};

use context::{env, junit, repo};
use pyo3::{exceptions::PyTypeError, prelude::*};
use pyo3_stub_gen::{define_stub_info_gatherer, derive::gen_stub_pyfunction};

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
    if junit_parser.parse(BufReader::new(&xml[..])).is_err() {
        let error_message = junit_parser
            .errors()
            .into_iter()
            .map(|e| e.to_string())
            .collect::<Vec<String>>()
            .join("\n");
        return Err(PyTypeError::new_err(error_message));
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
) -> junit::validator::JunitReportValidation {
    junit::validator::validate(&report.into())
}

#[gen_stub_pyfunction]
#[pyfunction]
fn repo_validate(bundle_repo: repo::BundleRepo) -> repo::validator::RepoValidation {
    repo::validator::validate(&bundle_repo)
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
    m.add_class::<junit::validator::JunitValidationLevel>()?;
    m.add_class::<junit::validator::JunitValidationType>()?;
    m.add_function(wrap_pyfunction!(junit_parse, m)?)?;
    m.add_function(wrap_pyfunction!(junit_validate, m)?)?;

    m.add_class::<repo::BundleRepo>()?;
    m.add_class::<repo::RepoUrlParts>()?;
    m.add_class::<repo::validator::RepoValidationLevel>()?;
    m.add_function(wrap_pyfunction!(repo_validate, m)?)?;
    Ok(())
}
