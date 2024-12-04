use chrono::DateTime;
use context::repo::{
    self,
    validator::{RepoValidationIssue, RepoValidationLevel, MAX_SHA_FIELD_LEN},
    BundleRepo, RepoUrlParts,
};
use test_utils::mock_git_repo::{setup_repo_with_commit, TEST_BRANCH, TEST_ORIGIN};

#[test]
fn test_try_read_from_root() {
    let root = tempfile::tempdir()
        .expect("failed to create temp directory")
        .into_path();
    setup_repo_with_commit(&root).expect("failed to setup repo");
    let bundle_repo = BundleRepo::new(
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
        RepoUrlParts {
            host: "github.com".to_string(),
            owner: "trunk-io".to_string(),
            name: "analytics-cli".to_string(),
        }
    );
    assert_eq!(
        bundle_repo.repo.repo_full_name(),
        "github.com/trunk-io/analytics-cli"
    );
    assert_eq!(bundle_repo.repo_url, TEST_ORIGIN);
    assert_eq!(
        bundle_repo.repo_head_branch,
        format!("refs/heads/{}", TEST_BRANCH)
    );
    assert_eq!(bundle_repo.repo_head_sha.len(), 40);
    assert!(bundle_repo.repo_head_commit_epoch > 0);
    assert_eq!(bundle_repo.repo_head_commit_message, "Initial commit");

    let repo_validation = repo::validator::validate(&bundle_repo);
    assert_eq!(repo_validation.max_level(), RepoValidationLevel::Valid);
    assert_eq!(repo_validation.issues(), &[]);
}

#[test]
fn test_try_read_from_root_with_url_override() {
    let root = tempfile::tempdir()
        .expect("failed to create temp directory")
        .into_path();
    setup_repo_with_commit(&root).expect("failed to setup repo");
    let origin_url = "https://host.com/owner/repo.git";
    let bundle_repo = BundleRepo::new(
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
        RepoUrlParts {
            host: "host.com".to_string(),
            owner: "owner".to_string(),
            name: "repo".to_string(),
        }
    );
    assert_eq!(bundle_repo.repo.repo_full_name(), "host.com/owner/repo");
    assert_eq!(bundle_repo.repo_url, origin_url);
    assert_eq!(
        bundle_repo.repo_head_branch,
        format!("refs/heads/{}", TEST_BRANCH)
    );
    assert_eq!(bundle_repo.repo_head_sha.len(), 40);
    assert!(bundle_repo.repo_head_commit_epoch > 0);
    assert_eq!(bundle_repo.repo_head_commit_message, "Initial commit");

    let repo_validation = repo::validator::validate(&bundle_repo);
    assert_eq!(repo_validation.max_level(), RepoValidationLevel::Valid);
    assert_eq!(repo_validation.issues(), &[]);
}

#[test]
fn test_try_read_from_root_with_sha_override() {
    let root = tempfile::tempdir()
        .expect("failed to create temp directory")
        .into_path();
    setup_repo_with_commit(&root).expect("failed to setup repo");
    let sha = "1234567890123456789012345678901234567890";
    let bundle_repo = BundleRepo::new(
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
        RepoUrlParts {
            host: "github.com".to_string(),
            owner: "trunk-io".to_string(),
            name: "analytics-cli".to_string(),
        }
    );
    assert_eq!(
        bundle_repo.repo.repo_full_name(),
        "github.com/trunk-io/analytics-cli"
    );
    assert_eq!(bundle_repo.repo_url, TEST_ORIGIN);
    assert_eq!(
        bundle_repo.repo_head_branch,
        format!("refs/heads/{}", TEST_BRANCH)
    );
    assert_eq!(bundle_repo.repo_head_sha, sha);
    assert!(bundle_repo.repo_head_commit_epoch > 0);
    assert_eq!(bundle_repo.repo_head_commit_message, "Initial commit");

    let repo_validation = repo::validator::validate(&bundle_repo);
    assert_eq!(repo_validation.max_level(), RepoValidationLevel::Valid);
    assert_eq!(repo_validation.issues(), &[]);
}

#[test]
fn test_try_read_from_root_with_branch_override() {
    let root = tempfile::tempdir()
        .expect("failed to create temp directory")
        .into_path();
    setup_repo_with_commit(&root).expect("failed to setup repo");
    let branch = "other-branch";
    let bundle_repo = BundleRepo::new(
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
        RepoUrlParts {
            host: "github.com".to_string(),
            owner: "trunk-io".to_string(),
            name: "analytics-cli".to_string(),
        }
    );
    assert_eq!(
        bundle_repo.repo.repo_full_name(),
        "github.com/trunk-io/analytics-cli"
    );
    assert_eq!(bundle_repo.repo_url, TEST_ORIGIN);
    assert_eq!(bundle_repo.repo_head_branch, branch);
    assert_eq!(bundle_repo.repo_head_sha.len(), 40);
    assert!(bundle_repo.repo_head_commit_epoch > 0);
    assert_eq!(bundle_repo.repo_head_commit_message, "Initial commit");

    let repo_validation = repo::validator::validate(&bundle_repo);
    assert_eq!(repo_validation.max_level(), RepoValidationLevel::Valid);
    assert_eq!(repo_validation.issues(), &[]);
}

