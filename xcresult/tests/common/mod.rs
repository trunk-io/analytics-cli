//! Harness shared by the integration test binaries in this directory.
//!
//! Each file under `tests/` is its own crate, so anything two of them need lives here
//! rather than being written twice. `tests/common/mod.rs` is a module rather than
//! `tests/common.rs`, which cargo would build and run as a third test binary.

use std::{fs::File, path::Path};

use context::repo::RepoUrlParts;
use flate2::read::GzDecoder;
use lazy_static::lazy_static;
use tar::Archive;
use temp_testdir::TempDir;
#[cfg(target_os = "macos")]
use xcresult::{test_locations::Limits, xcresult::XCResult};

/// The bundles are checked in as tarballs, so a test reads one by unpacking it into a
/// temporary directory that is removed with the `TempDir`.
pub fn unpack_archive_to_temp_dir<T: AsRef<Path>>(archive_file_path: T) -> TempDir {
    let file = File::open(archive_file_path).unwrap();
    let decoder = GzDecoder::new(file);
    let mut archive = Archive::new(decoder);
    let temp_dir = TempDir::default();
    if let Err(e) = archive.unpack(temp_dir.as_ref()) {
        panic!("failed to unpack data.tar.gz: {}", e);
    }
    temp_dir
}

lazy_static! {
    pub static ref ORG_URL_SLUG: String = String::from("trunk");
    pub static ref REPO_FULL_NAME: String = RepoUrlParts {
        host: "github.com".to_string(),
        owner: "trunk-io".to_string(),
        name: "analytics-cli".to_string()
    }
    .repo_full_name();
}

/// Read a bundle the way `--use-experimental-xcresult-test-locations` does: each test's
/// file comes from where it is declared in `repo_root`, not from a failure.
#[cfg(target_os = "macos")]
pub fn declaration_report<T: AsRef<Path>, U: AsRef<Path>>(
    bundle_path: T,
    repo_root: U,
) -> quick_junit::Report {
    let xcresult = XCResult::new_with_declaration_locations(
        bundle_path.as_ref().to_str().unwrap(),
        ORG_URL_SLUG.clone(),
        REPO_FULL_NAME.clone(),
        repo_root.as_ref(),
        Limits::default(),
    )
    .expect("the declaration path reads the bundle");

    let mut junits = xcresult.generate_junits();
    assert_eq!(junits.len(), 1);
    junits.pop().unwrap()
}

/// Case name -> reported file. Only safe where case names are unique across the bundle;
/// where two suites run a case of the same name, key on the suite as well.
#[cfg(target_os = "macos")]
pub fn declaration_files<T: AsRef<Path>, U: AsRef<Path>>(
    bundle_path: T,
    repo_root: U,
) -> std::collections::HashMap<String, String> {
    declaration_report(bundle_path, repo_root)
        .test_suites
        .iter()
        .flat_map(|test_suite| test_suite.test_cases.iter())
        .map(|test_case| {
            let file = test_case
                .extra
                .iter()
                .find(|(key, _)| key.as_str() == "file")
                .map(|(_, value)| value.as_str().to_owned())
                .unwrap_or_default();
            (test_case.name.as_str().to_owned(), file)
        })
        .collect()
}

/// Assert the declaration flag changes a bundle's reported `file` and nothing else.
///
/// Everything except `file` — suite and case names, ids, statuses, timestamps — has to
/// come out identical on both paths, because the flag is only meant to move the file.
/// `repo_root` of `None` means an empty checkout, where nothing resolves.
#[cfg(target_os = "macos")]
pub fn assert_the_declaration_flag_moves_only_the_file(
    archive: &str,
    bundle: &str,
    repo_root: Option<&str>,
) {
    fn shape(xcresult: &xcresult::xcresult::XCResult) -> Vec<String> {
        let mut junits = xcresult.generate_junits();
        assert_eq!(junits.len(), 1);
        let junit = junits.pop().unwrap();
        let mut rows = Vec::new();
        for test_suite in &junit.test_suites {
            for test_case in &test_suite.test_cases {
                let id = test_case
                    .extra
                    .iter()
                    .find(|(key, _)| key.as_str() == "id")
                    .map(|(_, value)| value.as_str().to_owned())
                    .unwrap_or_default();
                rows.push(format!(
                    "{} | {} | {} | {:?} | {}",
                    test_suite.name.as_str(),
                    test_case.name.as_str(),
                    id,
                    test_case.status,
                    test_case
                        .timestamp
                        .map(|timestamp| timestamp.to_string())
                        .unwrap_or_default(),
                ));
            }
        }
        rows
    }

    let temp_dir = unpack_archive_to_temp_dir(archive);
    let bundle_path = temp_dir.as_ref().join(bundle);
    let path_str = bundle_path.to_str().unwrap();
    let empty_checkout = TempDir::default();
    let root: &Path = match repo_root {
        Some(repo_root) => Path::new(repo_root),
        None => empty_checkout.as_ref(),
    };

    let default = XCResult::new(
        path_str,
        ORG_URL_SLUG.clone(),
        REPO_FULL_NAME.clone(),
        false,
    )
    .expect("the default path reads the bundle");
    let declarations = XCResult::new_with_declaration_locations(
        path_str,
        ORG_URL_SLUG.clone(),
        REPO_FULL_NAME.clone(),
        root,
        Limits::default(),
    )
    .expect("the declaration path reads the bundle");

    let expected = shape(&default);
    assert!(!expected.is_empty(), "the bundle must have test cases");
    pretty_assertions::assert_eq!(shape(&declarations), expected);

    for file in declaration_files(&bundle_path, root).values() {
        assert!(
            !["/.build/", "/checkouts/", "/DerivedData/"]
                .iter()
                .any(|segment| file.contains(segment)),
            "the declaration path reported a vendored file: {file}"
        );
    }
}
