use std::path::PathBuf;

use bundle::FileSetBuilder;

fn repo_with<const N: usize>(files: [&str; N]) -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().canonicalize().unwrap();
    for file in files {
        let path = root.join(file);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(path, "").unwrap();
    }
    (dir, root)
}

#[test]
fn a_file_two_globs_both_match_is_returned_once() {
    let (_dir, root) = repo_with(["junit.xml", "junit-swift-testing.xml"]);

    let files = FileSetBuilder::expand_globs(
        root.to_string_lossy(),
        &[String::from("junit*.xml"), String::from("junit.xml")],
    )
    .unwrap();

    // The second pattern owns nothing: the first already claimed the file it names.
    assert_eq!(
        files,
        vec![root.join("junit-swift-testing.xml"), root.join("junit.xml")]
    );
}

/// The dedupe is on the canonical path rather than the matched one, which is the only way a
/// link and its target are recognised as one file.
#[cfg(unix)]
#[test]
fn a_symlink_to_an_already_matched_file_is_not_returned_again() {
    let (_dir, root) = repo_with(["real.xml"]);
    std::os::unix::fs::symlink(root.join("real.xml"), root.join("link.xml")).unwrap();

    let files =
        FileSetBuilder::expand_globs(root.to_string_lossy(), &[String::from("*.xml")]).unwrap();

    assert_eq!(files, vec![root.join("real.xml")]);
}

/// A relative pattern is resolved against the repo root, not the working directory, so where
/// the uploader was invoked from does not change which files it finds.
#[test]
fn relative_globs_resolve_against_the_repo_root() {
    let (_dir, root) = repo_with(["reports/junit.xml"]);

    let files =
        FileSetBuilder::expand_globs(root.to_string_lossy(), &[String::from("reports/*.xml")])
            .unwrap();

    assert_eq!(files, vec![root.join("reports/junit.xml")]);
}

#[test]
fn a_glob_matching_nothing_owns_nothing() {
    let (_dir, root) = repo_with(["junit.xml"]);

    let files = FileSetBuilder::expand_globs(
        root.to_string_lossy(),
        &[String::from("nothing-here/*.xml")],
    )
    .unwrap();

    assert!(files.is_empty(), "got {files:?}");
}
