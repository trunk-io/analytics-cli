use super::*;

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
