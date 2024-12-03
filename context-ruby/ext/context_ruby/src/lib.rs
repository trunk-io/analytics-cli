use context::{env, repo};
use magnus::{Attr, Module, Object};
use std::collections::HashMap;

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

    Ok(ci_info_class)
}

pub fn env_validate(ci_info: &env::parser::CIInfo) -> env::validator::EnvValidation {
    env::validator::validate(ci_info)
}

pub fn repo_validate(bundle_repo: repo::BundleRepo) -> repo::validator::RepoValidation {
    repo::validator::validate(&bundle_repo)
}

#[magnus::init]
fn init(ruby: &magnus::Ruby) -> Result<(), magnus::Error> {
    let ci_platform = ruby.define_class("CIPlatform", ruby.class_object())?;
    ci_platform
        .define_singleton_method("new", magnus::function!(env::parser::CIPlatform::new, 1))?;
    ci_platform.define_method(
        "to_s",
        magnus::method!(env::parser::CIPlatform::to_string, 0),
    )?;
    let ci_info = ruby.define_class("CIInfo", ruby.class_object())?;
    ci_info.define_singleton_method("new", magnus::function!(env::parser::CIInfo::new, 1))?;
    ci_info.define_method(
        "platform",
        magnus::method!(env::parser::CIInfo::platform, 0),
    )?;
    ci_info.define_method("job_url", magnus::method!(env::parser::CIInfo::job_url, 0))?;
    ci_info.define_method("branch", magnus::method!(env::parser::CIInfo::branch, 0))?;
    ci_info.define_method(
        "branch_class",
        magnus::method!(env::parser::CIInfo::branch_class, 0),
    )?;
    ci_info.define_method(
        "pr_number",
        magnus::method!(env::parser::CIInfo::pr_number, 0),
    )?;
    ci_info.define_method("actor", magnus::method!(env::parser::CIInfo::actor, 0))?;
    ci_info.define_method(
        "committer_name",
        magnus::method!(env::parser::CIInfo::committer_name, 0),
    )?;
    ci_info.define_method(
        "committer_email",
        magnus::method!(env::parser::CIInfo::committer_email, 0),
    )?;
    ci_info.define_method(
        "author_name",
        magnus::method!(env::parser::CIInfo::author_name, 0),
    )?;
    ci_info.define_method(
        "author_email",
        magnus::method!(env::parser::CIInfo::author_email, 0),
    )?;
    ci_info.define_method(
        "commit_message",
        magnus::method!(env::parser::CIInfo::commit_message, 0),
    )?;
    ci_info.define_method("title", magnus::method!(env::parser::CIInfo::title, 0))?;
    ci_info.define_method(
        "workflow",
        magnus::method!(env::parser::CIInfo::workflow, 0),
    )?;
    ci_info.define_method("job", magnus::method!(env::parser::CIInfo::job, 0))?;
    let bundle_repo = ruby.define_class("BundleRepo", ruby.class_object())?;
    bundle_repo.define_singleton_method("new", magnus::function!(repo::BundleRepo::ruby_new, 5))?;
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
