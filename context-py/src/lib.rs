use std::{collections::HashMap, io::BufReader, sync::Arc};

use bundle::{
    parse_internal_bin_from_tarball as parse_internal_bin_from_tarball_impl,
    parse_meta as parse_meta_impl, parse_meta_from_tarball as parse_meta_from_tarball_impl,
};
use codeowners::{
    associate_codeowners_multithreaded as associate_codeowners, BindingsOwners,
    BindingsOwnersAndSource, CodeOwners, CodeOwnersFile, Owners, OwnersSource,
};
use context::{
    env,
    junit::{self, junit_path::TestRunnerReport},
    meta::{bindings, id, validator},
    repo,
};
use pyo3::{exceptions::PyTypeError, prelude::*};
use pyo3_stub_gen::{define_stub_info_gatherer, derive::gen_stub_pyfunction};

mod py_bytes_read;

use py_bytes_read::PyBytesReader;

define_stub_info_gatherer!(stub_info);

#[gen_stub_pyfunction]
#[pyfunction]
#[pyo3(signature = (env_vars, stable_branches, repo=None))]
fn env_parse(
    env_vars: HashMap<String, String>,
    stable_branches: Vec<String>,
    repo: Option<repo::BundleRepo>,
) -> Option<env::parser::CIInfo> {
    let stable_branches_ref: &[&str] = &stable_branches
        .iter()
        .map(String::as_str)
        .collect::<Vec<&str>>();

    let mut env_parser = env::parser::EnvParser::new();
    env_parser.parse(&env_vars, stable_branches_ref, repo.as_ref());

    env_parser
        .into_ci_info_parser()
        .map(|ci_info_parser| ci_info_parser.info_ci_info())
}

#[gen_stub_pyfunction]
#[pyfunction]
#[pyo3(signature = (bytes, owners_source=None))]
pub fn make_codeowners_file(bytes: Vec<u8>, owners_source: Option<&str>) -> CodeOwnersFile {
    CodeOwnersFile {
        bytes,
        owners_source: parse_owners_source(owners_source),
    }
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
    branch_class.to_string()
}

#[gen_stub_pyfunction]
#[pyfunction]
fn ci_platform_to_string(ci_platform: env::parser::CIPlatform) -> String {
    ci_platform.to_string()
}