#[test]
fn test_try_read_from_root_with_time_override() {
    let root = tempfile::tempdir()
        .expect("failed to create temp directory")
        .into_path();
    setup_repo_with_commit(&root).expect("failed to setup repo");
    let epoch = 123;
    let bundle_repo = BundleRepo::new(
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
        RepoUrlParts {
            host: "github.com".to_string(),
            owner: "trunk-io".to_string(),
            name: "analytics-cli".to_string(),
        }
    );
    assert_eq!(
        bundle_repo.repo.repo_full_name(),
        "github.com/trunk-io/analytics-cli"
    );
    assert_eq!(bundle_repo.repo_url, TEST_ORIGIN);
    assert_eq!(
        bundle_repo.repo_head_branch,
        format!("refs/heads/{}", TEST_BRANCH)
    );
    assert_eq!(bundle_repo.repo_head_sha.len(), 40);
    assert_eq!(bundle_repo.repo_head_commit_epoch, epoch);
    assert_eq!(bundle_repo.repo_head_commit_message, "Initial commit");

    let repo_validation = repo::validator::validate(&bundle_repo);
    assert_eq!(repo_validation.max_level(), RepoValidationLevel::SubOptimal);
    pretty_assertions::assert_eq!(
        repo_validation.issues(),
        &[RepoValidationIssue::SubOptimal(
            repo::validator::RepoValidationIssueSubOptimal::RepoCommitOldTimestamp(
                DateTime::from_timestamp(epoch, 0).unwrap()
            )
        )]
    );
}

#[test]
fn test_parse_ssh_urls() {
    let good_urls = &[
        (
            "git@github.com:user/repository.git",
            RepoUrlParts {
                host: "github.com".to_string(),
                owner: "user".to_string(),
                name: "repository".to_string(),
            },
        ),
        (
            "git@gitlab.com:group/project.git",
            RepoUrlParts {
                host: "gitlab.com".to_string(),
                owner: "group".to_string(),
                name: "project".to_string(),
            },
        ),
        (
            "git@bitbucket.org:team/repo.git",
            RepoUrlParts {
                host: "bitbucket.org".to_string(),
                owner: "team".to_string(),
                name: "repo".to_string(),
            },
        ),
        (
            "git@ssh.dev.azure.com:company/project",
            RepoUrlParts {
                host: "ssh.dev.azure.com".to_string(),
                owner: "company".to_string(),
                name: "project".to_string(),
            },
        ),
        (
            "git@sourceforge.net:owner/repo.git",
            RepoUrlParts {
                host: "sourceforge.net".to_string(),
                owner: "owner".to_string(),
                name: "repo".to_string(),
            },
        ),
    ];

    for (url, expected) in good_urls {
        let actual = RepoUrlParts::from_url(url).unwrap();
        assert_eq!(actual, *expected);
    }
}

#[test]
fn test_parse_https_urls() {
    let good_urls = &[
        (
            "https://github.com/username/repository.git",
            RepoUrlParts {
                host: "github.com".to_string(),
                owner: "username".to_string(),
                name: "repository".to_string(),
            },
        ),
        (
            "https://gitlab.com/group/project.git",
            RepoUrlParts {
                host: "gitlab.com".to_string(),
                owner: "group".to_string(),
                name: "project".to_string(),
            },
        ),
        (
            "https://bitbucket.org/teamname/reponame.git",
            RepoUrlParts {
                host: "bitbucket.org".to_string(),
                owner: "teamname".to_string(),
                name: "reponame".to_string(),
            },
        ),
        (
            "https://dev.azure.com/organization/project",
            RepoUrlParts {
                host: "dev.azure.com".to_string(),
                owner: "organization".to_string(),
                name: "project".to_string(),
            },
        ),
        (
            "https://gitlab.example.edu/groupname/project.git",
            RepoUrlParts {
                host: "gitlab.example.edu".to_string(),
                owner: "groupname".to_string(),
                name: "project".to_string(),
            },
        ),
    ];

    for (url, expected) in good_urls {
        let actual = RepoUrlParts::from_url(url).unwrap();
        assert_eq!(actual, *expected);
    }
}

