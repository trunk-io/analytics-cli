def test_parse_meta_from_tarball():
    from context_py import codeowners_parse

    codeowners_text = """* @trunk/test"""
    codeowners = codeowners_parse(str.encode(codeowners_text))

    assert codeowners != None