#[gen_stub_pyfunction]
#[pyfunction]
fn junit_parse(xml: Vec<u8>) -> PyResult<junit::bindings::BindingsParseResult> {
    let mut junit_parser = junit::parser::JunitParser::new();
    if let Err(e) = junit_parser.parse(BufReader::new(&xml[..])) {
        return Err(PyTypeError::new_err(e.to_string()));
    }

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

#[gen_stub_pyfunction]
#[pyfunction]
fn junit_parse_issue_level_to_string(
    junit_parse_issue_level: junit::parser::JunitParseIssueLevel,
) -> String {
    match junit_parse_issue_level {
        junit::parser::JunitParseIssueLevel::Valid => "VALID".to_string(),
        junit::parser::JunitParseIssueLevel::SubOptimal => "SUBOPTIMAL".to_string(),
        junit::parser::JunitParseIssueLevel::Invalid => "INVALID".to_string(),
    }
}

#[gen_stub_pyfunction]
#[pyfunction]
fn bin_parse(bin: Vec<u8>) -> PyResult<Vec<junit::bindings::BindingsReport>> {
    match context::junit::parser::bin_parse(&bin) {
        Ok(reports) => Ok(reports
            .into_iter()
            .map(junit::bindings::BindingsReport::from)
            .collect()),
        Err(e) => Err(PyTypeError::new_err(e.to_string())),
    }
}

// NOTE: This complains about the deprecation of using `Option<T>` as an auto-variadic
// function signature, but because we use `gen_stub_pyfunction` it always requires the second
// argument to be passed. And if you try to implement the suggested fix of using
// `#[pyo3(signature = (report, test_runner_report=None))]`, `gen_stub_pyfunction` does not work
// correctly, so it's best to just leave this the way it is.
#[gen_stub_pyfunction]
#[pyfunction]
fn junit_validate(
    report: junit::bindings::BindingsReport,
    test_runner_report: Option<bundle::FileSetTestRunnerReport>,
) -> junit::bindings::BindingsJunitReportValidation {
    junit::bindings::BindingsJunitReportValidation::from(junit::validator::validate(
        &report.into(),
        test_runner_report.map(TestRunnerReport::from),
    ))
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
        junit::validator::JunitValidationType::TestRunnerReport => "TestRunnerReport".to_string(),
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
) -> PyResult<bundle::BindingsVersionedBundle> {
    let py_bytes_reader = PyBytesReader::new(reader.into_bound(py))?;
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    let versioned_bundle = rt
        .block_on(parse_meta_from_tarball_impl(py_bytes_reader))
        .map_err(|err| PyTypeError::new_err(err.to_string()))?;
    Ok(bundle::BindingsVersionedBundle(versioned_bundle))
}

#[gen_stub_pyfunction]
#[pyfunction]
pub fn parse_internal_bin_from_tarball(
    py: Python<'_>,
    reader: PyObject,
) -> PyResult<Vec<junit::bindings::BindingsReport>> {
    let py_bytes_reader = PyBytesReader::new(reader.into_bound(py))?;
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    let test_report = rt
        .block_on(parse_internal_bin_from_tarball_impl(py_bytes_reader))
        .map_err(|err| PyTypeError::new_err(err.to_string()))?;

    Ok(test_report
        .test_results
        .into_iter()
        .map(junit::bindings::BindingsReport::from)
        .collect())
}

#[gen_stub_pyfunction]
#[pyfunction]
pub fn parse_meta(meta_bytes: Vec<u8>) -> PyResult<bundle::BindingsVersionedBundle> {
    let versioned_bundle =
        parse_meta_impl(meta_bytes).map_err(|err| PyTypeError::new_err(err.to_string()))?;
    Ok(bundle::BindingsVersionedBundle(versioned_bundle))
}

#[gen_stub_pyfunction]
#[pyfunction]
fn meta_validate(meta_context: bindings::BindingsMetaContext) -> validator::MetaValidation {
    validator::validate(&bindings::BindingsMetaContext::into(meta_context))
}

#[gen_stub_pyfunction]
#[pyfunction]
fn meta_validation_level_to_string(
    meta_validation_level: validator::MetaValidationLevel,
) -> String {
    meta_validation_level.to_string()
}

#[gen_stub_pyfunction]
#[pyfunction]
#[pyo3(signature = (codeowners_bytes, owners_source_str=None))]
fn codeowners_parse(
    codeowners_bytes: Vec<u8>,
    owners_source_str: Option<&str>,
) -> PyResult<BindingsOwners> {
    let codeowners = CodeOwners::parse(codeowners_bytes, &parse_owners_source(owners_source_str));
    match codeowners.owners {
        Some(owners) => Ok(BindingsOwners(owners)),
        None => Err(PyTypeError::new_err("Failed to parse CODEOWNERS file")),
    }
}

fn parse_owners_source(s: Option<&str>) -> OwnersSource {
    if let Some(s) = s {
        match s.to_lowercase().as_str() {
            "github" => OwnersSource::GitHub,
            "gitlab" => OwnersSource::GitLab,
            _ => OwnersSource::Unknown,
        }
    } else {
        OwnersSource::Unknown
    }
}

#[gen_stub_pyfunction]
#[pyfunction]
fn parse_many_codeowners_and_source_n_threads(
    to_parse: Vec<Option<CodeOwnersFile>>,
    num_threads: usize,
) -> PyResult<Vec<Option<BindingsOwnersAndSource>>> {
    let rt = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(num_threads)
        .enable_all()
        .build()?;
    parse_many_codeowners_and_source_multithreaded_impl(rt, to_parse)
}

#[gen_stub_pyfunction]
#[pyfunction]
fn parse_many_codeowners_n_threads(
    to_parse: Vec<Option<CodeOwnersFile>>,
    num_threads: usize,
) -> PyResult<Vec<Option<BindingsOwners>>> {
    let rt = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(num_threads)
        .enable_all()
        .build()?;
    parse_many_codeowners_multithreaded_impl(rt, to_parse)
}

#[gen_stub_pyfunction]
#[pyfunction]
fn parse_many_codeowners_multithreaded(
    to_parse: Vec<Option<CodeOwnersFile>>,
) -> PyResult<Vec<Option<BindingsOwners>>> {
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;
    parse_many_codeowners_multithreaded_impl(rt, to_parse)
}

fn parse_many_codeowners_multithreaded_impl(
    rt: tokio::runtime::Runtime,
    to_parse: Vec<Option<CodeOwnersFile>>,
) -> PyResult<Vec<Option<BindingsOwners>>> {
    let to_parse_len = to_parse.len();
    let parsed_indexes = to_parse
        .iter()
        .enumerate()
        .filter_map(|(i, file)| -> Option<usize> { file.as_ref().map(|_file| i) })
        .collect::<Vec<_>>();
    let parsed_codeowners = rt
        .block_on(CodeOwners::parse_many_multithreaded(
            to_parse.into_iter().flatten().collect(),
        ))
        .map_err(|err| PyTypeError::new_err(err.to_string()))?;

    let mut results: Vec<Option<BindingsOwners>> = vec![None; to_parse_len];
    for (i, codeowners) in parsed_codeowners.into_iter().enumerate() {
        results[parsed_indexes[i]] = codeowners.owners.map(BindingsOwners);
    }
    Ok(results)
}

fn parse_many_codeowners_and_source_multithreaded_impl(
    rt: tokio::runtime::Runtime,
    to_parse: Vec<Option<CodeOwnersFile>>,
) -> PyResult<Vec<Option<BindingsOwnersAndSource>>> {
    let to_parse_len = to_parse.len();
    let parsed_indexes = to_parse
        .iter()
        .enumerate()
        .filter_map(|(i, file)| -> Option<usize> { file.as_ref().map(|_file| i) })
        .collect::<Vec<_>>();
    let parsed_codeowners = rt
        .block_on(CodeOwners::parse_many_multithreaded_with_source(
            to_parse.into_iter().flatten().collect(),
        ))
        .map_err(|err| PyTypeError::new_err(err.to_string()))?;

    let mut results: Vec<Option<BindingsOwnersAndSource>> = vec![None; to_parse_len];
    for (i, codeowners) in parsed_codeowners.into_iter().enumerate() {
        results[parsed_indexes[i]] = codeowners.0.owners.map(|owners| BindingsOwnersAndSource {
            owners: BindingsOwners(owners),
            source: codeowners.1,
        });
    }
    Ok(results)
}

#[gen_stub_pyfunction]
#[pyfunction]
fn associate_codeowners_n_threads(
    codeowners_matchers: HashMap<String, Option<BindingsOwners>>,
    to_associate: Vec<(String, Option<String>)>,
    num_threads: usize,
) -> PyResult<Vec<Vec<String>>> {
    let rt = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(num_threads)
        .enable_all()
        .build()?;
    associate_codeowners_multithreaded_impl(rt, codeowners_matchers, to_associate)
}

#[gen_stub_pyfunction]
#[pyfunction]
fn associate_codeowners_multithreaded(
    codeowners_matchers: HashMap<String, Option<BindingsOwners>>,
    to_associate: Vec<(String, Option<String>)>,
) -> PyResult<Vec<Vec<String>>> {
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;
    associate_codeowners_multithreaded_impl(rt, codeowners_matchers, to_associate)
}

fn associate_codeowners_multithreaded_impl(
    rt: tokio::runtime::Runtime,
    codeowners_matchers: HashMap<String, Option<BindingsOwners>>,
    to_associate: Vec<(String, Option<String>)>,
) -> PyResult<Vec<Vec<String>>> {
    let to_associate_len = to_associate.len();
    let associated_indexes = to_associate
        .iter()
        .enumerate()
        .filter_map(|(i, (bundle_upload_id, file))| {
            file.as_ref().map(|_file| (i, bundle_upload_id))
        })
        .filter_map(|(i, bundle_upload_id)| {
            codeowners_matchers
                .get(bundle_upload_id)
                .map(|codeowners_matcher| (i, codeowners_matcher))
        })
        .filter_map(|(i, codeowners_matcher)| {
            codeowners_matcher.as_ref().map(|_codeowners_matcher| i)
        })
        .collect::<Vec<_>>();
    let codeowners_matchers: HashMap<String, Option<Arc<Owners>>> = codeowners_matchers
        .into_iter()
        .map(|(key, value)| {
            (
                key,
                value.map(|bindings_owners| Arc::new(bindings_owners.0)),
            )
        })
        .collect();
    let associated_codeowners = rt
        .block_on(associate_codeowners(
            to_associate
                .into_iter()
                .filter_map(|(bundle_upload_id, file)| file.map(|file| (bundle_upload_id, file)))
                .filter_map(|(bundle_upload_id, file)| {
                    codeowners_matchers
                        .get(&bundle_upload_id)
                        .map(|codeowners_matcher| (codeowners_matcher, file))
                })
                .filter_map(|(codeowners_matcher, file)| {
                    codeowners_matcher
                        .as_ref()
                        .map(|codeowners_matcher| (Arc::clone(codeowners_matcher), file))
                })
                .collect(),
        ))
        .map_err(|err| PyTypeError::new_err(err.to_string()))?;

    let mut results: Vec<Vec<String>> = vec![Vec::new(); to_associate_len];
    for (i, owners) in associated_codeowners.into_iter().enumerate() {
        results[associated_indexes[i]] = owners;
    }
    Ok(results)
}

#[gen_stub_pyfunction]
#[pyfunction]
// trunk-ignore(clippy/too_many_arguments)
pub fn gen_info_id(
    org_url_slug: String,
    repo_full_name: String,
    variant: String,
    file: Option<String>,
    classname: Option<String>,
    parent_fact_path: Option<String>,
    name: Option<String>,
    info_id: Option<String>,
) -> String {
    id::gen_info_id(
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

    m.add_class::<junit::bindings::BindingsParseResult>()?;
    m.add_class::<junit::bindings::BindingsReport>()?;
    m.add_class::<bundle::FileSetTestRunnerReport>()?;
    m.add_class::<junit::junit_path::TestRunnerReportStatus>()?;
    m.add_class::<junit::bindings::BindingsTestSuite>()?;
    m.add_class::<junit::bindings::BindingsTestCase>()?;
    m.add_class::<junit::bindings::BindingsTestRerun>()?;
    m.add_class::<junit::bindings::BindingsTestCaseStatusStatus>()?;
    m.add_class::<junit::bindings::BindingsNonSuccessKind>()?;
    m.add_class::<junit::bindings::BindingsJunitReportValidation>()?;
    m.add_class::<junit::parser::JunitParseFlatIssue>()?;
    m.add_class::<junit::parser::JunitParseIssueLevel>()?;
    m.add_class::<junit::validator::JunitReportValidationFlatIssue>()?;
    m.add_class::<junit::validator::JunitValidationLevel>()?;
    m.add_class::<junit::validator::JunitValidationType>()?;
    m.add_function(wrap_pyfunction!(junit_parse, m)?)?;
    m.add_function(wrap_pyfunction!(bin_parse, m)?)?;
    m.add_function(wrap_pyfunction!(junit_parse_issue_level_to_string, m)?)?;
    m.add_function(wrap_pyfunction!(junit_validate, m)?)?;
    m.add_function(wrap_pyfunction!(junit_validation_level_to_string, m)?)?;
    m.add_function(wrap_pyfunction!(junit_validation_type_to_string, m)?)?;

    m.add_class::<repo::BundleRepo>()?;
    m.add_class::<repo::RepoUrlParts>()?;
    m.add_class::<repo::validator::RepoValidationLevel>()?;
    m.add_function(wrap_pyfunction!(repo_validate, m)?)?;
    m.add_function(wrap_pyfunction!(repo_validation_level_to_string, m)?)?;

    m.add_class::<bindings::BindingsMetaContext>()?;
    m.add_class::<validator::MetaValidation>()?;
    m.add_class::<validator::MetaValidationLevel>()?;
    m.add_function(wrap_pyfunction!(parse_meta_from_tarball, m)?)?;
    m.add_function(wrap_pyfunction!(parse_meta, m)?)?;
    m.add_class::<bundle::BindingsVersionedBundle>()?;
    m.add_class::<bundle::BundleMetaV0_5_29>()?;
    m.add_class::<bundle::BundleMetaV0_5_34>()?;
    m.add_class::<bundle::BundleMetaV0_6_2>()?;
    m.add_class::<bundle::BundleMetaV0_6_3>()?;
    m.add_class::<bundle::BundleMetaV0_7_6>()?;
    m.add_class::<bundle::BundleMetaV0_7_7>()?;
    m.add_function(wrap_pyfunction!(meta_validate, m)?)?;
    m.add_function(wrap_pyfunction!(meta_validation_level_to_string, m)?)?;
    m.add_function(wrap_pyfunction!(parse_internal_bin_from_tarball, m)?)?;
    m.add_function(wrap_pyfunction!(gen_info_id, m)?)?;

    m.add_class::<codeowners::BindingsOwners>()?;
    m.add_function(wrap_pyfunction!(codeowners_parse, m)?)?;
    m.add_function(wrap_pyfunction!(associate_codeowners_multithreaded, m)?)?;
    m.add_function(wrap_pyfunction!(associate_codeowners_n_threads, m)?)?;
    m.add_function(wrap_pyfunction!(parse_many_codeowners_multithreaded, m)?)?;
    m.add_function(wrap_pyfunction!(parse_many_codeowners_n_threads, m)?)?;
    m.add_function(wrap_pyfunction!(make_codeowners_file, m)?)?;

    m.add_class::<codeowners::BindingsOwnersAndSource>()?;
    m.add_function(wrap_pyfunction!(
        parse_many_codeowners_and_source_n_threads,
        m
    )?)?;

    m.add_class::<codeowners::CodeOwnersFile>()?;

    Ok(())
}
