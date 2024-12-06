use context::{env, repo};
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
    env::parser::ruby_init(ruby)?;
    env::test_reporter::ruby_init(ruby)?;
    ruby.define_global_function("env_parse", magnus::function!(env_parse, 1));
    Ok(())
}
