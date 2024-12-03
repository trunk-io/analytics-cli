use context::{env, junit, repo};
use magnus::{Attr, Module, Object};
use std::{collections::HashMap, io::BufReader};

pub fn env_parse(env_vars: magnus::RHash) -> Result<Option<env::parser::CIInfo>, magnus::Error> {
    let env_vars: HashMap<String, String> = env_vars.to_hash_map().unwrap_or_default();
    let mut env_parser = env::parser::EnvParser::new();
    if env_parser.parse(&env_vars).is_err() {
        let error_message = env_parser
            .errors()
            .into_iter()
            .map(|e| e.to_string())
            .collect::<Vec<String>>()
            .join("\n");
        let handle = magnus::Ruby::get().unwrap();
        return Err(magnus::Error::new(
            handle.exception_type_error(),
            error_message,
        ));
    }

    let ci_info_class = env_parser
        .into_ci_info_parser()
        .map(|ci_info_parser| ci_info_parser.info_ci_info());

    println!("{:?}", ci_info_class);
    Ok(ci_info_class)
}

pub fn env_validate(ci_info: &env::parser::CIInfo) -> env::validator::EnvValidation {
    env::validator::validate(ci_info)
}

pub fn junit_parse(
    ruby: magnus::Ruby,
    xml: Vec<u8>,
) -> Result<Vec<junit::bindings::BindingsReport>, magnus::Error> {
    let mut junit_parser = junit::parser::JunitParser::new();
    if junit_parser.parse(BufReader::new(&xml[..])).is_err() {
        let error_message = junit_parser
            .errors()
            .into_iter()
            .map(|e| e.to_string())
            .collect::<Vec<String>>()
            .join("\n");
        return Err(magnus::Error::new(
            ruby.exception_type_error(),
            error_message,
        ));
    }

    Ok(junit_parser
        .into_reports()
        .into_iter()
        .map(junit::bindings::BindingsReport::from)
        .collect())
}

pub fn junit_validate(
    report: &junit::bindings::BindingsReport,
) -> junit::validator::JunitReportValidation {
    junit::validator::validate(&report.clone().into())
}

pub fn repo_validate(bundle_repo: repo::BundleRepo) -> repo::validator::RepoValidation {
    repo::validator::validate(&bundle_repo)
}

#[magnus::init]
fn init(ruby: &magnus::Ruby) -> Result<(), magnus::Error> {
    let ci_info = ruby.define_class("CIInfo", ruby.class_object())?;
    ci_info.define_attr("job_url", Attr::ReadWrite)?;
    ci_info.define_attr("actor", Attr::ReadWrite)?;
    let bundle_repo = ruby.define_class("BundleRepo", ruby.class_object())?;
    bundle_repo.define_singleton_method(
        "initialize",
        magnus::function!(repo::BundleRepo::ruby_new, 5),
    )?;
    ruby.define_class("RepoUrlParts", ruby.class_object())?;
    ruby.define_global_function("env_parse", magnus::function!(env_parse, 1));
    let repo_validation_flat_issue =
        ruby.define_class("RepoValidationFlatIssue", ruby.class_object())?;
    repo_validation_flat_issue.define_attr("level", Attr::ReadWrite)?;
    repo_validation_flat_issue.define_attr("error_message", Attr::ReadWrite)?;
    let repo_validation = ruby.define_class("RepoValidation", ruby.class_object())?;
    repo_validation.define_method(
        "level",
        magnus::method!(repo::validator::RepoValidation::level, 0),
    )?;
    // TODO
    // repo_validation.define_method("issues_flat", magnus::method!(RepoValidation::issues_flat, 0))?;
    repo_validation.define_method(
        "max_level",
        magnus::method!(repo::validator::RepoValidation::max_level, 0),
    )?;
    Ok(())
}
