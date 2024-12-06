def test_parse_codeowners_from_tarball_github():
    from context_py import codeowners_parse

    codeowners_text = b"""* @trunk/test"""
    codeowners = codeowners_parse(codeowners_text)

    assert codeowners is not None

    github_owners = codeowners.get_github_owners()
    assert github_owners is not None
    assert github_owners.of("*.py") == ["@trunk/test"]


def test_parse_codeowners_from_tarball_gitlab():
    from context_py import codeowners_parse

    codeowners_text = b"""
        [Documentation]
        ee/docs    @gl-docs
        docs       @gl-docs

        [Database]
        README.md  @gl-database
        model/db   @gl-database

        [dOcUmEnTaTiOn]
        README.md  @gl-docs

        [Two Words]
        README.md  @gl-database
        model/db   @gl-database

        [Double::Colon]
        README.md  @gl-database
        model/db   @gl-database

        [DefaultOwners] @config-owner @gl-docs
        README.md
        model/db

        [OverriddenOwners] @config-owner
        README.md  @gl-docs
        model/db   @gl-docs
    """
    codeowners = codeowners_parse(codeowners_text)

    assert codeowners is not None

    gitlab_owners = codeowners.get_gitlab_owners()
    assert gitlab_owners is not None
    assert gitlab_owners.of("README.md") == [
        "@gl-docs",
        "@gl-database",
        "@gl-database",
        "@gl-database",
        "@config-owner",
        "@gl-docs",
        "@gl-docs",
    ]
