use test_utils::mock_git_repo::{setup_repo_with_commit, TEST_BRANCH, TEST_ORIGIN};
use trunk_analytics_cli::scanner::*;
use trunk_analytics_cli::types::Repo;

mod test_utils;

#[test]
fn test_try_read_from_root() {
    let root = tempfile::tempdir()
        .expect("failed to create temp directory")
        .into_path();
    setup_repo_with_commit(&root).expect("failed to setup repo");
    let bundle_repo = BundleRepo::try_read_from_root(
        Some(root.to_str().unwrap().to_string()),
        None,
        None,
        None,
        None,
    );

    assert!(bundle_repo.is_ok());
    let bundle_repo = bundle_repo.unwrap();
    assert_eq!(bundle_repo.repo_root, root.to_str().unwrap());
    assert_eq!(
        bundle_repo.repo,
        Repo {
            host: "github.com".to_string(),
            owner: "trunk-io".to_string(),
            name: "analytics-cli".to_string(),
        }
    );
    assert_eq!(bundle_repo.repo_url, TEST_ORIGIN);
    assert_eq!(
        bundle_repo.repo_head_branch,
        format!("refs/heads/{}", TEST_BRANCH)
    );
    assert_eq!(bundle_repo.repo_head_sha.len(), 40);
    assert!(bundle_repo.repo_head_commit_epoch > 0);
    assert_eq!(bundle_repo.repo_head_commit_message, "Initial commit");
}

#[test]
fn test_try_read_from_root_with_url_override() {
    let root = tempfile::tempdir()
        .expect("failed to create temp directory")
        .into_path();
    setup_repo_with_commit(&root).expect("failed to setup repo");
    let origin_url = "https://host.com/owner/repo.git";
    let bundle_repo = BundleRepo::try_read_from_root(
        Some(root.to_str().unwrap().to_string()),
        Some(origin_url.to_string()),
        None,
        None,
        None,
    );

    assert!(bundle_repo.is_ok());
    let bundle_repo = bundle_repo.unwrap();
    assert_eq!(bundle_repo.repo_root, root.to_str().unwrap());
    assert_eq!(
        bundle_repo.repo,
        Repo {
            host: "host.com".to_string(),
            owner: "owner".to_string(),
            name: "repo".to_string(),
        }
    );
    assert_eq!(bundle_repo.repo_url, origin_url);
    assert_eq!(
        bundle_repo.repo_head_branch,
        format!("refs/heads/{}", TEST_BRANCH)
    );
    assert_eq!(bundle_repo.repo_head_sha.len(), 40);
    assert!(bundle_repo.repo_head_commit_epoch > 0);
    assert_eq!(bundle_repo.repo_head_commit_message, "Initial commit");
}

#[test]
fn test_try_read_from_root_with_sha_override() {
    let root = tempfile::tempdir()
        .expect("failed to create temp directory")
        .into_path();
    setup_repo_with_commit(&root).expect("failed to setup repo");
    let sha = "1234567890123456789012345678901234567890";
    let bundle_repo = BundleRepo::try_read_from_root(
        Some(root.to_str().unwrap().to_string()),
        None,
        Some(sha.to_string()),
        None,
        None,
    );

    assert!(bundle_repo.is_ok());
    let bundle_repo = bundle_repo.unwrap();
    assert_eq!(bundle_repo.repo_root, root.to_str().unwrap());
    assert_eq!(
        bundle_repo.repo,
        Repo {
            host: "github.com".to_string(),
            owner: "trunk-io".to_string(),
            name: "analytics-cli".to_string(),
        }
    );
    assert_eq!(bundle_repo.repo_url, TEST_ORIGIN);
    assert_eq!(
        bundle_repo.repo_head_branch,
        format!("refs/heads/{}", TEST_BRANCH)
    );
    assert_eq!(bundle_repo.repo_head_sha, sha);
    assert!(bundle_repo.repo_head_commit_epoch > 0);
    assert_eq!(bundle_repo.repo_head_commit_message, "Initial commit");
}

#[test]
fn test_try_read_from_root_with_branch_override() {
    let root = tempfile::tempdir()
        .expect("failed to create temp directory")
        .into_path();
    setup_repo_with_commit(&root).expect("failed to setup repo");
    let branch = "other-branch";
    let bundle_repo = BundleRepo::try_read_from_root(
        Some(root.to_str().unwrap().to_string()),
        None,
        None,
        Some(branch.to_string()),
        None,
    );

    assert!(bundle_repo.is_ok());
    let bundle_repo = bundle_repo.unwrap();
    assert_eq!(bundle_repo.repo_root, root.to_str().unwrap());
    assert_eq!(
        bundle_repo.repo,
        Repo {
            host: "github.com".to_string(),
            owner: "trunk-io".to_string(),
            name: "analytics-cli".to_string(),
        }
    );
    assert_eq!(bundle_repo.repo_url, TEST_ORIGIN);
    assert_eq!(bundle_repo.repo_head_branch, branch);
    assert_eq!(bundle_repo.repo_head_sha.len(), 40);
    assert!(bundle_repo.repo_head_commit_epoch > 0);
    assert_eq!(bundle_repo.repo_head_commit_message, "Initial commit");
}

#[test]
fn test_try_read_from_root_with_time_override() {
    let root = tempfile::tempdir()
        .expect("failed to create temp directory")
        .into_path();
    setup_repo_with_commit(&root).expect("failed to setup repo");
    let epoch = "123";
    let bundle_repo = BundleRepo::try_read_from_root(
        Some(root.to_str().unwrap().to_string()),
        None,
        None,
        None,
        Some(epoch.to_string()),
    );

    assert!(bundle_repo.is_ok());
    let bundle_repo = bundle_repo.unwrap();
    assert_eq!(bundle_repo.repo_root, root.to_str().unwrap());
    assert_eq!(
        bundle_repo.repo,
        Repo {
            host: "github.com".to_string(),
            owner: "trunk-io".to_string(),
            name: "analytics-cli".to_string(),
        }
    );
    assert_eq!(bundle_repo.repo_url, TEST_ORIGIN);
    assert_eq!(
        bundle_repo.repo_head_branch,
        format!("refs/heads/{}", TEST_BRANCH)
    );
    assert_eq!(bundle_repo.repo_head_sha.len(), 40);
    assert_eq!(bundle_repo.repo_head_commit_epoch, 123);
    assert_eq!(bundle_repo.repo_head_commit_message, "Initial commit");
}
