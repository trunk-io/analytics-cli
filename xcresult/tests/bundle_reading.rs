//! What reading a bundle does to it on disk, and what it needs from the filesystem.
//!
//! `xcresulttool` migrates a bundle that predates `database.sqlite3` in place the first
//! time it is read. Both halves of that are pinned here: the format Xcode writes today is
//! unaffected, and the older one is a known limitation rather than something we pay to
//! avoid on every upload.

mod common;

use common::{ORG_URL_SLUG, REPO_FULL_NAME, entries, set_writable, unpack_archive_to_temp_dir};
use xcresult::xcresult::XCResult;

// The case that matters now: a current bundle already carries `database.sqlite3`, so there
// is nothing to migrate and it is read where it lies. CI hands out artifact mounts without
// write access, so this has to hold without copying the bundle first.
#[cfg(target_os = "macos")]
#[test]
fn a_read_only_modern_bundle_is_readable_and_is_not_written_to() {
    let temp_dir = unpack_archive_to_temp_dir("tests/data/test-inherited-test.xcresult.tar.gz");
    let bundle = temp_dir.as_ref().join("InheritedTest.xcresult");
    let before = entries(&bundle);
    assert!(
        before.iter().any(|entry| entry == "database.sqlite3"),
        "the fixture must already be migrated for this to prove anything"
    );

    set_writable(&bundle, false);
    let read_only_result = XCResult::new(
        bundle.to_str().unwrap(),
        ORG_URL_SLUG.clone(),
        REPO_FULL_NAME.clone(),
        false,
    )
    .map(|xcresult| xcresult.generate_junits().len());
    set_writable(&bundle, true);

    assert_eq!(
        read_only_result.map_err(|e| e.to_string()),
        Ok(1),
        "a read-only bundle in the current format must still be readable"
    );
    pretty_assertions::assert_eq!(
        entries(&bundle),
        before,
        "reading the bundle changed it on disk"
    );
}

// The accepted limitation. A bundle predating `database.sqlite3` is migrated in place on
// first read, so a read-only one cannot be read at all. Copying every bundle to avoid this
// costs more than the case is worth, and no bundle Xcode writes today is in this format.
// If this ever starts passing, the migration behaviour changed and the note in
// `xcresult.rs` is stale.
#[cfg(target_os = "macos")]
#[test]
fn a_read_only_legacy_bundle_cannot_be_read() {
    let temp_dir = unpack_archive_to_temp_dir("tests/data/test4.xcresult.tar.gz");
    let bundle = temp_dir.as_ref().join("test4.xcresult");
    assert!(
        !entries(&bundle)
            .iter()
            .any(|entry| entry == "database.sqlite3"),
        "the fixture must start un-migrated for this to prove anything"
    );

    set_writable(&bundle, false);
    let read_only_result = XCResult::new(
        bundle.to_str().unwrap(),
        ORG_URL_SLUG.clone(),
        REPO_FULL_NAME.clone(),
        false,
    )
    .map(|xcresult| xcresult.generate_junits().len());
    set_writable(&bundle, true);

    assert!(
        read_only_result.is_err(),
        "a read-only bundle in the older format is expected to fail the in-place migration"
    );
}
