use super::*;

#[test]
fn test_simple_drone() {
    let job_url = String::from("https://drone.io/test/builds/123");
    let branch = String::from("some-branch-name");
    let pr_number = 123;
    let title = String::from("some title");
    let actor = String::from("username");
    let name = String::from("firstname lastname");
    let email = String::from("user@example.com");
    let env_vars = EnvVars::from_iter(vec![
        (String::from("DRONE_BUILD_LINK"), String::from(&job_url)),
        (String::from("DRONE_SOURCE_BRANCH"), String::from(&branch)),
        (String::from("DRONE_PULL_REQUEST"), pr_number.to_string()),
        (
            String::from("DRONE_PULL_REQUEST_TITLE"),
            String::from(&title),
        ),
        (String::from("DRONE_COMMIT_AUTHOR"), String::from(&actor)),
        (
            String::from("DRONE_COMMIT_AUTHOR_NAME"),
            String::from(&name),
        ),
        (
            String::from("DRONE_COMMIT_AUTHOR_EMAIL"),
            String::from(&email),
        ),
        (String::from("DRONE"), String::from("true")),
    ]);

    let mut env_parser = EnvParser::new();
    env_parser.parse(&env_vars, &[], None);

    let ci_info = env_parser.into_ci_info_parser().unwrap().info_ci_info();

    pretty_assertions::assert_eq!(
        ci_info,
        CIInfo {
            platform: CIPlatform::Drone,
            job_url: Some(job_url),
            branch: Some(branch),
            branch_class: Some(BranchClass::PullRequest),
            pr_number: Some(pr_number),
            actor: Some(actor),
            committer_name: Some(name.clone()),
            committer_email: Some(email.clone()),
            author_name: Some(name.clone()),
            author_email: Some(email.clone()),
            commit_message: None,
            title: Some(title),
            workflow: None,
            job: None,
        }
    );

    let env_validation = env::validator::validate(&ci_info);
    assert_eq!(env_validation.max_level(), EnvValidationLevel::SubOptimal);
    pretty_assertions::assert_eq!(
        env_validation.issues(),
        &[EnvValidationIssue::SubOptimal(
            EnvValidationIssueSubOptimal::CIInfoCommitMessageTooShort(String::from(""))
        ),]
    );
}
