def test_env_parse_and_validate():
    from context_py import (
        CIPlatform,
        EnvValidationLevel,
        branch_class_to_string,
        env_parse,
        env_validate,
    )

    env_vars = {
        "GITHUB_ACTIONS": "true",
        "GITHUB_REF": "abc",
        "GITHUB_ACTOR": "Spikey",
        "GITHUB_REPOSITORY": "analytics-cli",
        "GITHUB_RUN_ID": "12345",
        "GITHUB_WORKFLOW": "test-workflow",
        "GITHUB_JOB": "test-job",
    }

    ci_info = env_parse(env_vars, ["main", "master"])
    assert ci_info is not None
    env_validation = env_validate(ci_info)

    assert ci_info.platform == CIPlatform.GitHubActions
    assert ci_info.workflow == "test-workflow"
    assert ci_info.job == "test-job"
    assert ci_info.branch_class is not None
    assert branch_class_to_string(ci_info.branch_class) == "NONE"
    assert env_validation.max_level() == EnvValidationLevel.SubOptimal
    assert [issue.error_message for issue in env_validation.issues_flat()] == [
        "CI info author email too short",
        "CI info author name too short",
        "CI info commit message too short",
        "CI info committer email too short",
        "CI info committer name too short",
        "CI info title too short",
    ], "\n" + "\n".join([issue.error_message for issue in env_validation.issues_flat()])


def test_env_parse_repo_fills_missing_values():
    """Test that repo fills in missing CI info values when use_uncloned_repo is None/False."""
    from context_py import env_parse  # type: ignore[reportUnknownVariableType]
    from context_py import BundleRepo, CIPlatform, RepoUrlParts

    env_vars = {
        "GITHUB_ACTIONS": "true",
        "GITHUB_REF": "refs/heads/feature-branch",
        "GITHUB_ACTOR": "env-actor",
        "GITHUB_REPOSITORY": "analytics-cli",
        "GITHUB_RUN_ID": "12345",
        "GITHUB_WORKFLOW": "test-workflow",
        "GITHUB_JOB": "test-job",
    }

    bundle_repo = BundleRepo(
        RepoUrlParts(host="github.com", owner="trunk-io", name="analytics-cli"),
        ".",
        "https://github.com/trunk-io/analytics-cli",
        "abc123def456",
        "abc123d",
        "repo-branch-name",
        1234567890,
        "This is a commit message from repo",
        "Repo Author Name",
        "repo-author@example.com",
        None,  # use_uncloned_repo is None
    )

    ci_info = env_parse(env_vars, ["main", "master"], repo=bundle_repo)
    assert ci_info is not None
    assert ci_info.platform == CIPlatform.GitHubActions
    # Branch should come from env vars (not repo) since use_uncloned_repo is None
    assert ci_info.branch == "feature-branch"
    # Actor should come from env vars
    assert ci_info.actor == "env-actor"
    # Commit message should come from repo since it's missing in env vars
    assert ci_info.commit_message == "This is a commit message from repo"
    # Author fields should come from repo since they're missing in env vars
    assert ci_info.author_name == "Repo Author Name"
    assert ci_info.author_email == "repo-author@example.com"
    assert ci_info.committer_name == "Repo Author Name"
    assert ci_info.committer_email == "repo-author@example.com"


def test_env_parse_repo_overrides_env_vars():
    """Test that repo overrides env vars when use_uncloned_repo is True."""
    from context_py import env_parse  # type: ignore[reportUnknownVariableType]
    from context_py import BundleRepo, CIPlatform, RepoUrlParts, branch_class_to_string

    env_vars = {
        "GITHUB_ACTIONS": "true",
        "GITHUB_REF": "refs/heads/feature-branch",
        "GITHUB_ACTOR": "env-actor",
        "GITHUB_REPOSITORY": "analytics-cli",
        "GITHUB_RUN_ID": "12345",
        "GITHUB_WORKFLOW": "test-workflow",
        "GITHUB_JOB": "test-job",
    }

    bundle_repo_override = BundleRepo(
        RepoUrlParts(host="github.com", owner="trunk-io", name="analytics-cli"),
        ".",
        "https://github.com/trunk-io/analytics-cli",
        "abc123def456",
        "abc123d",
        "repo-override-branch",
        1234567890,
        "Repo override commit message",
        "Repo Override Author",
        "repo-override@example.com",
        True,  # use_uncloned_repo is True
    )

    ci_info_override = env_parse(
        env_vars, ["main", "master"], repo=bundle_repo_override
    )
    assert ci_info_override is not None
    assert ci_info_override.platform == CIPlatform.GitHubActions
    # Branch should come from repo (overrides env var) when use_uncloned_repo is True
    assert ci_info_override.branch == "repo-override-branch"
    # Actor should come from repo (overrides env var)
    assert ci_info_override.actor == "repo-override@example.com"
    # Commit message should come from repo
    assert ci_info_override.commit_message == "Repo override commit message"
    # Author fields should come from repo
    assert ci_info_override.author_name == "Repo Override Author"
    assert ci_info_override.author_email == "repo-override@example.com"
    assert ci_info_override.committer_name == "Repo Override Author"
    assert ci_info_override.committer_email == "repo-override@example.com"
    # Branch class should be recalculated based on repo branch
    assert ci_info_override.branch_class is not None
    assert branch_class_to_string(ci_info_override.branch_class) == "NONE"


def test_env_parse_repo_fills_missing_when_env_vars_empty():
    """Test that repo fills in missing values when env vars are minimal/empty."""
    from context_py import env_parse  # type: ignore[reportUnknownVariableType]
    from context_py import BundleRepo, RepoUrlParts

    env_vars_minimal = {
        "GITHUB_ACTIONS": "true",
        "GITHUB_REPOSITORY": "analytics-cli",
        "GITHUB_RUN_ID": "12345",
    }

    bundle_repo = BundleRepo(
        RepoUrlParts(host="github.com", owner="trunk-io", name="analytics-cli"),
        ".",
        "https://github.com/trunk-io/analytics-cli",
        "abc123def456",
        "abc123d",
        "repo-branch-name",
        1234567890,
        "This is a commit message from repo",
        "Repo Author Name",
        "repo-author@example.com",
        None,  # use_uncloned_repo is None
    )

    ci_info_minimal = env_parse(env_vars_minimal, ["main", "master"], repo=bundle_repo)
    assert ci_info_minimal is not None
    # Branch should come from repo since it's missing in env vars
    assert ci_info_minimal.branch == "repo-branch-name"
    # Actor should come from repo since it's missing in env vars
    assert ci_info_minimal.actor == "repo-author@example.com"
    # Commit message should come from repo
    assert ci_info_minimal.commit_message == "This is a commit message from repo"
