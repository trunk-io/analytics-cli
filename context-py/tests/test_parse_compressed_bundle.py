import io
import json
import os
import tarfile
import tempfile
import typing

import zstandard as zstd
from botocore.response import StreamingBody


def base_meta() -> typing.Any:
    return {
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


def create_stream_from_meta(
    expected_meta: typing.Any, internal_bin_filepath: typing.Optional[str] = None
) -> tuple[StreamingBody, bytes]:
    encoded_meta = json.dumps(expected_meta).encode()

    meta_tarball_compressed: bytes = b""
    with tempfile.TemporaryDirectory() as tempdir:
        meta_file_path = f"{tempdir}/meta.json"
        with open(meta_file_path, "wb") as f:
            f.write(encoded_meta)

        internal_bin_path: str | None = None
        if internal_bin_filepath is not None:
            internal_bin_current_path = os.path.join(
                os.path.dirname(__file__), internal_bin_filepath
            )
            with open(internal_bin_current_path, "rb") as f:
                internal_bin_data = f.read()

            internal_bin_path = f"{tempdir}/internal.bin"
            with open(internal_bin_path, "wb") as f:
                f.write(internal_bin_data)

        meta_tarball_path = f"{tempdir}/meta.tar"
        with tarfile.open(meta_tarball_path, "w") as tar:
            tar.add(meta_file_path, "meta.json")
            if internal_bin_path is not None:
                tar.add(internal_bin_path, "internal.bin")
            # Add some junk files into the tarball.
            for i in range(1000):
                tar.add(meta_file_path, f"junk{str(i)}")

        with open(meta_tarball_path, "rb") as f:
            encoder = zstd.ZstdCompressor(level=6)
            meta_tarball_compressed = encoder.compress(f.read())

    raw_stream = StreamingBody(
        io.BytesIO(meta_tarball_compressed), len(meta_tarball_compressed)
    )

    return (raw_stream, meta_tarball_compressed)


def test_parse_meta_from_tarball():
    from context_py import parse_meta_from_tarball

    expected_meta: typing.Any = base_meta()

    raw_stream, meta_tarball_compressed = create_stream_from_meta(expected_meta)

    versioned_bundle = parse_meta_from_tarball(raw_stream)

    # Check that the entire stream isn't read
    assert raw_stream.tell() < len(meta_tarball_compressed)

    assert versioned_bundle.get_v0_5_34() is None
    assert versioned_bundle.get_v0_6_2() is None

    bundle_meta = versioned_bundle.get_v0_5_29()
    assert bundle_meta.base_props.bundle_upload_id == expected_meta["bundle_upload_id"]


def test_parse_internal_bin_from_tarball():
    from context_py import parse_internal_bin_from_tarball

    raw_stream, _ = create_stream_from_meta(
        expected_meta=base_meta(),
        internal_bin_filepath="test_internal.bin",
    )

    internal_bin = parse_internal_bin_from_tarball(raw_stream)
    assert len(internal_bin) == 1

    bindings_report = internal_bin[0]
    assert len(bindings_report.test_suites) == 2
    assert bindings_report.tests == 13
    assert bindings_report.variant == ""

    test_suite_context_ruby = next(
        (
            suite
            for suite in bindings_report.test_suites
            if suite.name == "context_ruby"
        ),
        None,
    )
    assert test_suite_context_ruby is not None
    assert len(test_suite_context_ruby.test_cases) == 5

    test_suite_rspec_expectations = next(
        (
            suite
            for suite in bindings_report.test_suites
            if suite.name == "RSpec Expectations"
        ),
        None,
    )
    assert test_suite_rspec_expectations is not None
    assert len(test_suite_rspec_expectations.test_cases) == 8


def test_parse_internal_bin_with_variant_from_tarball():
    from context_py import parse_internal_bin_from_tarball

    raw_stream, _ = create_stream_from_meta(
        expected_meta=base_meta(),
        internal_bin_filepath="test_internal_with_variant.bin",
    )

    internal_bin = parse_internal_bin_from_tarball(raw_stream)
    assert len(internal_bin) == 1

    bindings_report = internal_bin[0]
    assert bindings_report.variant == "test-variant"


def test_parse_internal_bin_v2_from_tarball():
    from context_py import parse_internal_bin_from_tarball

    raw_stream, _ = create_stream_from_meta(
        expected_meta=base_meta(),
        internal_bin_filepath="test_internal_v2.bin",
    )

    internal_bin = parse_internal_bin_from_tarball(raw_stream)
    assert len(internal_bin) == 1

    bindings_report = internal_bin[0]
    assert len(bindings_report.test_suites) == 1
    assert bindings_report.tests == 1
    assert bindings_report.variant == ""
    assert bindings_report.bazel_build_information is not None
    assert (
        bindings_report.bazel_build_information.label
        == "//trunk/hello_world/cc:hello_test"
    )

    test_suite = next(
        (suite for suite in bindings_report.test_suites if suite.name == "HelloTest"),
        None,
    )
    assert test_suite is not None
    assert len(test_suite.test_cases) == 1
