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
