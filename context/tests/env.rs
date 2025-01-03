use context::env::{
    self,
    parser::{BranchClass, CIInfo, CIPlatform, EnvParser},
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
    env_parser.parse(&env_vars);

    let ci_info = env_parser.into_ci_info_parser().unwrap().info_ci_info();

    pretty_assertions::assert_eq!(
        ci_info,
        CIInfo {
            platform: CIPlatform::Buildkite,
            job_url: Some(job_url),
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
    env_parser.parse(&env_vars);

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

#[test]
fn test_simple_github() {
    let run_id = String::from("42069");
    let actor = String::from("username");
    let repository = String::from("test/tester");
    let branch = String::from("some-branch-name");
    let workflow = String::from("test-workflow");
    let job = String::from("test-job");

    let env_vars = EnvVars::from_iter(
        vec![
            (String::from("GITHUB_ACTIONS"), String::from("true")),
            (String::from("GITHUB_EVENT_NAME"), String::from("schedule")),
            (String::from("GITHUB_RUN_ID"), String::from(&run_id)),
            (String::from("GITHUB_ACTOR"), String::from(&actor)),
            (String::from("GITHUB_REPOSITORY"), String::from(&repository)),
            (
                String::from("GITHUB_REF"),
                format!("refs/heads/origin/{branch}"),
            ),
            (String::from("GITHUB_WORKFLOW"), String::from(&workflow)),
            (String::from("GITHUB_JOB"), String::from(&job)),
        ]
        .into_iter(),
    );

    let mut env_parser = EnvParser::new();
    env_parser.parse(&env_vars);

    let ci_info = env_parser.into_ci_info_parser().unwrap().info_ci_info();

    pretty_assertions::assert_eq!(
        ci_info,
        CIInfo {
            platform: CIPlatform::GitHubActions,
            job_url: Some(format!(
                "https://github.com/{repository}/actions/runs/{run_id}"
            )),
            branch: Some(branch),
            branch_class: Some(BranchClass::None),
            pr_number: None,
            actor: Some(actor),
            committer_name: None,
            committer_email: None,
            author_name: None,
            author_email: None,
            commit_message: None,
            title: None,
            workflow: Some(workflow),
            job: Some(job),
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

#[test]
fn test_simple_github_pr() {
    let run_id = String::from("42069");
    let pr_number = 123;
    let actor = String::from("username");
    let repository = String::from("test/tester");
    let branch = String::from("some-branch-name");
    let workflow = String::from("Pull Request");
    let job = String::from("test-job");

    let env_vars = EnvVars::from_iter(
        vec![
            (String::from("GITHUB_ACTIONS"), String::from("true")),
            (
                String::from("GITHUB_EVENT_NAME"),
                String::from("pull_request"),
            ),
            (String::from("GITHUB_RUN_ID"), String::from(&run_id)),
            (String::from("GITHUB_ACTOR"), String::from(&actor)),
            (String::from("GITHUB_REPOSITORY"), String::from(&repository)),
            (String::from("GITHUB_HEAD_REF"), String::from(&branch)),
            (
                String::from("GITHUB_REF"),
                format!("refs/pull/{pr_number}/merge"),
            ),
            (String::from("GITHUB_WORKFLOW"), String::from(&workflow)),
            (String::from("GITHUB_JOB"), String::from(&job)),
        ]
        .into_iter(),
    );

    let mut env_parser = EnvParser::new();
    env_parser.parse(&env_vars);

    let ci_info = env_parser.into_ci_info_parser().unwrap().info_ci_info();

    pretty_assertions::assert_eq!(
        ci_info,
        CIInfo {
            platform: CIPlatform::GitHubActions,
            job_url: Some(format!(
                "https://github.com/{repository}/actions/runs/{run_id}?pr={pr_number}"
            )),
            branch: Some(branch),
            branch_class: Some(BranchClass::PullRequest),
            pr_number: Some(pr_number),
            actor: Some(actor),
            committer_name: None,
            committer_email: None,
            author_name: None,
            author_email: None,
            commit_message: None,
            title: None,
            workflow: Some(workflow),
            job: Some(job),
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

#[test]
fn test_simple_github_merge_queue() {
    let run_id = String::from("42069");
    let actor = String::from("username");
    let repository = String::from("test/tester");
    let branch = String::from("gh-readonly-queue/some-branch-name");
    let workflow = String::from("Pull Request");
    let job = String::from("test-job");

    let env_vars = EnvVars::from_iter(
        vec![
            (String::from("GITHUB_ACTIONS"), String::from("true")),
            (
                String::from("GITHUB_EVENT_NAME"),
                String::from("pull_request"),
            ),
            (String::from("GITHUB_RUN_ID"), String::from(&run_id)),
            (String::from("GITHUB_ACTOR"), String::from(&actor)),
            (String::from("GITHUB_REPOSITORY"), String::from(&repository)),
            (
                String::from("GITHUB_REF"),
                String::from("refs/gh-readonly-queue/some-branch-name"),
            ),
            (String::from("GITHUB_WORKFLOW"), String::from(&workflow)),
            (String::from("GITHUB_JOB"), String::from(&job)),
        ]
        .into_iter(),
    );

    let mut env_parser = EnvParser::new();
    env_parser.parse(&env_vars);

    let ci_info = env_parser.into_ci_info_parser().unwrap().info_ci_info();

    pretty_assertions::assert_eq!(
        ci_info,
        CIInfo {
            platform: CIPlatform::GitHubActions,
            job_url: Some(format!(
                "https://github.com/{repository}/actions/runs/{run_id}"
            )),
            branch: Some(branch),
            branch_class: Some(BranchClass::Merge),
            pr_number: None,
            actor: Some(actor),
            committer_name: None,
            committer_email: None,
            author_name: None,
            author_email: None,
            commit_message: None,
            title: None,
            workflow: Some(workflow),
            job: Some(job),
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

#[test]
fn test_simple_github_trunk_merge_queue() {
    let run_id = String::from("42069");
    let actor = String::from("username");
    let repository = String::from("test/tester");
    let branch = String::from("trunk-merge/some-branch-name");
    let workflow = String::from("Pull Request");
    let job = String::from("test-job");

    let env_vars = EnvVars::from_iter(
        vec![
            (String::from("GITHUB_ACTIONS"), String::from("true")),
            (
                String::from("GITHUB_EVENT_NAME"),
                String::from("pull_request"),
            ),
            (String::from("GITHUB_RUN_ID"), String::from(&run_id)),
            (String::from("GITHUB_ACTOR"), String::from(&actor)),
            (String::from("GITHUB_REPOSITORY"), String::from(&repository)),
            (
                String::from("GITHUB_REF"),
                String::from("refs/trunk-merge/some-branch-name"),
            ),
            (String::from("GITHUB_WORKFLOW"), String::from(&workflow)),
            (String::from("GITHUB_JOB"), String::from(&job)),
        ]
        .into_iter(),
    );

    let mut env_parser = EnvParser::new();
    env_parser.parse(&env_vars);

    let ci_info = env_parser.into_ci_info_parser().unwrap().info_ci_info();

    pretty_assertions::assert_eq!(
        ci_info,
        CIInfo {
            platform: CIPlatform::GitHubActions,
            job_url: Some(format!(
                "https://github.com/{repository}/actions/runs/{run_id}"
            )),
            branch: Some(branch),
            branch_class: Some(BranchClass::Merge),
            pr_number: None,
            actor: Some(actor),
            committer_name: None,
            committer_email: None,
            author_name: None,
            author_email: None,
            commit_message: None,
            title: None,
            workflow: Some(workflow),
            job: Some(job),
        }
    );
}

#[test]
fn test_simple_github_graphite_merge_queue() {
    let run_id = String::from("42069");
    let actor = String::from("username");
    let repository = String::from("test/tester");
    let branch = String::from("gtmq_some-branch-name");
    let workflow = String::from("Pull Request");
    let job = String::from("test-job");

    let env_vars = EnvVars::from_iter(
        vec![
            (String::from("GITHUB_ACTIONS"), String::from("true")),
            (
                String::from("GITHUB_EVENT_NAME"),
                String::from("pull_request"),
            ),
            (String::from("GITHUB_RUN_ID"), String::from(&run_id)),
            (String::from("GITHUB_ACTOR"), String::from(&actor)),
            (String::from("GITHUB_REPOSITORY"), String::from(&repository)),
            (
                String::from("GITHUB_REF"),
                String::from("refs/gtmq_some-branch-name"),
            ),
            (String::from("GITHUB_WORKFLOW"), String::from(&workflow)),
            (String::from("GITHUB_JOB"), String::from(&job)),
        ]
        .into_iter(),
    );

    let mut env_parser = EnvParser::new();
    env_parser.parse(&env_vars);

    let ci_info = env_parser.into_ci_info_parser().unwrap().info_ci_info();

    pretty_assertions::assert_eq!(
        ci_info,
        CIInfo {
            platform: CIPlatform::GitHubActions,
            job_url: Some(format!(
                "https://github.com/{repository}/actions/runs/{run_id}"
            )),
            branch: Some(branch),
            branch_class: Some(BranchClass::Merge),
            pr_number: None,
            actor: Some(actor),
            committer_name: None,
            committer_email: None,
            author_name: None,
            author_email: None,
            commit_message: None,
            title: None,
            workflow: Some(workflow),
            job: Some(job),
        }
    );
}

#[test]
fn test_simple_semaphore() {
    let job_id = String::from("42069");
    let project_id = String::from("12345");
    let org_url = String::from("https://semaphoreci.com");
    let actor = String::from("username");
    let branch = String::from("some-branch-name");
    let workflow = String::from("test-workflow");
    let job = String::from("test-job");

    let env_vars = EnvVars::from_iter(
        vec![
            (String::from("SEMAPHORE"), String::from("true")),
            (
                String::from("SEMAPHORE_ORGANIZATION_URL"),
                String::from(&org_url),
            ),
            (String::from("SEMAPHORE_JOB_ID"), String::from(&job_id)),
            (
                String::from("SEMAPHORE_PROJECT_ID"),
                String::from(&project_id),
            ),
            (
                String::from("SEMAPHORE_GIT_COMMIT_AUTHOR"),
                String::from(&actor),
            ),
            (
                String::from("SEMAPHORE_GIT_BRANCH"),
                format!("refs/heads/origin/{branch}"),
            ),
            (
                String::from("SEMAPHORE_PROJECT_NAME"),
                String::from(&workflow),
            ),
            (String::from("SEMAPHORE_JOB_NAME"), String::from(&job)),
        ]
        .into_iter(),
    );

    let mut env_parser = EnvParser::new();
    env_parser.parse(&env_vars);

    let ci_info = env_parser.into_ci_info_parser().unwrap().info_ci_info();

    pretty_assertions::assert_eq!(
        ci_info,
        CIInfo {
            platform: CIPlatform::Semaphore,
            job_url: Some(format!("{org_url}/projects/{project_id}/jobs/{job_id}")),
            branch: Some(branch),
            branch_class: Some(BranchClass::None),
            pr_number: None,
            actor: Some(actor.clone()),
            committer_name: None,
            committer_email: None,
            author_name: Some(actor),
            author_email: None,
            commit_message: None,
            title: None,
            workflow: Some(workflow),
            job: Some(job),
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
fn test_simple_gitlab_pr() {
    let pr_number = 123;
    let job_url = String::from("https://example.com");
    let actor = String::from("username");
    let email = String::from("username@example.com");
    let commit_author = String::from("username <username@example.com>");
    let branch = String::from("some-branch-name");
    let workflow = String::from("test-job-name");
    let job = String::from("test-job-stage");

    let env_vars = EnvVars::from_iter(
        vec![
            (String::from("GITLAB_CI"), String::from("true")),
            (String::from("CI_JOB_URL"), String::from(&job_url)),
            (String::from("CI_MERGE_REQUEST_IID"), pr_number.to_string()),
            (
                String::from("CI_COMMIT_AUTHOR"),
                String::from(&commit_author),
            ),
            (
                String::from("CI_COMMIT_REF_NAME"),
                format!("remotes/{branch}"),
            ),
            (String::from("CI_JOB_NAME"), String::from(&workflow)),
            (String::from("CI_JOB_STAGE"), String::from(&job)),
        ]
        .into_iter(),
    );

    let mut env_parser = EnvParser::new();
    env_parser.parse(&env_vars);

    let ci_info = env_parser.into_ci_info_parser().unwrap().info_ci_info();

    pretty_assertions::assert_eq!(
        ci_info,
        CIInfo {
            platform: CIPlatform::GitLabCI,
            job_url: Some(job_url),
            branch: Some(branch),
            branch_class: Some(BranchClass::PullRequest),
            pr_number: Some(pr_number),
            actor: Some(actor.clone()),
            committer_name: Some(actor.clone()),
            committer_email: Some(email.clone()),
            author_name: Some(actor),
            author_email: Some(email),
            commit_message: None,
            title: None,
            workflow: Some(workflow),
            job: Some(job),
        }
    );

    let env_validation = env::validator::validate(&ci_info);
    assert_eq!(env_validation.max_level(), EnvValidationLevel::SubOptimal);
    pretty_assertions::assert_eq!(
        env_validation.issues(),
        &[
            EnvValidationIssue::SubOptimal(
                EnvValidationIssueSubOptimal::CIInfoCommitMessageTooShort(String::from(""),),
            ),
            EnvValidationIssue::SubOptimal(EnvValidationIssueSubOptimal::CIInfoTitleTooShort(
                String::from(""),
            ),),
        ]
    );
}

#[test]
fn test_simple_gitlab_merge_branch() {
    let pr_number = 123;
    let job_url = String::from("https://example.com");
    let actor = String::from("username");
    let email = String::from("username@example.com");
    let commit_author = String::from("username <username@example.com>");
    let branch = String::from("some-branch-name");
    let workflow = String::from("test-job-name");
    let job = String::from("test-job-stage");

    let env_vars = EnvVars::from_iter(
        vec![
            (String::from("GITLAB_CI"), String::from("true")),
            (String::from("CI_JOB_URL"), String::from(&job_url)),
            (String::from("CI_MERGE_REQUEST_IID"), pr_number.to_string()),
            (
                String::from("CI_COMMIT_AUTHOR"),
                String::from(&commit_author),
            ),
            (
                String::from("CI_COMMIT_REF_NAME"),
                format!("remotes/{branch}"),
            ),
            (String::from("CI_JOB_NAME"), String::from(&workflow)),
            (String::from("CI_JOB_STAGE"), String::from(&job)),
            (
                String::from("CI_MERGE_REQUEST_EVENT_TYPE"),
                String::from("merge_train"),
            ),
        ]
        .into_iter(),
    );

    let mut env_parser = EnvParser::new();
    env_parser.parse(&env_vars);

    let ci_info = env_parser.into_ci_info_parser().unwrap().info_ci_info();

    pretty_assertions::assert_eq!(
        ci_info,
        CIInfo {
            platform: CIPlatform::GitLabCI,
            job_url: Some(job_url),
            branch: Some(branch),
            branch_class: Some(BranchClass::Merge),
            pr_number: Some(pr_number),
            actor: Some(actor.clone()),
            committer_name: Some(actor.clone()),
            committer_email: Some(email.clone()),
            author_name: Some(actor),
            author_email: Some(email),
            commit_message: None,
            title: None,
            workflow: Some(workflow),
            job: Some(job),
        }
    );

    let env_validation = env::validator::validate(&ci_info);
    assert_eq!(env_validation.max_level(), EnvValidationLevel::SubOptimal);
    pretty_assertions::assert_eq!(
        env_validation.issues(),
        &[
            EnvValidationIssue::SubOptimal(
                EnvValidationIssueSubOptimal::CIInfoCommitMessageTooShort(String::from(""),),
            ),
            EnvValidationIssue::SubOptimal(EnvValidationIssueSubOptimal::CIInfoTitleTooShort(
                String::from(""),
            ),),
        ]
    );
}
