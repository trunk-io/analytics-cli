def test_branch_supplied_by_env():
    from context_py import (
        BindingsMetaContext,
        MetaValidation,
        MetaValidationLevel,
        meta_validate,
    )

    ci_info, bundle_repo = ci_info_and_bundle_repo()
    meta_context = BindingsMetaContext(ci_info, bundle_repo)
    meta_validation: MetaValidation = meta_validate(meta_context)

    assert meta_validation.max_level() == MetaValidationLevel.Valid, "\n" + "\n".join(
        [issue.error_message for issue in meta_validation.issues_flat()]
    )


def test_branch_supplied_by_repo():
    from context_py import (
        BindingsMetaContext,
        MetaValidation,
        MetaValidationLevel,
        meta_validate,
    )

    ci_info, bundle_repo = ci_info_and_bundle_repo({"GITHUB_REF": ""})
    meta_context = BindingsMetaContext(ci_info, bundle_repo)
    meta_validation: MetaValidation = meta_validate(meta_context)

    assert meta_validation.max_level() == MetaValidationLevel.Valid, "\n" + "\n".join(
        [issue.error_message for issue in meta_validation.issues_flat()]
    )


def test_no_branch_supplied():
    from context_py import (
        BindingsMetaContext,
        BundleRepo,
        MetaValidation,
        MetaValidationLevel,
        RepoUrlParts,
        meta_validate,
    )

    ci_info, _ = ci_info_and_bundle_repo({"GITHUB_REF": ""})
    bundle_repo = BundleRepo(
        RepoUrlParts(host="github", owner="trunk-io", name="analytics-cli"),
        ".",
        "https://github.com/trunk-io/analytics-cli",
        "abc",
        "abc",
        "",
        123,
        "commit",
        "Spikey",
        "spikey@trunk.io",
    )
    meta_context = BindingsMetaContext(ci_info, bundle_repo)
    meta_validation: MetaValidation = meta_validate(meta_context)

    assert meta_validation.max_level() == MetaValidationLevel.Invalid, "\n" + "\n".join(
        [issue.error_message for issue in meta_validation.issues_flat()]
    )


def ci_info_and_bundle_repo(env_overrides: dict[str, str] | None = None):
    from context_py import BundleRepo, CIInfo, RepoUrlParts, env_parse

    env_vars = {
        "GITHUB_ACTIONS": "true",
        "GITHUB_REF": "abc",
        "GITHUB_ACTOR": "Spikey",
        "GITHUB_REPOSITORY": "analytics-cli",
        "GITHUB_RUN_ID": "12345",
        "GITHUB_WORKFLOW": "test-workflow",
        "GITHUB_JOB": "test-job",
    }

    if env_overrides is not None:
        env_vars.update(env_overrides)

    ci_info: CIInfo | None = env_parse(env_vars)
    assert ci_info is not None

    bundle_repo = BundleRepo(
        RepoUrlParts(host="github", owner="trunk-io", name="analytics-cli"),
        ".",
        "https://github.com/trunk-io/analytics-cli",
        "abc",
        "abc",
        "main",
        123,
        "commit",
        "Spikey",
        "spikey@trunk.io",
    )

    return (ci_info, bundle_repo)
