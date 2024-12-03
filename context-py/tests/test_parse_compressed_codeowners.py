def test_parse_meta_from_tarball():
    import io
    import json
    import tarfile
    import tempfile

    import zstandard as zstd
    from botocore.response import StreamingBody

    from context_py import codeowners_parse

    codeowners_text = """* @trunk/test"""
    owners = codeowners_parse(str.encode(codeowners_text))

    assert owners != None
