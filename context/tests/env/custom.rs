use super::*;

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
