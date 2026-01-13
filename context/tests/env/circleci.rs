use super::*;

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
