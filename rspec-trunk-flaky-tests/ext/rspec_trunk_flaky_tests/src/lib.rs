use std::collections::HashMap;

use context::{env, repo};
use magnus::Module;
use test_report::report;

pub fn env_parse(
    env_vars: magnus::RHash,
    stable_branches: Vec<String>,
) -> Option<env::parser::CIInfo> {
    let env_vars: HashMap<String, String> = env_vars.to_hash_map().unwrap_or_default();
    let stable_branches_ref: &[&str] = &stable_branches
        .iter()
        .map(String::as_str)
        .collect::<Vec<&str>>();

    let mut env_parser = env::parser::EnvParser::new();
    env_parser.parse(&env_vars, stable_branches_ref, None);

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
    // Everything lives under `RSpec::Trunk` so nothing collides with names in the
    // user's app (a `Status` model, say). `RSpec` is reopened, not replaced.
    let namespace = ruby.define_module("RSpec")?.define_module("Trunk")?;
    env::parser::ruby_init(ruby, namespace)?;
    report::ruby_init(ruby, namespace)?;
    namespace.define_module_function("env_parse", magnus::function!(env_parse, 2))?;
    Ok(())
}
