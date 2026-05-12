def test_parse_meta_valid():
    import json
    import typing as PT

    from context_py import TestRunnerReportStatus, parse_meta

    resolved_time_epoch_ms = 1749505703092
    resolved_label = "//trunk/test:test"
    valid_meta: PT.Dict[str, PT.Any] = {
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
            },
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
                "resolved_status": None,
            },
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
                "resolved_status": "Passed",
            },
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
                "resolved_status": "Passed",
                "resolved_start_time_epoch_ms": resolved_time_epoch_ms,
                "resolved_end_time_epoch_ms": resolved_time_epoch_ms,
            },
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
                "resolved_status": "Passed",
                "resolved_start_time_epoch_ms": resolved_time_epoch_ms,
                "resolved_end_time_epoch_ms": resolved_time_epoch_ms,
                "resolved_label": resolved_label,
            },
        ],
        "envs": {},
        "upload_time_epoch": 1721095230,
        "test_command": None,
        "os_info": "linux",
        "group_is_quarantined": None,
        "quarantined_tests": [],
    }

    encoded_meta = json.dumps(valid_meta).encode()
    versioned_bundle = parse_meta(encoded_meta)
    assert versioned_bundle.get_v0_5_34() is None
    assert versioned_bundle.get_v0_6_2() is None

    bundle_meta = versioned_bundle.get_v0_5_29()
    assert bundle_meta.base_props.bundle_upload_id == valid_meta["bundle_upload_id"]

    assert len(bundle_meta.base_props.file_sets) == 5
    assert bundle_meta.base_props.file_sets[0].test_runner_report is None
    assert bundle_meta.base_props.file_sets[1].test_runner_report is None
    assert bundle_meta.base_props.file_sets[2].test_runner_report is not None
    assert (
        bundle_meta.base_props.file_sets[2].test_runner_report.resolved_status
        == TestRunnerReportStatus.Passed
    )
    assert (
        bundle_meta.base_props.file_sets[
            2
        ].test_runner_report.resolved_start_time_epoch_ms.timestamp()
        == 0
    )
    assert (
        bundle_meta.base_props.file_sets[
            2
        ].test_runner_report.resolved_end_time_epoch_ms.timestamp()
        == 0
    )
    assert bundle_meta.base_props.file_sets[3].test_runner_report is not None
    assert (
        bundle_meta.base_props.file_sets[3].test_runner_report.resolved_status
        == TestRunnerReportStatus.Passed
    )
    assert (
        bundle_meta.base_props.file_sets[
            3
        ].test_runner_report.resolved_start_time_epoch_ms.timestamp()
        * 1000
        == resolved_time_epoch_ms
    )
    assert (
        bundle_meta.base_props.file_sets[
            3
        ].test_runner_report.resolved_end_time_epoch_ms.timestamp()
        * 1000
        == resolved_time_epoch_ms
    )
    assert bundle_meta.base_props.file_sets[4].test_runner_report is not None
    assert (
        bundle_meta.base_props.file_sets[4].test_runner_report.resolved_label
        == resolved_label
    )


def test_parse_meta_invalid():
    import json

    from context_py import parse_meta
    from pytest import raises

    invalid_meta = {"bad": "data"}

    encoded_meta = json.dumps(invalid_meta).encode()
    with raises(Exception) as excinfo:
        _ = parse_meta(encoded_meta)

    assert "missing field `version` at" in str(excinfo.value)


