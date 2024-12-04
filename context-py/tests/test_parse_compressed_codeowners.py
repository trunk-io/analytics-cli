def test_parse_meta_from_tarball():
    from context_py import codeowners_parse

    codeowners_text = b"""* @trunk/test"""
    codeowners = codeowners_parse(codeowners_text)

    assert codeowners is not None
