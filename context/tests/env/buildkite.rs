use super::*;

#[test]
fn test_simple_buildkite() {
    let job_url = String::from("https://buildkite.com/test/builds/123");
    let job_id = String::from("job-id");
    let full_job_url = format!("{}#{}", job_url, job_id);
    let branch = String::from("some-branch-name");
    let env_vars = EnvVars::from_iter(vec![
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
        (String::from("BUILDKITE_JOB_ID"), String::from("job-id")),
    ]);

    let mut env_parser = EnvParser::new();
    env_parser.parse(&env_vars, &[], None);

    let ci_info = env_parser.into_ci_info_parser().unwrap().info_ci_info();

    pretty_assertions::assert_eq!(
        ci_info,
        CIInfo {
            platform: CIPlatform::Buildkite,
            job_url: Some(full_job_url),
            branch: Some(branch),
            branch_class: Some(BranchClass::None),
            pr_number: None,
            actor: None,
            committer_name: None,
            committer_email: None,
            author_name: None,
            author_email: None,
            commit_message: None,
            title: None,
            workflow: None,
            job: None,
        }
    );

    let env_validation = env::validator::validate(&ci_info);
    assert_eq!(env_validation.max_level(), EnvValidationLevel::SubOptimal);
    pretty_assertions::assert_eq!(
        env_validation.issues(),
        &[
            EnvValidationIssue::SubOptimal(EnvValidationIssueSubOptimal::CIInfoActorTooShort(
                String::from("")
            )),
            EnvValidationIssue::SubOptimal(
                EnvValidationIssueSubOptimal::CIInfoAuthorEmailTooShort(String::from(""),),
            ),
            EnvValidationIssue::SubOptimal(EnvValidationIssueSubOptimal::CIInfoAuthorNameTooShort(
                String::from(""),
            ),),
            EnvValidationIssue::SubOptimal(
                EnvValidationIssueSubOptimal::CIInfoCommitMessageTooShort(String::from(""),),
            ),
            EnvValidationIssue::SubOptimal(
                EnvValidationIssueSubOptimal::CIInfoCommitterEmailTooShort(String::from(""),),
            ),
            EnvValidationIssue::SubOptimal(
                EnvValidationIssueSubOptimal::CIInfoCommitterNameTooShort(String::from(""),),
            ),
            EnvValidationIssue::SubOptimal(EnvValidationIssueSubOptimal::CIInfoTitleTooShort(
                String::from(""),
            ),),
        ]
    );
}
