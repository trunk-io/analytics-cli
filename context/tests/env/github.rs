use super::*;

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
