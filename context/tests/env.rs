use context::env::{
    self, EnvVars,
    parser::{BranchClass, CIInfo, CIPlatform, EnvParser},
    validator::{EnvValidationIssue, EnvValidationIssueSubOptimal, EnvValidationLevel},
};

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

#[test]
fn test_simple_github() {
    let run_id = String::from("42069");
    let actor = String::from("username");
    let repository = String::from("test/tester");
    let branch = String::from("some-branch-name");
    let workflow = String::from("test-workflow");
    let job = String::from("test-job");

    let env_vars = EnvVars::from_iter(vec![
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
        (String::from("PR_TITLE"), String::from("pr-title")),
    ]);

    let mut env_parser = EnvParser::new();
    env_parser.parse(&env_vars, &[], None);

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
            title: Some("pr-title".into()),
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

    let env_vars = EnvVars::from_iter(vec![
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
    ]);

    let mut env_parser = EnvParser::new();
    env_parser.parse(&env_vars, &[], None);

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
fn test_github_job_url_override() {
    let run_id = String::from("42069");
    let pr_number = 123;
    let actor = String::from("username");
    let repository = String::from("test/tester");
    let branch = String::from("some-branch-name");
    let workflow = String::from("Pull Request");
    let job = String::from("test-job");

    let env_vars = EnvVars::from_iter(vec![
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
        (
            String::from("JOB_URL"),
            String::from("https://example.com/job-url"),
        ),
    ]);

    let mut env_parser = EnvParser::new();
    env_parser.parse(&env_vars, &[], None);

    let ci_info = env_parser.into_ci_info_parser().unwrap().info_ci_info();

    pretty_assertions::assert_eq!(
        ci_info,
        CIInfo {
            platform: CIPlatform::GitHubActions,
            job_url: Some(String::from("https://example.com/job-url")),
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

    let env_vars = EnvVars::from_iter(vec![
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
    ]);

    let mut env_parser = EnvParser::new();
    env_parser.parse(&env_vars, &[], None);

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

    let env_vars = EnvVars::from_iter(vec![
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
    ]);

    let mut env_parser = EnvParser::new();
    env_parser.parse(&env_vars, &[], None);

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

    let env_vars = EnvVars::from_iter(vec![
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
    ]);

    let mut env_parser = EnvParser::new();
    env_parser.parse(&env_vars, &[], None);

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
fn test_simple_github_stable_branches() {
    let run_id = String::from("42069");
    let actor = String::from("username");
    let repository = String::from("test/tester");
    let branch = String::from("master");
    let workflow = String::from("test-workflow");
    let job = String::from("test-job");

    let env_vars = EnvVars::from_iter(vec![
        (String::from("GITHUB_ACTIONS"), String::from("true")),
        (String::from("GITHUB_RUN_ID"), String::from(&run_id)),
        (String::from("GITHUB_ACTOR"), String::from(&actor)),
        (String::from("GITHUB_REPOSITORY"), String::from(&repository)),
        (String::from("GITHUB_REF"), String::from(&branch)),
        (String::from("GITHUB_WORKFLOW"), String::from(&workflow)),
        (String::from("GITHUB_JOB"), String::from(&job)),
    ]);

    let mut env_parser = EnvParser::new();
    env_parser.parse(&env_vars, &["main", "master"], None);

    let ci_info = env_parser.into_ci_info_parser().unwrap().info_ci_info();

    pretty_assertions::assert_eq!(
        ci_info,
        CIInfo {
            platform: CIPlatform::GitHubActions,
            job_url: Some(format!(
                "https://github.com/{repository}/actions/runs/{run_id}"
            )),
            branch: Some(branch),
            branch_class: Some(BranchClass::ProtectedBranch),
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

    let env_vars = EnvVars::from_iter(vec![
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
    ]);

    let mut env_parser = EnvParser::new();
    env_parser.parse(&env_vars, &[], None);

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

    let env_vars = EnvVars::from_iter(vec![
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
    ]);

    let mut env_parser = EnvParser::new();
    env_parser.parse(&env_vars, &[], None);

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

    let env_vars = EnvVars::from_iter(vec![
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
    ]);

    let mut env_parser = EnvParser::new();
    env_parser.parse(&env_vars, &[], None);

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

#[test]
fn test_custom_config() {
    let job_url = String::from("https://example.com");
    let job_name = String::from("CI Job");
    let author_email = String::from("test_author@example.com");
    let author_name = String::from("John TestUser");
    let commit_branch = String::from("yvr-123-test-commit");
    let commit_message = String::from("Fixes in test branch");
    let pr_number = 123;
    let pr_title = String::from("YVR-123 Test Commit");

    let env_vars = EnvVars::from_iter(vec![
        (String::from("CUSTOM"), String::from("true")),
        (String::from("JOB_URL"), String::from(&job_url)),
        (String::from("JOB_NAME"), String::from(&job_name)),
        (String::from("AUTHOR_EMAIL"), String::from(&author_email)),
        (String::from("AUTHOR_NAME"), String::from(&author_name)),
        (String::from("COMMIT_BRANCH"), String::from(&commit_branch)),
        (
            String::from("COMMIT_MESSAGE"),
            String::from(&commit_message),
        ),
        (String::from("PR_NUMBER"), pr_number.to_string()),
        (String::from("PR_TITLE"), String::from(&pr_title)),
    ]);

    let mut env_parser = EnvParser::new();
    env_parser.parse(&env_vars, &[], None);

    let ci_info = env_parser.into_ci_info_parser().unwrap().info_ci_info();

    pretty_assertions::assert_eq!(
        ci_info,
        CIInfo {
            platform: CIPlatform::Custom,
            job_url: Some(job_url),
            branch: Some(commit_branch),
            branch_class: Some(BranchClass::PullRequest),
            pr_number: Some(pr_number),
            actor: Some(author_email.clone()),
            committer_name: Some(author_name.clone()),
            committer_email: Some(author_email.clone()),
            author_name: Some(author_name),
            author_email: Some(author_email),
            commit_message: Some(commit_message),
            title: Some(pr_title),
            workflow: Some(job_name.clone()),
            job: Some(job_name),
        }
    );
}

#[test]
fn test_simple_gitlab_stable_branches() {
    let job_url = String::from("https://example.com");
    let actor = String::from("username");
    let email = String::from("username@example.com");
    let commit_author = String::from("username <username@example.com>");
    let branch = String::from("some-branch-name");
    let workflow = String::from("test-job-name");
    let job = String::from("test-job-stage");

    let env_vars = EnvVars::from_iter(vec![
        (String::from("GITLAB_CI"), String::from("true")),
        (String::from("CI_JOB_URL"), String::from(&job_url)),
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
    ]);

    let mut env_parser = EnvParser::new();
    let stable_branches = [branch.as_str()];
    env_parser.parse(&env_vars, &stable_branches, None);

    let ci_info = env_parser.into_ci_info_parser().unwrap().info_ci_info();

    pretty_assertions::assert_eq!(
        ci_info,
        CIInfo {
            platform: CIPlatform::GitLabCI,
            job_url: Some(job_url),
            branch: Some(branch),
            branch_class: Some(BranchClass::ProtectedBranch),
            pr_number: None,
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
fn test_simple_bitbucket() {
    let workspace = String::from("my-workspace");
    let repo_slug = String::from("my-repo");
    let build_number = String::from("42");
    let branch = String::from("feature-branch");
    let pipeline_uuid = String::from("{12345678-1234-1234-1234-123456789abc}");
    let step_uuid = String::from("{abcdef12-3456-7890-abcd-ef1234567890}");

    let env_vars = EnvVars::from_iter(vec![
        (
            String::from("BITBUCKET_BUILD_NUMBER"),
            String::from(&build_number),
        ),
        (
            String::from("BITBUCKET_WORKSPACE"),
            String::from(&workspace),
        ),
        (
            String::from("BITBUCKET_REPO_SLUG"),
            String::from(&repo_slug),
        ),
        (String::from("BITBUCKET_BRANCH"), String::from(&branch)),
        (
            String::from("BITBUCKET_PIPELINE_UUID"),
            String::from(&pipeline_uuid),
        ),
        (
            String::from("BITBUCKET_STEP_UUID"),
            String::from(&step_uuid),
        ),
    ]);

    let mut env_parser = EnvParser::new();
    env_parser.parse(&env_vars, &[], None);

    let ci_info = env_parser.into_ci_info_parser().unwrap().info_ci_info();

    // step_uuid should be URL-encoded in the job_url (curly braces become %7B and %7D)
    let encoded_step_uuid = step_uuid.replace('{', "%7B").replace('}', "%7D");
    pretty_assertions::assert_eq!(
        ci_info,
        CIInfo {
            platform: CIPlatform::BitbucketPipelines,
            job_url: Some(format!(
                "https://bitbucket.org/{workspace}/{repo_slug}/pipelines/results/{build_number}/steps/{encoded_step_uuid}"
            )),
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
            workflow: Some(pipeline_uuid),
            job: Some(step_uuid),
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
fn test_bitbucket_pr() {
    let workspace = String::from("my-workspace");
    let repo_slug = String::from("my-repo");
    let build_number = String::from("123");
    let branch = String::from("feature/add-tests");
    let pr_id = 456;
    let pipeline_uuid = String::from("{pipeline-uuid-1234}");
    let step_uuid = String::from("{step-uuid-5678}");

    let env_vars = EnvVars::from_iter(vec![
        (
            String::from("BITBUCKET_BUILD_NUMBER"),
            String::from(&build_number),
        ),
        (
            String::from("BITBUCKET_WORKSPACE"),
            String::from(&workspace),
        ),
        (
            String::from("BITBUCKET_REPO_SLUG"),
            String::from(&repo_slug),
        ),
        (String::from("BITBUCKET_BRANCH"), String::from(&branch)),
        (String::from("BITBUCKET_PR_ID"), pr_id.to_string()),
        (
            String::from("BITBUCKET_PIPELINE_UUID"),
            String::from(&pipeline_uuid),
        ),
        (
            String::from("BITBUCKET_STEP_UUID"),
            String::from(&step_uuid),
        ),
    ]);

    let mut env_parser = EnvParser::new();
    env_parser.parse(&env_vars, &[], None);

    let ci_info = env_parser.into_ci_info_parser().unwrap().info_ci_info();

    // Verify that PR branch class is correctly set when BITBUCKET_PR_ID is present
    // step_uuid should be URL-encoded in the job_url
    let encoded_step_uuid = step_uuid.replace('{', "%7B").replace('}', "%7D");
    pretty_assertions::assert_eq!(
        ci_info,
        CIInfo {
            platform: CIPlatform::BitbucketPipelines,
            job_url: Some(format!(
                "https://bitbucket.org/{workspace}/{repo_slug}/pipelines/results/{build_number}/steps/{encoded_step_uuid}"
            )),
            branch: Some(branch),
            branch_class: Some(BranchClass::PullRequest),
            pr_number: Some(pr_id),
            actor: None,
            committer_name: None,
            committer_email: None,
            author_name: None,
            author_email: None,
            commit_message: None,
            title: None,
            workflow: Some(pipeline_uuid),
            job: Some(step_uuid),
        }
    );
}

#[test]
fn test_bitbucket_without_step_uuid() {
    // Test that job URL works without step UUID (no /steps/ suffix)
    // and that workflow/job are None when UUIDs not provided
    let workspace = String::from("my-workspace");
    let repo_slug = String::from("my-repo");
    let build_number = String::from("99");
    let branch = String::from("develop");

    let env_vars = EnvVars::from_iter(vec![
        (
            String::from("BITBUCKET_BUILD_NUMBER"),
            String::from(&build_number),
        ),
        (
            String::from("BITBUCKET_WORKSPACE"),
            String::from(&workspace),
        ),
        (
            String::from("BITBUCKET_REPO_SLUG"),
            String::from(&repo_slug),
        ),
        (String::from("BITBUCKET_BRANCH"), String::from(&branch)),
    ]);

    let mut env_parser = EnvParser::new();
    env_parser.parse(&env_vars, &[], None);

    let ci_info = env_parser.into_ci_info_parser().unwrap().info_ci_info();

    pretty_assertions::assert_eq!(
        ci_info,
        CIInfo {
            platform: CIPlatform::BitbucketPipelines,
            job_url: Some(format!(
                "https://bitbucket.org/{workspace}/{repo_slug}/pipelines/results/{build_number}"
            )),
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
}

#[test]
fn test_bitbucket_stable_branch() {
    let workspace = String::from("my-workspace");
    let repo_slug = String::from("my-repo");
    let build_number = String::from("200");
    let branch = String::from("main");

    let env_vars = EnvVars::from_iter(vec![
        (
            String::from("BITBUCKET_BUILD_NUMBER"),
            String::from(&build_number),
        ),
        (
            String::from("BITBUCKET_WORKSPACE"),
            String::from(&workspace),
        ),
        (
            String::from("BITBUCKET_REPO_SLUG"),
            String::from(&repo_slug),
        ),
        (String::from("BITBUCKET_BRANCH"), String::from(&branch)),
    ]);

    let mut env_parser = EnvParser::new();
    env_parser.parse(&env_vars, &["main", "master"], None);

    let ci_info = env_parser.into_ci_info_parser().unwrap().info_ci_info();

    pretty_assertions::assert_eq!(
        ci_info,
        CIInfo {
            platform: CIPlatform::BitbucketPipelines,
            job_url: Some(format!(
                "https://bitbucket.org/{workspace}/{repo_slug}/pipelines/results/{build_number}"
            )),
            branch: Some(branch),
            branch_class: Some(BranchClass::ProtectedBranch),
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
}

#[test]
fn test_bitbucket_missing_job_url_vars() {
    // Test that job_url is None when required vars are missing
    let branch = String::from("feature-branch");

    let env_vars = EnvVars::from_iter(vec![
        (String::from("BITBUCKET_BUILD_NUMBER"), String::from("42")),
        // Missing BITBUCKET_WORKSPACE and BITBUCKET_REPO_SLUG
        (String::from("BITBUCKET_BRANCH"), String::from(&branch)),
    ]);

    let mut env_parser = EnvParser::new();
    env_parser.parse(&env_vars, &[], None);

    let ci_info = env_parser.into_ci_info_parser().unwrap().info_ci_info();

    pretty_assertions::assert_eq!(
        ci_info,
        CIInfo {
            platform: CIPlatform::BitbucketPipelines,
            job_url: None,
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
}

#[test]
fn test_simple_circleci() {
    let branch = String::from("circleci-project-setup");
    let build_url = String::from("https://circleci.com/gh/trunk-io/trunk2/6");
    let workflow_id = String::from("5a984496-cf63-4f63-b315-5776a3186d4b");
    let job_name = String::from("unit-tests");
    let username = String::from("mmatheson");

    let env_vars = EnvVars::from_iter(vec![
        (String::from("CI"), String::from("true")),
        (String::from("CIRCLECI"), String::from("true")),
        (String::from("CIRCLE_JOB"), String::from(&job_name)),
        (
            String::from("CIRCLE_SHA1"),
            String::from("fcddd4c25274e885fc6fd584b0d04290289b8e3e"),
        ),
        (String::from("CIRCLE_BRANCH"), String::from(&branch)),
        (String::from("CIRCLE_USERNAME"), String::from(&username)),
        (String::from("CIRCLE_BUILD_NUM"), String::from("6")),
        (String::from("CIRCLE_BUILD_URL"), String::from(&build_url)),
        (
            String::from("CIRCLE_WORKFLOW_ID"),
            String::from(&workflow_id),
        ),
        (
            String::from("CIRCLE_REPOSITORY_URL"),
            String::from("git@github.com:trunk-io/trunk2.git"),
        ),
        (
            String::from("CIRCLE_WORKFLOW_JOB_ID"),
            String::from("8fa2fd0d-e60a-42ac-9be3-67255ba5badc"),
        ),
        (
            String::from("CIRCLE_PROJECT_REPONAME"),
            String::from("trunk2"),
        ),
        (
            String::from("CIRCLE_PROJECT_USERNAME"),
            String::from("trunk-io"),
        ),
    ]);

    let mut env_parser = EnvParser::new();
    env_parser.parse(&env_vars, &[], None);

    let ci_info = env_parser.into_ci_info_parser().unwrap().info_ci_info();

    pretty_assertions::assert_eq!(
        ci_info,
        CIInfo {
            platform: CIPlatform::CircleCI,
            job_url: Some(build_url),
            branch: Some(branch),
            branch_class: Some(BranchClass::None),
            pr_number: None,
            actor: Some(username),
            committer_name: None,
            committer_email: None,
            author_name: None,
            author_email: None,
            commit_message: None,
            title: None,
            workflow: Some(workflow_id),
            job: Some(job_name),
        }
    );
}

#[test]
fn test_circleci_pr() {
    let branch = String::from("feature/add-feature");
    let build_url = String::from("https://circleci.com/gh/my-org/my-repo/456");
    let workflow_id = String::from("workflow-def-456");
    let job_name = String::from("build-and-test");
    let username = String::from("janedoe");
    let pr_number = 42;

    let env_vars = EnvVars::from_iter(vec![
        (String::from("CIRCLECI"), String::from("true")),
        (String::from("CIRCLE_BRANCH"), String::from(&branch)),
        (String::from("CIRCLE_BUILD_URL"), String::from(&build_url)),
        (
            String::from("CIRCLE_WORKFLOW_ID"),
            String::from(&workflow_id),
        ),
        (String::from("CIRCLE_JOB"), String::from(&job_name)),
        (String::from("CIRCLE_USERNAME"), String::from(&username)),
        (String::from("CIRCLE_PR_NUMBER"), pr_number.to_string()),
    ]);

    let mut env_parser = EnvParser::new();
    env_parser.parse(&env_vars, &[], None);

    let ci_info = env_parser.into_ci_info_parser().unwrap().info_ci_info();

    pretty_assertions::assert_eq!(
        ci_info,
        CIInfo {
            platform: CIPlatform::CircleCI,
            job_url: Some(build_url),
            branch: Some(branch),
            branch_class: Some(BranchClass::PullRequest),
            pr_number: Some(pr_number),
            actor: Some(username),
            committer_name: None,
            committer_email: None,
            author_name: None,
            author_email: None,
            commit_message: None,
            title: None,
            workflow: Some(workflow_id),
            job: Some(job_name),
        }
    );
}

#[test]
fn test_circleci_stable_branch() {
    let branch = String::from("main");
    let build_url = String::from("https://circleci.com/gh/my-org/my-repo/789");

    let env_vars = EnvVars::from_iter(vec![
        (String::from("CIRCLECI"), String::from("true")),
        (String::from("CIRCLE_BRANCH"), String::from(&branch)),
        (String::from("CIRCLE_BUILD_URL"), String::from(&build_url)),
    ]);

    let stable_branches: &[&str] = &["main", "master"];

    let mut env_parser = EnvParser::new();
    env_parser.parse(&env_vars, stable_branches, None);

    let ci_info = env_parser.into_ci_info_parser().unwrap().info_ci_info();

    pretty_assertions::assert_eq!(
        ci_info,
        CIInfo {
            platform: CIPlatform::CircleCI,
            job_url: Some(build_url),
            branch: Some(branch),
            branch_class: Some(BranchClass::ProtectedBranch),
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
}

#[test]
fn test_circleci_minimal() {
    // Test that CircleCI works with minimal env vars (just the platform identifier)
    // When no branch is set, branch_class is None
    let env_vars = EnvVars::from_iter(vec![(String::from("CIRCLECI"), String::from("true"))]);

    let mut env_parser = EnvParser::new();
    env_parser.parse(&env_vars, &[], None);

    let ci_info = env_parser.into_ci_info_parser().unwrap().info_ci_info();

    pretty_assertions::assert_eq!(
        ci_info,
        CIInfo {
            platform: CIPlatform::CircleCI,
            job_url: None,
            branch: None,
            branch_class: None, // No branch means no branch_class
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
}

#[test]
fn does_not_cross_contaminate_prioritizes_custom() {
    let pr_number = 123;
    let job_url = String::from("https://example.com");
    let commit_author = String::from("username <username@example.com>");
    let branch = String::from("some-branch-name");
    let workflow = String::from("test-job-name");
    let job = String::from("test-job-stage");

    let custom_job_url = String::from("custom_job_url");
    let custom_job_name = String::from("custom_job_name");
    let custom_email = String::from("custom_email");
    let custom_name = String::from("custom_name");
    let custom_branch = String::from("custom_branch");
    let custom_commit_message = String::from("custom_commit_message");
    let custom_pr_number = 456;
    let custom_pr_title = String::from("custom_pr_title");

    // Contains both a full set of custom and gitlab vars, but we only choose one set to parse
    let env_vars = EnvVars::from_iter(vec![
        (String::from("CUSTOM"), String::from("true")),
        (String::from("JOB_URL"), String::from(&custom_job_url)),
        (String::from("JOB_NAME"), String::from(&custom_job_name)),
        (String::from("AUTHOR_EMAIL"), String::from(&custom_email)),
        (String::from("AUTHOR_NAME"), String::from(&custom_name)),
        (String::from("COMMIT_BRANCH"), String::from(&custom_branch)),
        (
            String::from("COMMIT_MESSAGE"),
            String::from(&custom_commit_message),
        ),
        (String::from("PR_NUMBER"), custom_pr_number.to_string()),
        (String::from("PR_TITLE"), String::from(&custom_pr_title)),
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
    ]);

    let mut env_parser = EnvParser::new();
    env_parser.parse(&env_vars, &[], None);

    let ci_info = env_parser.into_ci_info_parser().unwrap().info_ci_info();

    let custom_info = CIInfo {
        platform: CIPlatform::Custom,
        job_url: Some(custom_job_url),
        branch: Some(custom_branch),
        branch_class: Some(BranchClass::Merge),
        pr_number: Some(custom_pr_number),
        actor: Some(custom_email.clone()),
        committer_name: Some(custom_name.clone()),
        committer_email: Some(custom_email.clone()),
        author_name: Some(custom_name),
        author_email: Some(custom_email),
        commit_message: Some(custom_commit_message),
        title: Some(custom_pr_title),
        workflow: Some(custom_job_name.clone()),
        job: Some(custom_job_name),
    };

    pretty_assertions::assert_eq!(ci_info, custom_info);
}
