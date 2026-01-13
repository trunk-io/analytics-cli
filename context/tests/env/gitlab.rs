use super::*;

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
