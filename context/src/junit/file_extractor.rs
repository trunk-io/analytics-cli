use std::fs;
use std::path::Path;

use quick_junit::{TestCase, XmlString};
use walkdir::{DirEntry, WalkDir};

use super::parser::extra_attrs;
use crate::repo::BundleRepo;

fn not_hidden(entry: &DirEntry) -> bool {
    entry
        .file_name()
        .to_str()
        .map(|file_string| !file_string.starts_with('.'))
        .unwrap_or(true)
}

fn contains_test(path: &String, test_xml_name: &XmlString) -> bool {
    let test_name = test_xml_name.as_str();
    // Frameworks like vitest handle tests with an it() call inside a describe() call by
    // using "{describe_name} > {it_name}" as the test name, so we split on that in order
    // to get substrings that we can search for.
    let mut test_parts = test_name.split(" > ");
    fs::read_to_string(Path::new(path))
        .ok()
        .map(|text| {
            let has_full_name = text.contains(test_name);
            let has_name_splits = test_parts.all(|p| text.contains(p));
            has_full_name || has_name_splits
        })
        .unwrap_or(false)
}

fn file_containing_tests(file_paths: Vec<String>, test_name: &XmlString) -> Option<String> {
    let mut matching_paths = file_paths
        .iter()
        .filter(|path| contains_test(path, test_name));
    let first_match = matching_paths.next();
    let another_match = matching_paths.next();
    match (first_match, another_match) {
        (None, _) => None,
        (Some(only_match), None) => Some((*only_match).clone()),
        (_, _) => None,
    }
}

// None if is not a file or file does not exist, Some(absolute path) if it does exist in the root
fn convert_to_absolute(
    initial: &XmlString,
    repo: &BundleRepo,
    test_name: &XmlString,
) -> Option<String> {
    let initial_str = String::from(initial.as_str());
    let path = Path::new(&initial_str);
    let repo_root_path = Path::new(&repo.repo_root);
    if path.is_absolute() {
        path.to_str().map(String::from)
    } else if repo_root_path.is_absolute() && repo_root_path.exists() {
        let mut walk = WalkDir::new(repo.repo_root.clone())
            .into_iter()
            .filter_entry(not_hidden)
            .filter_map(|result| {
                if let Ok(entry) = result {
                    if entry.path().ends_with(path) {
                        entry.path().as_os_str().to_str().map(String::from).clone()
                    } else {
                        None
                    }
                } else {
                    None
                }
            });
        let first_match = walk.next();
        let another_match = walk.next();
        match (first_match, another_match) {
            (None, _) => None,
            (Some(only_match), None) => Some(only_match),
            (Some(first_match), Some(second_match)) => file_containing_tests(
                [vec![first_match, second_match], walk.collect()].concat(),
                test_name,
            ),
        }
    } else if path
        .file_name()
        .iter()
        .flat_map(|os| os.to_str())
        .all(|name| name.contains('.'))
    {
        Some(initial_str)
    } else {
        None
    }
}

fn validate_as_filename(initial: &XmlString) -> Option<String> {
    let initial_str = String::from(initial.as_str());
    let path = Path::new(&initial_str);
    if path.extension().is_some() {
        Some(initial_str)
    } else {
        None
    }
}

pub fn filename_for_test_case(test_case: &TestCase) -> String {
    test_case
        .extra
        .get(extra_attrs::FILE)
        .or(test_case.extra.get(extra_attrs::FILEPATH))
        .or(test_case.classname.as_ref())
        .iter()
        .flat_map(|s| validate_as_filename(s))
        .next()
        .unwrap_or_default()
}