def test_parse_and_dump_meta_roundtrip():
    import json
    import typing as PT

    from context_py import parse_meta

    valid_meta: dict[str, PT.Any] = {
        "bundle_upload_id": "59c8ddd9-0a00-4b56-9eea-ef0d60ebcb79",
        "cli_version": "cargo=0.5.11 git=7e5824fa365c63a2d4b38020762be17f4edd6425 rustc=1.80.0-nightly",
        "codeowners": None,
        "envs": {},
        "file_sets": [
            {
                "file_set_type": "Junit",
                "files": [
                    {
                        # serde_json doesn't support u128 in #[serde(flatten)] yet
                        "last_modified_epoch_ns": 0,
                        "original_path": "/home/runner/work/trunk/test/file1.xml",
                        "original_path_rel": None,
                        "owners": [],
                        "path": "junit/0",
                        "team": "",
                    },
                    {
                        # serde_json doesn't support u128 in #[serde(flatten)] yet
                        "last_modified_epoch_ns": 0,
                        "original_path": "/home/runner/work/trunk/test/file2.xml",
                        "original_path_rel": None,
                        "owners": [],
                        "path": "junit/1",
                        "team": "",
                    },
                ],
                "glob": "junit.xml",
            },
            {
                "file_set_type": "Junit",
                "files": [
                    {
                        # serde_json doesn't support u128 in #[serde(flatten)] yet
                        "last_modified_epoch_ns": 0,
                        "original_path": "/home/runner/work/trunk/test/file1.xml",
                        "original_path_rel": None,
                        "owners": [],
                        "path": "junit/0",
                        "team": "",
                    },
                    {
                        # serde_json doesn't support u128 in #[serde(flatten)] yet
                        "last_modified_epoch_ns": 0,
                        "original_path": "/home/runner/work/trunk/test/file2.xml",
                        "original_path_rel": None,
                        "owners": [],
                        "path": "junit/1",
                        "team": "",
                    },
                ],
                "glob": "junit.xml",
                "resolved_end_time_epoch_ms": 1749505703092,
                "resolved_start_time_epoch_ms": 1749505703092,
                "resolved_status": "Passed",
                "resolved_label": "//trunk/test:test",
            },
        ],
        "org": "trunk",
        "os_info": "linux",
        "quarantined_tests": [],
        "repo": {
            "repo": {"host": "github.com", "name": "test", "owner": "trunk"},
            "repo_head_author_email": "42547082+deepsource-io[bot]@users.noreply.github.com",
            "repo_head_author_name": "deepsource-io[bot]",
            "repo_head_branch": "refs/heads/main",
            "repo_head_commit_epoch": 1720652103,
            "repo_head_commit_message": "ci: add .deepsource.toml",
            "repo_head_sha": "74518d470d8cfeb41408a85cf6097bb7f09ad902",
            "repo_head_sha_short": None,
            "repo_root": "/home/runner/work/trunk/test",
            "repo_url": "https://github.com/trunk/test",
        },
        "schema": "V0_5_29",
        "tags": [],
        "test_command": None,
        "upload_time_epoch": 1721095230,
        "use_uncloned_repo": None,
        "version": "1",
    }

    valid_meta_str = json.dumps(valid_meta, sort_keys=True)
    bundle_meta = parse_meta(valid_meta_str.encode())
    assert (
        json.dumps(json.loads(bundle_meta.dump_json()), sort_keys=True)
        == valid_meta_str
    )


def test_parse_meta_version():
    import json
    import typing as PT

    from context_py import (
        BindingsVersionedBundle,
        BundleMetaV0_5_29,
        BundleMetaV0_5_34,
        BundleMetaV0_6_2,
        BundleMetaV0_6_3,
        BundleMetaV0_7_6,
        BundleMetaV0_7_7,
        parse_meta,
    )

    valid_meta: dict[str, PT.Any] = {
        "bundle_upload_id": "59c8ddd9-0a00-4b56-9eea-ef0d60ebcb79",
        "cli_version": "cargo=0.5.11 git=7e5824fa365c63a2d4b38020762be17f4edd6425 rustc=1.80.0-nightly",
        "codeowners": None,
        "envs": {},
        "file_sets": [],
        "org": "trunk",
        "os_info": "linux",
        "quarantined_tests": [],
        "repo": {
            "repo": {"host": "github.com", "name": "test", "owner": "trunk"},
            "repo_head_author_email": "42547082+deepsource-io[bot]@users.noreply.github.com",
            "repo_head_author_name": "deepsource-io[bot]",
            "repo_head_branch": "refs/heads/main",
            "repo_head_commit_epoch": 1720652103,
            "repo_head_commit_message": "ci: add .deepsource.toml",
            "repo_head_sha": "74518d470d8cfeb41408a85cf6097bb7f09ad902",
            "repo_head_sha_short": None,
            "repo_root": "/home/runner/work/trunk/test",
            "repo_url": "https://github.com/trunk/test",
        },
        "schema": "V0_5_29",
        "tags": [],
        "test_command": None,
        "upload_time_epoch": 1721095230,
        "version": "1",
    }

    bundle_meta = parse_meta(json.dumps(valid_meta).encode())
    assert isinstance(bundle_meta, BindingsVersionedBundle)
    assert isinstance(bundle_meta.get_v0_5_29(), BundleMetaV0_5_29 | None)
    assert isinstance(bundle_meta.get_v0_5_34(), BundleMetaV0_5_34 | None)
    assert isinstance(bundle_meta.get_v0_6_2(), BundleMetaV0_6_2 | None)
    assert isinstance(bundle_meta.get_v0_6_3(), BundleMetaV0_6_3 | None)
    assert isinstance(bundle_meta.get_v0_7_6(), BundleMetaV0_7_6 | None)
    assert isinstance(bundle_meta.get_v0_7_7(), BundleMetaV0_7_7 | None)
