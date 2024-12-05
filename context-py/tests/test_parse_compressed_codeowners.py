def test_parse_codeowners_from_tarball_github():
    from context_py import codeowners_parse

    codeowners_text = b"""* @trunk/test"""
    codeowners = codeowners_parse(codeowners_text)

    assert codeowners is not None

    github_owners = codeowners.get_github_owners()
    assert github_owners is not None
    assert github_owners.of("*.py") == ["@trunk/test"]


# TODO: Add tests for GITLAB
# Currently I am unable to force the parser to interpret the input as a gitlab codeowners file