pub fn detected_file_for_test_case(test_case: &TestCase, repo: &BundleRepo) -> String {
    test_case
        .extra
        .get(extra_attrs::FILE)
        .or(test_case.extra.get(extra_attrs::FILEPATH))
        .or(test_case.classname.as_ref())
        .iter()
        .flat_map(|s| convert_to_absolute(s, repo, &test_case.name))
        .next()
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;

    use quick_junit::XmlString;
    use tempfile::{tempdir, TempDir};

    use super::*;
    use crate::repo::RepoUrlParts;

    fn stringify(fp: PathBuf) -> String {
        String::from(fp.as_os_str().to_str().unwrap())
    }

    fn bundle_repo(dir: &Path) -> BundleRepo {
        BundleRepo {
            repo: RepoUrlParts::default(),
            repo_root: String::from(dir.as_os_str().to_str().unwrap()),
            repo_url: String::from(""),
            repo_head_sha: String::from(""),
            repo_head_sha_short: None,
            repo_head_branch: String::from(""),
            repo_head_commit_epoch: 0,
            repo_head_commit_message: String::from(""),
            repo_head_author_name: String::from(""),
            repo_head_author_email: String::from(""),
        }
    }

    #[test]
    fn test_contains_test_if_unsplit_test_name_present() {
        let temp_dir = tempdir().unwrap();
        let file_path = temp_dir.as_ref().join("match.txt");
        let text = r#"
            describe("description", {
                it("my_super_good_test", {
                })
            })
        "#;
        fs::write(file_path.clone(), text).unwrap();
        let actual = contains_test(&stringify(file_path), &XmlString::new("my_super_good_test"));
        assert!(actual);
    }

    #[test]
    fn test_contains_test_if_split_test_name_present() {
        let temp_dir = tempdir().unwrap();
        let file_path = temp_dir.as_ref().join("match.txt");
        let text = r#"
            describe("description", {
                it("my_super_good_test", {
                })
            })
        "#;
        fs::write(file_path.clone(), text).unwrap();
        let actual = contains_test(
            &stringify(file_path),
            &XmlString::new("description > my_super_good_test"),
        );
        assert!(actual);
    }

    #[test]
    fn test_contains_test_if_part_of_split_test_name_present() {
        let temp_dir = tempdir().unwrap();
        let file_path = temp_dir.as_ref().join("match.txt");
        let text = r#"
            it("my_super_good_test", {
            })
        "#;
        fs::write(file_path.clone(), text).unwrap();
        let actual = contains_test(
            &stringify(file_path),
            &XmlString::new("description > my_super_good_test"),
        );
        assert!(!actual);
    }

    #[test]
    fn test_contains_test_if_test_name_not_present() {
        let temp_dir = tempdir().unwrap();
        let file_path = temp_dir.as_ref().join("match.txt");
        let text = r#"
            describe("description", {
                it("my_super_good_test", {
                })
            })
        "#;
        fs::write(file_path.clone(), text).unwrap();
        let actual = contains_test(&stringify(file_path), &XmlString::new("totally_different"));
        assert!(!actual);
    }

    #[test]
    fn test_file_containing_test_if_none_contains_test() {
        let temp_dir = tempdir().unwrap();
        let file_path_1 = temp_dir.as_ref().join("file_1.txt");
        let text_1 = r#"it("test_1")"#;
        fs::write(file_path_1.clone(), text_1).unwrap();
        let file_path_2 = temp_dir.as_ref().join("file_2.txt");
        let text_2 = r#"it("test_2")"#;
        fs::write(file_path_2.clone(), text_2).unwrap();
        let file_path_3 = temp_dir.as_ref().join("file_3.txt");
        let text_3 = r#"it("test_3")"#;
        fs::write(file_path_3.clone(), text_3).unwrap();
        let actual = file_containing_tests(
            vec![
                stringify(file_path_1),
                stringify(file_path_2),
                stringify(file_path_3),
            ],
            &XmlString::new("totally_different"),
        );
        assert_eq!(actual, None);
    }

    #[test]
    fn test_file_containing_test_if_one_contains_test() {
        let temp_dir = tempdir().unwrap();
        let file_path_1 = temp_dir.as_ref().join("file_1.txt");
        let text_1 = r#"it("test_1")"#;
        fs::write(file_path_1.clone(), text_1).unwrap();
        let file_path_2 = temp_dir.as_ref().join("file_2.txt");
        let text_2 = r#"it("test_2")"#;
        fs::write(file_path_2.clone(), text_2).unwrap();
        let file_path_3 = temp_dir.as_ref().join("file_3.txt");
        let text_3 = r#"it("test_3")"#;
        fs::write(file_path_3.clone(), text_3).unwrap();
        let actual = file_containing_tests(
            vec![
                stringify(file_path_1),
                stringify(file_path_2.clone()),
                stringify(file_path_3),
            ],
            &XmlString::new("test_2"),
        );
        assert_eq!(actual, Some(stringify(file_path_2)));
    }

    #[test]
    fn test_file_containing_test_if_multiple_contain_test() {
        let temp_dir = tempdir().unwrap();
        let file_path_1 = temp_dir.as_ref().join("file_1.txt");
        let text_1 = r#"it("common_test")"#;
        fs::write(file_path_1.clone(), text_1).unwrap();
        let file_path_2 = temp_dir.as_ref().join("file_2.txt");
        let text_2 = r#"it("common_test")"#;
        fs::write(file_path_2.clone(), text_2).unwrap();
        let file_path_3 = temp_dir.as_ref().join("file_3.txt");
        let text_3 = r#"it("test_3")"#;
        fs::write(file_path_3.clone(), text_3).unwrap();
        let actual = file_containing_tests(
            vec![
                stringify(file_path_1),
                stringify(file_path_2),
                stringify(file_path_3),
            ],
            &XmlString::new("common_test"),
        );
        assert_eq!(actual, None);
    }

    #[test]
    fn test_convert_to_absolute_when_no_repo_root() {
        let temp_dir = TempDir::with_prefix("not-hidden").unwrap();
        let file_path = temp_dir.as_ref().join("test.txt");
        let text = r#"it("test")"#;
        fs::write(file_path.clone(), text).unwrap();
        let actual = convert_to_absolute(
            &XmlString::new("test.txt"),
            &BundleRepo::default(),
            &XmlString::new("test"),
        );
        assert_eq!(actual, Some(String::from("test.txt")));
    }

    #[test]
    fn test_convert_to_absolute_when_already_absolute() {
        let temp_dir = TempDir::with_prefix("not-hidden").unwrap();
        let file_path = temp_dir.as_ref().join("test.txt");
        let text = r#"it("test")"#;
        fs::write(file_path.clone(), text).unwrap();
        let actual = convert_to_absolute(
            &XmlString::new(stringify(file_path.clone())),
            &bundle_repo(temp_dir.as_ref()),
            &XmlString::new("test"),
        );
        assert_eq!(actual, Some(stringify(file_path)));
    }

    #[test]
    fn test_convert_to_absolute_when_no_file_matches() {
        let temp_dir = TempDir::with_prefix("not-hidden").unwrap();
        let file_path_1 = temp_dir.as_ref().join("file_1.txt");
        let text_1 = r#"it("test_1")"#;
        fs::write(file_path_1.clone(), text_1).unwrap();
        let file_path_2 = temp_dir.as_ref().join("file_2.txt");
        let text_2 = r#"it("test_2")"#;
        fs::write(file_path_2.clone(), text_2).unwrap();
        let file_path_3 = temp_dir.as_ref().join("file_3.txt");
        let text_3 = r#"it("test_3")"#;
        fs::write(file_path_3.clone(), text_3).unwrap();
        let actual = convert_to_absolute(
            &XmlString::new("not_a_test.txt"),
            &bundle_repo(temp_dir.as_ref()),
            &XmlString::new("test"),
        );
        assert_eq!(actual, None);
    }

    #[test]
    fn test_convert_to_absolute_when_one_file_matches() {
        let temp_dir = TempDir::with_prefix("not-hidden").unwrap();
        let inner_dir = "inner_dir";
        fs::create_dir(temp_dir.as_ref().join(inner_dir)).unwrap();
        let file_path_1 = temp_dir.as_ref().join(inner_dir).join("file_1.txt");
        let text_1 = r#"it("test_1")"#;
        fs::write(file_path_1.clone(), text_1).unwrap();
        let file_path_2 = temp_dir.as_ref().join(inner_dir).join("file_2.txt");
        let text_2 = r#"it("test_2")"#;
        fs::write(file_path_2.clone(), text_2).unwrap();
        let file_path_3 = temp_dir.as_ref().join(inner_dir).join("file_3.txt");
        let text_3 = r#"it("test_3")"#;
        fs::write(file_path_3.clone(), text_3).unwrap();
        let actual = convert_to_absolute(
            &XmlString::new("file_1.txt"),
            &bundle_repo(temp_dir.as_ref()),
            &XmlString::new("test"),
        );
        assert_eq!(actual, Some(stringify(file_path_1)));
    }

    #[test]
    fn test_convert_to_absolute_when_many_files_match_and_none_contain() {
        let temp_dir = TempDir::with_prefix("not-hidden").unwrap();
        let inner_dir = "inner_dir";
        let other_dir = "other_dir";
        fs::create_dir(temp_dir.as_ref().join(inner_dir)).unwrap();
        fs::create_dir(temp_dir.as_ref().join(other_dir)).unwrap();
        let file_path_1 = temp_dir.as_ref().join(inner_dir).join("file.txt");
        let text_1 = r#"it("test_1")"#;
        fs::write(file_path_1.clone(), text_1).unwrap();
        let file_path_2 = temp_dir.as_ref().join(other_dir).join("file.txt");
        let text_2 = r#"it("test_2")"#;
        fs::write(file_path_2.clone(), text_2).unwrap();
        let actual = convert_to_absolute(
            &XmlString::new("file.txt"),
            &bundle_repo(temp_dir.as_ref()),
            &XmlString::new("totally_different"),
        );
        assert_eq!(actual, None);
    }

    #[test]
    fn test_convert_to_absolute_when_many_files_match_and_one_contains() {
        let temp_dir = TempDir::with_prefix("not-hidden").unwrap();
        let inner_dir = "inner_dir";
        let other_dir = "other_dir";
        fs::create_dir(temp_dir.as_ref().join(inner_dir)).unwrap();
        fs::create_dir(temp_dir.as_ref().join(other_dir)).unwrap();
        let file_path_1 = temp_dir.as_ref().join(inner_dir).join("file.txt");
        let text_1 = r#"it("test_1")"#;
        fs::write(file_path_1.clone(), text_1).unwrap();
        let file_path_2 = temp_dir.as_ref().join(other_dir).join("file.txt");
        let text_2 = r#"it("test_2")"#;
        fs::write(file_path_2.clone(), text_2).unwrap();
        let actual = convert_to_absolute(
            &XmlString::new("file.txt"),
            &bundle_repo(temp_dir.as_ref()),
            &XmlString::new("test_1"),
        );
        assert_eq!(actual, Some(stringify(file_path_1)));
    }

    #[test]
    fn test_convert_to_absolute_when_only_match_is_in_hidden_directory() {
        let temp_dir = TempDir::with_prefix("not-hidden").unwrap();
        let hidden_dir = ".hidden";
        fs::create_dir(temp_dir.as_ref().join(hidden_dir)).unwrap();
        let file_path = temp_dir.as_ref().join(hidden_dir).join("test.txt");
        let text = r#"it("test")"#;
        fs::write(file_path.clone(), text).unwrap();
        let actual = convert_to_absolute(
            &XmlString::new("test.txt"),
            &bundle_repo(temp_dir.as_ref()),
            &XmlString::new("test"),
        );
        assert_eq!(actual, None);
    }
}