#[test]
fn test_parse_git_urls() {
    let good_urls = &[
        (
            [
                "ssh://github.com/github/testrepo",
                "github.com/github/testrepo",
            ],
            RepoUrlParts {
                host: "github.com".to_string(),
                owner: "github".to_string(),
                name: "testrepo".to_string(),
            },
        ),
        (
            [
                "git://github.com/github/testrepo",
                "github.com/github/testrepo",
            ],
            RepoUrlParts {
                host: "github.com".to_string(),
                owner: "github".to_string(),
                name: "testrepo".to_string(),
            },
        ),
        (
            [
                "http://github.com/github/testrepo",
                "github.com/github/testrepo",
            ],
            RepoUrlParts {
                host: "github.com".to_string(),
                owner: "github".to_string(),
                name: "testrepo".to_string(),
            },
        ),
        (
            [
                "https://github.com/github/testrepo",
                "github.com/github/testrepo",
            ],
            RepoUrlParts {
                host: "github.com".to_string(),
                owner: "github".to_string(),
                name: "testrepo".to_string(),
            },
        ),
        (
            [
                "ftp://github.com/github/testrepo",
                "github.com/github/testrepo",
            ],
            RepoUrlParts {
                host: "github.com".to_string(),
                owner: "github".to_string(),
                name: "testrepo".to_string(),
            },
        ),
        (
            [
                "ftps://github.com/github/testrepo",
                "github.com/github/testrepo",
            ],
            RepoUrlParts {
                host: "github.com".to_string(),
                owner: "github".to_string(),
                name: "testrepo".to_string(),
            },
        ),
        (
            [
                "user@github.com:github/testrepo",
                "github.com/github/testrepo",
            ],
            RepoUrlParts {
                host: "github.com".to_string(),
                owner: "github".to_string(),
                name: "testrepo".to_string(),
            },
        ),
    ];

    let bad_urls = &[
        "sshy://github.com/github/testrepo",
        "ssh://github.com//testrepo",
        "ssh:/github.com//testrepo",
        "ssh:///testrepo",
        "ssh://github.com/github/",
    ];

    for ([url, repo_full_name], expected) in good_urls {
        let actual1 = RepoUrlParts::from_url(url).unwrap();
        assert_eq!(actual1, *expected);
        assert_eq!(actual1.repo_full_name(), *repo_full_name);
        let actual2 = RepoUrlParts::from_url(&(url.to_string() + ".git")).unwrap();
        assert_eq!(actual2, *expected);
        assert_eq!(actual2.repo_full_name(), *repo_full_name);
        let actual3 = RepoUrlParts::from_url(&(url.to_string() + ".git/")).unwrap();
        assert_eq!(actual3, *expected);
        assert_eq!(actual3.repo_full_name(), *repo_full_name);
    }

    for url in bad_urls {
        let actual = RepoUrlParts::from_url(url);
        assert!(actual.is_err());
    }
}

#[test]
fn test_parse_repo_shas_too_long() {
    let root = tempfile::tempdir()
        .expect("failed to create temp directory")
        .into_path();
    setup_repo_with_commit(&root).expect("failed to setup repo");
    let sha = "12345678901234567890123456789012345678900";
    let bundle_repo = BundleRepo::new(
        Some(root.to_str().unwrap().to_string()),
        None,
        Some(sha.to_string()),
        None,
        None,
    );

    assert!(bundle_repo.is_ok());
    let bundle_repo = bundle_repo.unwrap();

    let repo_validation = repo::validator::validate(&bundle_repo);
    assert_eq!(repo_validation.max_level(), RepoValidationLevel::Invalid);
    pretty_assertions::assert_eq!(
        repo_validation.issues(),
        &[RepoValidationIssue::Invalid(
            repo::validator::RepoValidationIssueInvalid::RepoShaTooLong(
                sha.to_string()[..MAX_SHA_FIELD_LEN].to_string()
            )
        )]
    );
}

#[test]
fn test_parse_repo_shas_too_short() {
    let root = tempfile::tempdir()
        .expect("failed to create temp directory")
        .into_path();
    setup_repo_with_commit(&root).expect("failed to setup repo");
    let blank_sha = "";
    let bundle_repo = BundleRepo::new(
        Some(root.to_str().unwrap().to_string()),
        None,
        Some(blank_sha.to_string()),
        None,
        None,
    );

    assert!(bundle_repo.is_ok());
    let bundle_repo = bundle_repo.unwrap();

    let repo_validation = repo::validator::validate(&bundle_repo);
    assert_eq!(repo_validation.max_level(), RepoValidationLevel::Invalid);
    pretty_assertions::assert_eq!(
        repo_validation.issues(),
        &[RepoValidationIssue::Invalid(
            repo::validator::RepoValidationIssueInvalid::RepoShaTooShort(blank_sha.to_string())
        )]
    );
}
