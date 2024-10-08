use context::env::{
    self,
    parser::{CIInfo, CIPlatform, EnvParser},
    validator::{EnvValidationIssue, EnvValidationIssueSubOptimal, EnvValidationLevel},
    EnvVars,
};

#[test]
fn test_simple_buildkite() {
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
    env_parser.parse(&env_vars).unwrap();

    let ci_info = env_parser.into_ci_info_parser().unwrap().info_ci_info();

    pretty_assertions::assert_eq!(
        ci_info,
        CIInfo {
            platform: CIPlatform::Buildkite,
            job_url: Some(job_url),
            branch: Some(branch),
            branch_class: None,
            pr_number: None,
            actor: None,
            committer_name: None,
            committer_email: None,
            author_name: None,
            author_email: None,
            commit_message: None,
            title: None,
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

#[test]
fn test_simple_drone() {
    let job_url = String::from("https://drone.io/test/builds/123");
    let branch = String::from("some-branch-name");
    let pr_number = 123;
    let title = String::from("some title");
    let actor = String::from("username");
    let name = String::from("firstname lastname");
    let email = String::from("user@example.com");
    let env_vars = EnvVars::from_iter(
        vec![
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
        ]
        .into_iter(),
    );

    let mut env_parser = EnvParser::new();
    env_parser.parse(&env_vars).unwrap();

    let ci_info = env_parser.into_ci_info_parser().unwrap().info_ci_info();

    pretty_assertions::assert_eq!(
        ci_info,
        CIInfo {
            platform: CIPlatform::Drone,
            job_url: Some(job_url),
            branch: Some(branch),
            branch_class: None,
            pr_number: Some(pr_number),
            actor: Some(actor),
            committer_name: Some(name.clone()),
            committer_email: Some(email.clone()),
            author_name: Some(name.clone()),
            author_email: Some(email.clone()),
            commit_message: None,
            title: Some(title),
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

#[test]
fn test_simple_github() {
    let run_id = String::from("42069");
    let actor = String::from("username");
    let repository = String::from("test/tester");
    let branch = String::from("some-branch-name");

    let env_vars = EnvVars::from_iter(
        vec![
            (String::from("GITHUB_ACTIONS"), String::from("true")),
            (String::from("GITHUB_RUN_ID"), String::from(&run_id)),
            (String::from("GITHUB_ACTOR"), String::from(&actor)),
            (String::from("GITHUB_REPOSITORY"), String::from(&repository)),
            (
                String::from("GITHUB_REF"),
                format!("refs/heads/origin/{branch}"),
            ),
        ]
        .into_iter(),
    );

    let mut env_parser = EnvParser::new();
    env_parser.parse(&env_vars).unwrap();

    let ci_info = env_parser.into_ci_info_parser().unwrap().info_ci_info();

    pretty_assertions::assert_eq!(
        ci_info,
        CIInfo {
            platform: CIPlatform::GitHubActions,
            job_url: Some(format!(
                "https://github.com/{repository}/actions/runs/{run_id}"
            )),
            branch: Some(branch),
            branch_class: None,
            pr_number: None,
            actor: Some(actor),
            committer_name: None,
            committer_email: None,
            author_name: None,
            author_email: None,
            commit_message: None,
            title: None,
        }
    );

    let env_validation = env::validator::validate(&ci_info);
    assert_eq!(env_validation.max_level(), EnvValidationLevel::SubOptimal);
    pretty_assertions::assert_eq!(
        env_validation.issues(),
        &[
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
