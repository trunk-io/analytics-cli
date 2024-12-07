def test_repo_validate():
    import math
    import time

    from context_py import BundleRepo, RepoUrlParts, RepoValidationLevel, repo_validate

    repo = RepoUrlParts(host="github", owner="trunk-io", name="analytics-cli")
    bundle_repo = BundleRepo(
        repo,
        ".",
        "https://github.com/trunk-io/analytics-cli",
        "abc",
        "abc",
        "main",
        math.floor(time.time()),
        "commit",
        "Spikey",
        "spikey@trunk.io",
    )

    repo_validation = repo_validate(bundle_repo)

    assert repo_validation.max_level() == RepoValidationLevel.Valid, "\n" + "\n".join(
        [issue.error_message for issue in repo_validation.issues_flat()]
    )
