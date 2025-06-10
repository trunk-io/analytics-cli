def test_parse_meta_valid():
    import json
    import typing as PT

    from context_py import TestRunnerReportStatus, parse_meta

    resolved_time_epoch_ms = 1749505703092
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

    assert len(bundle_meta.base_props.file_sets) == 4
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


def test_parse_meta_invalid():
    import json

    from context_py import parse_meta
    from pytest import raises

    invalid_meta = {"bad": "data"}

    encoded_meta = json.dumps(invalid_meta).encode()
    with raises(Exception) as excinfo:
        _ = parse_meta(encoded_meta)

    assert "missing field `version` at" in str(excinfo.value)
