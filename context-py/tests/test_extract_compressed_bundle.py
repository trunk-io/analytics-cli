def test_extract_files_from_tarball():
    import io
    import json
    import tarfile
    import tempfile

    import zstandard as zstd
    from botocore.response import StreamingBody
    from context_py import extract_files_from_tarball

    # trunk-ignore(pyright/reportUnknownVariableType)
    expected_meta = {
        "version": "1",
        "bundle_upload_id": "59c8ddd9-0a00-4b56-9eea-ef0d60ebcb79",
        "cli_version": "cargo=0.5.11 git=7e5824fa365c63a2d4b38020762be17f4edd6425 rustc=1.80.0-nightly",
        "org": "trunk",
        "repo": {
            "repo": {"host": "github.com", "owner": "trunk", "name": "test"},
            "repo_root": "/home/runner/work/trunk/test",
            "repo_url": "https://github.com/trunk/test",
            "repo_head_sha": "74518d470d8cfeb41408a85cf6097bb7f09ad902",
            "repo_head_branch": "refs/heads/main",
            "repo_head_commit_epoch": 1720652103,
            "repo_head_commit_message": "ci: add .deepsource.toml",
            "repo_head_author_name": "deepsource-io[bot]",
            "repo_head_author_email": "42547082+deepsource-io[bot]@users.noreply.github.com",
        },
        "tags": [],
        "file_sets": [
            {
                "file_set_type": "Junit",
                "files": [
                    {
                        "original_path": "/home/runner/work/trunk/test/file1.xml",
                        "path": "junit/0",
                        "last_modified_epoch_ns": 1721095230341044019,
                        "owners": [],
                        "team": "",
                    },
                    {
                        "original_path": "/home/runner/work/trunk/test/file2.xml",
                        "path": "junit/1",
                        "last_modified_epoch_ns": 1721095230341044019,
                        "owners": [],
                        "team": "",
                    },
                ],
                "glob": "junit.xml",
            }
        ],
        "envs": {
            "GITHUB_ACTION_REPOSITORY": "",
            "GITHUB_SERVER_URL": "https://github.com",
            "GITHUB_REPOSITORY": "trunk/test",
            "GITHUB_HEAD_REF": "",
            "GITHUB_RUN_ATTEMPT": "1",
            "GITHUB_BASE_REF": "",
            "RUNNER_ARCH": "X64",
            "RUNNER_OS": "Linux",
            "GITHUB_ACTOR": "trunk",
            "GITHUB_REF": "refs/heads/main",
            "GITHUB_EVENT_NAME": "schedule",
            "GITHUB_SHA": "74518d470d8cfeb41408a85cf6097bb7f09ad902",
            "GITHUB_WORKFLOW": "Sleeper Test",
            "GITHUB_REF_PROTECTED": "true",
            "CI": "true",
            "GITHUB_RUN_NUMBER": "4507",
            "GITHUB_ACTIONS": "true",
            "GITHUB_RUN_ID": "9949497745",
        },
        "upload_time_epoch": 1721095230,
        "test_command": None,
        "os_info": "linux",
        "group_is_quarantined": None,
        "quarantined_tests": [],
    }

    encoded_meta = json.dumps(expected_meta).encode()

    meta_tarball_compressed: bytes = b""
    with tempfile.TemporaryDirectory() as tempdir:
        meta_file_path = f"{tempdir}/meta.json"
        with open(meta_file_path, "wb") as f:
            f.write(encoded_meta)
        meta_tarball_path = f"{tempdir}/meta.tar"
        with tarfile.open(meta_tarball_path, "w") as tar:
            tar.add(meta_file_path, "meta.json")

            tar.add(meta_file_path, "CODEOWNERS")
            tar.add(meta_file_path, "junit/0")
            tar.add(meta_file_path, "junit/1")
        with open(meta_tarball_path, "rb") as f:
            encoder = zstd.ZstdCompressor(level=6)
            meta_tarball_compressed = encoder.compress(f.read())

    raw_stream = StreamingBody(
        io.BytesIO(meta_tarball_compressed), len(meta_tarball_compressed)
    )

    bundled_files = extract_files_from_tarball(raw_stream)

    # assert raw_stream.tell() == len(meta_tarball_compressed)

    versioned_bundle = bundled_files.meta
    assert versioned_bundle.get_v0_5_34() is None
    assert versioned_bundle.get_v0_6_2() is None

    bundle_meta = versioned_bundle.get_v0_5_29()
    assert bundle_meta.base_props.bundle_upload_id == expected_meta["bundle_upload_id"]

    codeowners = bundled_files.codeowners
    assert codeowners is not None and len(codeowners) == len(encoded_meta)

    files = bundled_files.files
    assert len(files) == 2
    assert files[0].file.path == "junit/0"
    assert files[0].file.original_path == "/home/runner/work/trunk/test/file1.xml"
    assert len(files[0].buffer) == len(encoded_meta)
    assert files[1].file.path == "junit/1"
    assert files[1].file.original_path == "/home/runner/work/trunk/test/file2.xml"
    assert len(files[1].buffer) == len(encoded_meta)
