use super::*;

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
