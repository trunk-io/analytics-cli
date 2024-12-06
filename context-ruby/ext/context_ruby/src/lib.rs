use context::test_report::report;
use context::{env, repo};
use std::collections::HashMap;

pub fn env_parse(env_vars: magnus::RHash) -> Option<env::parser::CIInfo> {
    let env_vars: HashMap<String, String> = env_vars.to_hash_map().unwrap_or_default();
    let mut env_parser = env::parser::EnvParser::new();
    env_parser.parse(&env_vars);

    env_parser
        .into_ci_info_parser()
        .map(|ci_info_parser| ci_info_parser.info_ci_info())
}

pub fn env_validate(ci_info: &env::parser::CIInfo) -> env::validator::EnvValidation {
    env::validator::validate(ci_info)
}

pub fn repo_validate(bundle_repo: repo::BundleRepo) -> repo::validator::RepoValidation {
    repo::validator::validate(&bundle_repo)
}

#[magnus::init]
fn init(ruby: &magnus::Ruby) -> Result<(), magnus::Error> {
    env::parser::ruby_init(ruby)?;
    report::ruby_init(ruby)?;
    ruby.define_global_function("env_parse", magnus::function!(env_parse, 1));
    Ok(())
}
