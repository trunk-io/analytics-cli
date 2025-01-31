use context::{
    env::{
        parser::{CIInfo, EnvParser},
        EnvVars,
    },
    meta::{
        validator::{
            validate, MetaValidationIssue, MetaValidationIssueInvalid, MetaValidationLevel,
        },
        MetaContext,
    },
    repo::{BundleRepo, RepoUrlParts},
};

#[test]
fn test_branch_supplied_by_env() {
    let (ci_info, bundle_repo) = valid_ci_info_and_bundle_repo();

    let meta_context = MetaContext::new(&ci_info, &bundle_repo, None);
    let meta_validation = validate(&meta_context);

    assert_eq!(meta_validation.max_level(), MetaValidationLevel::Valid);
    assert!(meta_validation.issues().is_empty());
}

#[test]
fn test_branch_supplied_by_repo() {
    let (mut ci_info, bundle_repo) = valid_ci_info_and_bundle_repo();
    ci_info.branch = None;

    let meta_context = MetaContext::new(&ci_info, &bundle_repo, None);
    let meta_validation = validate(&meta_context);

    assert_eq!(meta_validation.max_level(), MetaValidationLevel::Valid);
    assert!(meta_validation.issues().is_empty());
}

#[test]
fn test_no_branch_supplied() {
    let (mut ci_info, mut bundle_repo) = valid_ci_info_and_bundle_repo();
    ci_info.branch = None;
    bundle_repo.repo_head_branch = String::from("");

    let meta_context = MetaContext::new(&ci_info, &bundle_repo, None);
    let meta_validation = validate(&meta_context);

    assert_eq!(meta_validation.max_level(), MetaValidationLevel::Invalid);
    pretty_assertions::assert_eq!(
        meta_validation.issues(),
        &[MetaValidationIssue::Invalid(
            MetaValidationIssueInvalid::CIInfoBranchNameTooShort(String::from(""))
        )]
    );
}

fn valid_ci_info_and_bundle_repo() -> (CIInfo, BundleRepo) {
    let job_url = String::from("https://buildkite.com/test/builds/123");
    let branch = String::from("some-branch-name");
    let env_vars = EnvVars::from_iter(
        vec![
            (
                String::from("BUILDKITE_PULL_REQUEST"),
                String::from("false"),
            ),
            (String::from("BUILDKITE_BRANCH"), String::from(&branch)),
            (String::from("BUILDKITE_BUILD_URL"), String::from(&job_url)),
            (
                String::from("BUILDKITE_BUILD_AUTHOR_EMAIL"),
                String::from(""),
            ),
            (String::from("BUILDKITE"), String::from("true")),
        ]
        .into_iter(),
    );

    let mut env_parser = EnvParser::new();
    env_parser.parse(&env_vars, None);

    let ci_info = env_parser.into_ci_info_parser().unwrap().info_ci_info();
    let bundle_repo = BundleRepo {
        repo: RepoUrlParts {
            host: String::from(""),
            owner: String::from(""),
            name: String::from(""),
        },
        repo_root: String::from("."),
        repo_url: String::from("https://buildkite.com/trunk-io/analytics-cli"),
        repo_head_branch: String::from("some-branch-name"),
        repo_head_author_email: String::from("spikey@trunk.io"),
        repo_head_commit_message: String::from("commit"),
        repo_head_author_name: String::from("Spikey"),
        repo_head_sha: String::from("abc"),
        repo_head_sha_short: Some(String::from("abc")),
        repo_head_commit_epoch: 123,
    };

    (ci_info, bundle_repo)
}
