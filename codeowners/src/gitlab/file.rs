use std::{io::BufRead, path::PathBuf};

use fancy_regex::Regex;
use indexmap::IndexMap;
use lazy_static::lazy_static;

use crate::gitlab::{ErrorType, ReferenceExtractor};

use super::{Entry, Error, Section, SectionParser};

pub type ParsedData = IndexMap<String, IndexMap<String, Entry>>;

/// Reference: https://gitlab.com/gitlab-org/gitlab/-/blob/master/ee/lib/gitlab/code_owners/file.rb
#[derive(Debug, Clone, Default, PartialEq)]
pub struct File {
    path: PathBuf,
    errors: Vec<Error>,
    parsed_data: ParsedData,
}

impl File {
    pub fn new<B: BufRead>(buf_read: B, path: Option<PathBuf>) -> Self {
        let mut file = Self {
            path: path.unwrap_or_default(),
            errors: Vec::new(),
            ..Default::default()
        };

        file.parsed_data = file.get_parsed_data(buf_read);

        file
    }

    // NOTE: Allow reference implementation methods for double-checking
    #[allow(dead_code)]
    pub fn parsed_data(&self) -> ParsedData {
        self.parsed_data.clone()
    }

    // NOTE: Allow reference implementation methods for double-checking
    #[allow(dead_code)]
    pub fn empty(&self) -> bool {
        self.parsed_data.values().all(|v| v.is_empty())
    }

    // NOTE: Allow reference implementation methods for double-checking
    #[allow(dead_code)]
    pub fn path(&self) -> PathBuf {
        self.path.clone()
    }

    // NOTE: Allow reference implementation methods for double-checking
    #[allow(dead_code)]
    pub fn sections(&self) -> Vec<String> {
        self.parsed_data.keys().cloned().collect()
    }

    // NOTE: Allow reference implementation methods for double-checking
    #[allow(dead_code)]
    pub fn optional_section(&self, section: String) -> bool {
        self.parsed_data
            .get(&section)
            .map(|patterns| patterns.values().all(|entry| entry.optional))
            .unwrap_or_default()
    }

    pub fn entries_for_path(&self, path: String) -> Vec<Entry> {
        let path = if path.starts_with('/') {
            path
        } else {
            format!("/{path}")
        };
        self.parsed_data
            .iter()
            .filter_map(|(_, section_entries)| {
                section_entries
                    .iter()
                    .rev()
                    .find(|(pattern, ..)| File::path_matches(pattern, &path))
                    .map(|(_, entry)| entry)
                    .cloned()
            })
            .collect()
    }

    pub fn valid(&self) -> bool {
        self.errors.is_empty()
    }

    pub fn errors(&self) -> Vec<Error> {
        self.errors.clone()
    }

    fn get_parsed_data<B: BufRead>(&mut self, blob: B) -> ParsedData {
        let mut current_section = Section::new(String::from(Section::DEFAULT), None, None, None);
        let mut parsed_sectional_data = ParsedData::new();
        parsed_sectional_data.insert(current_section.name.clone(), Default::default());

        for (i, possible_line) in blob.lines().enumerate() {
            let line_number = i;
            if let Ok(line) = possible_line {
                let line = line.trim();
                if File::skip(line) {
                    continue;
                }

                let mut section_parser =
                    SectionParser::new(String::from(line), &parsed_sectional_data);
                let parsed_section = section_parser.execute();

                if !section_parser.valid() {
                    section_parser.errors.iter().for_each(|error| {
                        self.add_error(error.to_string(), line_number);
                    });
                }

                if let Some(new_parsed_section) = parsed_section {
                    current_section = new_parsed_section;
                    parsed_sectional_data
                        .entry(current_section.name.clone())
                        .or_default();

                    continue;
                }

                self.parse_entry(
                    String::from(line),
                    &mut parsed_sectional_data,
                    &current_section,
                    line_number,
                );
            }
        }

        parsed_sectional_data
    }

    fn parse_entry(
        &mut self,
        line: String,
        parsed: &mut ParsedData,
        section: &Section,
        line_number: usize,
    ) {
        lazy_static! {
            static ref LINE_PARTITION: Regex = Regex::new(r"(?<!\\)\s+").unwrap();
        }
        let (pattern, entry_owners) = LINE_PARTITION
            .find(&line)
            .ok()
            .flatten()
            .map(|r#match| r#match.range())
            .map_or_else(
                || (line.clone(), String::default()),
                |range| {
                    let mut pattern = line.clone();
                    let mut separator_and_entry_owners = pattern.split_off(range.start);
                    let entry_owners =
                        separator_and_entry_owners.split_off(range.end - range.start);
                    (pattern, entry_owners)
                },
            );
        let normalized_pattern = File::normalize_pattern(pattern.clone());

        if !entry_owners.is_empty()
            && ReferenceExtractor::new(entry_owners.clone())
                .references()
                .is_empty()
        {
            self.add_error(ErrorType::InvalidEntryOwnerFormat.to_string(), line_number);
        }

        let owners = if !entry_owners.is_empty() {
            entry_owners
        } else {
            section.default_owners.clone()
        };

        if owners.is_empty() {
            self.add_error(ErrorType::MissingEntryOwner.to_string(), line_number);
        }

        parsed.entry(section.name.clone()).or_default().insert(
            normalized_pattern,
            Entry::new(
                pattern,
                owners,
                Some(section.name.clone()),
                Some(section.optional),
                Some(section.approvals),
            ),
        );
    }

    fn skip<T: AsRef<str>>(line: T) -> bool {
        line.as_ref().is_empty() || line.as_ref().starts_with('#')
    }

    fn normalize_pattern(pattern: String) -> String {
        if pattern == "*" {
            return String::from("/**/*");
        }

        lazy_static! {
            static ref POUND_ESCAPE: Regex = Regex::new(r"\A\\#").unwrap();
            static ref WHITESPACE_ESCAPE: Regex = Regex::new(r"\\\s+").unwrap();
        }

        let mut pattern = String::from(POUND_ESCAPE.replace(&pattern, "#"));
        pattern = String::from(WHITESPACE_ESCAPE.replace_all(&pattern, " "));

        if !pattern.starts_with('/') {
            pattern = format!("/**/{pattern}");
        }

        if pattern.ends_with('/') {
            pattern = format!("{pattern}**/*");
        }

        pattern
    }

    fn path_matches<T: AsRef<str>, U: AsRef<str>>(pattern: T, path: U) -> bool {
        glob::Pattern::new(pattern.as_ref())
            .ok()
            .map(|p| {
                p.matches_with(
                    path.as_ref(),
                    glob::MatchOptions {
                        case_sensitive: true,
                        require_literal_leading_dot: false,
                        require_literal_separator: true,
                    },
                )
            })
            .unwrap_or_default()
    }

    fn add_error(&mut self, message: String, line_number: usize) {
        self.errors
            .push(Error::new(message, line_number, self.path.clone()));
    }
}

/// Reference: https://gitlab.com/gitlab-org/gitlab/-/blob/master/ee/spec/lib/gitlab/code_owners/file_spec.rb
#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use lazy_static::lazy_static;

    use crate::gitlab::File;

    /// Reference: https://gitlab.com/gitlab-org/gitlab/-/blob/master/ee/spec/fixtures/codeowners_example
    const CODEOWNERS_EXAMPLE: &[u8] =
        include_bytes!("../../test_fixtures/gitlab/codeowners_example");

    /// Reference: https://gitlab.com/gitlab-org/gitlab/-/blob/master/ee/spec/fixtures/mixed_case_sectional_codeowners_example
    const MIXED_CASE_SECTIONAL_CODEOWNERS_EXAMPLE: &[u8] =
        include_bytes!("../../test_fixtures/gitlab/mixed_case_sectional_codeowners_example");

    /// Reference: https://gitlab.com/gitlab-org/gitlab/-/blob/master/ee/spec/fixtures/sectional_codeowners_example
    const SECTIONAL_CODEOWNERS_EXAMPLE: &[u8] =
        include_bytes!("../../test_fixtures/gitlab/sectional_codeowners_example");

    lazy_static! {
        static ref FILE: File = File::new(CODEOWNERS_EXAMPLE, Some(PathBuf::from("CODEOWNERS")));
        static ref FILE_MIXED_CASE_SECTIONAL_CODEOWNERS_EXAMPLE: File = File::new(
            MIXED_CASE_SECTIONAL_CODEOWNERS_EXAMPLE,
            Some(PathBuf::from("CODEOWNERS"))
        );
        static ref FILE_SECTIONAL_CODEOWNERS_EXAMPLE: File = File::new(
            SECTIONAL_CODEOWNERS_EXAMPLE,
            Some(PathBuf::from("CODEOWNERS"))
        );
    }

    fn owner_line<T: AsRef<str>>(pattern: T) -> String {
        FILE.parsed_data()
            .get("codeowners")
            .unwrap()
            .get(pattern.as_ref())
            .unwrap()
            .owner_line
            .clone()
    }

    mod parsed_data {
        mod when_codeowners_file_contains_no_sections {
            use crate::gitlab::file::tests::{owner_line, FILE};

            #[test]
            fn parses_all_the_required_lines() {
                let expected_patterns = vec![
                    "/**/*",
                    "/**/*.rb",
                    "/**/#file_with_pound.rb",
                    "/**/CODEOWNERS",
                    "/**/LICENSE",
                    "/docs/**/*",
                    "/docs/*",
                    "/**/lib/**/*",
                    "/config/**/*",
                    "/**/path with spaces/**/*",
                ];

                assert_eq!(
                    FILE.parsed_data()
                        .get("codeowners")
                        .unwrap()
                        .keys()
                        .collect::<Vec<&String>>(),
                    expected_patterns
                );
            }

            #[test]
            fn allows_usernames_and_emails() {
                let owner_line = owner_line("/**/LICENSE");
                assert!(owner_line.contains("legal"));
                assert!(owner_line.contains("janedoe@gitlab.com"))
            }
        }

        mod when_handling_a_sectional_codeowners_file {
            use crate::gitlab::{
                file::tests::{
                    FILE, FILE_MIXED_CASE_SECTIONAL_CODEOWNERS_EXAMPLE,
                    FILE_SECTIONAL_CODEOWNERS_EXAMPLE,
                },
                Section,
            };

            mod creates_expected_parsed_sectional_results {
                use std::{collections::HashSet, iter::FromIterator};

                use crate::gitlab::File;

                pub fn shared_examples(file: &File) {
                    is_a_hash_sorted_by_sections_without_duplicates(file);
                    assigns_the_correct_paths_to_each_section(file);
                    assigns_the_correct_owners_for_each_entry(file);
                }

                fn is_a_hash_sorted_by_sections_without_duplicates(file: &File) {
                    let data = file.parsed_data();

                    assert_eq!(data.keys().len(), 7);
                    assert_eq!(
                        data.keys().collect::<Vec<&String>>(),
                        vec![
                            "codeowners",
                            "Documentation",
                            "Database",
                            "Two Words",
                            "Double::Colon",
                            "DefaultOwners",
                            "OverriddenOwners"
                        ]
                    );
                }

                const CODEOWNERS_SECTION_PATHS: &[&str] = &[
                    "/**/*",
                    "/**/*.rb",
                    "/**/#file_with_pound.rb",
                    "/**/CODEOWNERS",
                    "/**/LICENSE",
                    "/docs/**/*",
                    "/docs/*",
                    "/**/lib/**/*",
                    "/config/**/*",
                    "/**/path with spaces/**/*",
                ];

                const CODEOWNERS_SECTION_OWNERS: &[&str] = &[
                    "@all-docs",
                    "@config-owner",
                    "@default-codeowner",
                    "@legal this does not match janedoe@gitlab.com",
                    "@lib-owner",
                    "@multiple @owners\t@tab-separated",
                    "@owner-file-with-pound",
                    "@root-docs",
                    "@ruby-owner",
                    "@space-owner",
                ];

                const WHERE: &[(&str, &[&str], &[&str])] = &[
                    (
                        "codeowners",
                        CODEOWNERS_SECTION_PATHS,
                        CODEOWNERS_SECTION_OWNERS,
                    ),
                    (
                        "Documentation",
                        &["/**/ee/docs", "/**/docs", "/**/README.md"],
                        &["@gl-docs"],
                    ),
                    (
                        "Database",
                        &["/**/README.md", "/**/model/db"],
                        &["@gl-database"],
                    ),
                    (
                        "Two Words",
                        &["/**/README.md", "/**/model/db"],
                        &["@gl-database"],
                    ),
                    (
                        "Double::Colon",
                        &["/**/README.md", "/**/model/db"],
                        &["@gl-database"],
                    ),
                    (
                        "DefaultOwners",
                        &["/**/README.md", "/**/model/db"],
                        &["@config-owner @gl-docs"],
                    ),
                    (
                        "OverriddenOwners",
                        &["/**/README.md", "/**/model/db"],
                        &["@gl-docs"],
                    ),
                ];

                fn assigns_the_correct_paths_to_each_section(file: &File) {
                    for (section, patterns, ..) in Vec::from(WHERE) {
                        assert_eq!(
                            file.parsed_data()
                                .get(section)
                                .unwrap()
                                .keys()
                                .collect::<Vec<&String>>(),
                            patterns
                        );
                        assert_eq!(
                            file.parsed_data()
                                .get(section)
                                .unwrap()
                                .values()
                                .find(|entry| { entry.section != section }),
                            None
                        );
                    }
                }

                fn assigns_the_correct_owners_for_each_entry(file: &File) {
                    for (section, _, owners) in Vec::from(WHERE) {
                        assert_eq!(
                            file.parsed_data()
                                .get(section)
                                .unwrap()
                                .values()
                                .map(|entry| { entry.owner_line.clone() })
                                .collect::<HashSet<String>>(),
                            HashSet::from_iter(
                                Vec::from_iter(owners.iter().cloned().map(String::from))
                                    .into_iter()
                            )
                        );
                    }
                }
            }

            #[test]
            fn populates_a_hash_with_a_single_default_section() {
                let data = FILE.parsed_data();

                assert_eq!(data.keys().len(), 1);
                assert_eq!(
                    data.keys().collect::<Vec<&String>>(),
                    vec![Section::DEFAULT]
                );
            }

            mod when_codeowners_file_contains_sections_at_the_middle_of_a_line {
                use lazy_static::lazy_static;

                use crate::gitlab::File;

                const FILE_CONTENT: &str = r#"
                    [Required]
                    *_spec.rb @gl-test

                    ^[Optional]
                    *_spec.rb @gl-test

                    Something before [Partially optional]
                    *.md @gl-docs

                    Additional content before ^[Another partially optional]
                    doc/* @gl-docs
                "#;

                lazy_static! {
                    static ref FILE: File = File::new(FILE_CONTENT.as_bytes(), None);
                }

                #[test]
                fn parses_only_sections_that_start_at_the_beginning_of_a_line() {
                    assert_eq!(
                        FILE.parsed_data().keys().collect::<Vec<&String>>(),
                        vec!["codeowners", "Required", "Optional"]
                    );
                }
            }

            #[test]
            fn when_codeowners_file_contains_multiple_sections() {
                creates_expected_parsed_sectional_results::shared_examples(
                    &FILE_SECTIONAL_CODEOWNERS_EXAMPLE,
                );
            }

            #[test]
            fn when_codeowners_file_contains_multiple_sections_with_mixed_case_names() {
                creates_expected_parsed_sectional_results::shared_examples(
                    &FILE_MIXED_CASE_SECTIONAL_CODEOWNERS_EXAMPLE,
                );
            }

            mod when_codeowners_file_contains_approvals_required {
                use lazy_static::lazy_static;

                use crate::gitlab::File;

                const FILE_CONTENT: &str = r#"
                    [Required][2]
                    *_spec.rb @gl-test

                    [Another required]
                    *_spec.rb @gl-test

                    [Required with default owners] @config-owner
                    *_spec.rb @gl-test

                    ^[Optional][2] @gl-docs @config-owner
                    *_spec.rb

                    [Required with non-numbers][q]
                    *_spec.rb @gl-test

                    ^[Optional with non-numbers][.] @not-matching
                    *_spec.rb @gl-test
                "#;

                lazy_static! {
                    static ref FILE: File = File::new(FILE_CONTENT.as_bytes(), None);
                }

                #[test]
                fn parses_the_approvals_required() {
                    let data = FILE.parsed_data();

                    let entry = data.get("Required").unwrap().get("/**/*_spec.rb").unwrap();
                    assert_eq!(entry.approvals_required, 2);
                    assert_eq!(entry.owner_line, "@gl-test");

                    let entry = data
                        .get("Another required")
                        .unwrap()
                        .get("/**/*_spec.rb")
                        .unwrap();
                    assert_eq!(entry.approvals_required, 0);
                    assert_eq!(entry.owner_line, "@gl-test");

                    let entry = data
                        .get("Required with default owners")
                        .unwrap()
                        .get("/**/*_spec.rb")
                        .unwrap();
                    assert_eq!(entry.approvals_required, 0);
                    assert_eq!(entry.owner_line, "@gl-test");

                    let entry = data.get("Optional").unwrap().get("/**/*_spec.rb").unwrap();
                    assert_eq!(entry.approvals_required, 2);
                    assert_eq!(entry.owner_line, "@gl-docs @config-owner");

                    let entry = data
                        .get("Required with non-numbers")
                        .unwrap()
                        .get("/**/*_spec.rb")
                        .unwrap();
                    assert_eq!(entry.approvals_required, 0);
                    assert_eq!(entry.owner_line, "@gl-test");

                    let entry = data
                        .get("Optional with non-numbers")
                        .unwrap()
                        .get("/**/*_spec.rb")
                        .unwrap();
                    assert_eq!(entry.approvals_required, 0);
                    assert_eq!(entry.owner_line, "@gl-test");
                }
            }
        }
    }

    mod empty {
        use crate::gitlab::{file::tests::FILE, File};

        #[test]
        fn is_not_empty() {
            assert!(!FILE.empty())
        }

        #[test]
        fn when_there_is_no_content() {
            let file = File::new("".as_bytes(), None);

            assert!(file.empty())
        }

        #[test]
        fn when_the_file_is_binary() {
            let file = File::new(&[][..], None);

            assert!(file.empty())
        }

        #[test]
        fn when_the_file_did_not_exist() {
            let file = File::new(&[][..], None);

            assert!(file.empty())
        }
    }

    mod path {
        mod when_the_blob_exists {
            use crate::gitlab::file::tests::FILE;

            #[test]
            fn returns_the_path_to_the_file() {
                assert_eq!(FILE.path.to_str(), Some("CODEOWNERS"));
            }
        }

        mod when_the_path_is_none {
            use crate::gitlab::file::File;

            #[test]
            fn returns_empty() {
                let file = File::new(&[][..], None);
                assert_eq!(file.path.to_str(), Some(""));
            }
        }
    }

    mod sections {
        mod when_codeowners_file_contains_sections {
            use lazy_static::lazy_static;

            use crate::gitlab::File;

            const FILE_CONTENT: &str = r#"
                *.rb @ruby-owner

                [Documentation]
                *.md @gl-docs

                [Test]
                *_spec.rb @gl-test

                [Documentation]
                doc/* @gl-docs
            "#;

            lazy_static! {
                static ref FILE: File = File::new(FILE_CONTENT.as_bytes(), None);
            }

            #[test]
            fn returns_unique_sections() {
                assert_eq!(FILE.sections(), vec!["codeowners", "Documentation", "Test"]);
            }
        }

        mod when_codeowners_file_is_missing {
            use crate::gitlab::File;

            #[test]
            fn returns_a_default_section() {
                let file = File::new(&[][..], None);

                assert_eq!(file.sections(), vec!["codeowners"]);
            }
        }
    }

    mod optional_section {
        use lazy_static::lazy_static;

        use crate::gitlab::File;

        const FILE_CONTENT: &str = r#"
            *.rb @ruby-owner

            [Required]
            *_spec.rb @gl-test

            ^[Optional]
            *_spec.rb @gl-test

            [Partially optional]
            *.md @gl-docs

            ^[Partially optional]
            doc/* @gl-docs
        "#;

        lazy_static! {
            static ref FILE: File = File::new(FILE_CONTENT.as_bytes(), None);
        }

        #[test]
        fn returns_whether_a_section_is_optional() {
            assert!(!FILE.optional_section(String::from("Required")));
            assert!(FILE.optional_section(String::from("Optional")));
            assert!(!FILE.optional_section(String::from("Partially optional")));
            assert!(!FILE.optional_section(String::from("Does not exist")));
        }
    }

    mod entries_for_path {
        use crate::gitlab::file::tests::FILE;

        mod returns_expected_matches {
            use crate::gitlab::File;

            pub fn shared_examples(file: &File) {
                for_a_path_without_matches::returns_an_empty_array_for_an_unmatched_path();
                matches_random_files_to_a_pattern(file);
                uses_the_last_pattern_if_multiple_patterns_match(file);
                returns_the_usernames_for_a_file_matching_a_pattern_with_a_glob(file);
                allows_specifying_multiple_users(file);
                returns_emails_and_usernames_for_a_matched_pattern(file);
                allows_escaping_the_pound_sign_used_for_comments(file);
                returns_the_usernames_for_a_file_nested_in_a_directory(file);
                returns_the_usernames_for_a_pattern_matched_with_a_glob_in_a_folder(file);
                allows_matching_files_nested_anywhere_in_the_repository(file);
                allows_allows_limiting_the_matching_files_to_the_root_of_the_repository_aggregate_failure(file);
                correctly_matches_paths_with_spaces(file);
                paths_with_whitespaces_and_username_lookalikes::parses_correctly();
                a_glob_on_the_root_directory::matches_files_in_the_root_directory();
                a_glob_on_the_root_directory::does_not_match_nested_files();
                a_glob_on_the_root_directory::partial_matches::does_not_match_a_file_in_a_folder_that_looks_the_same();
                a_glob_on_the_root_directory::partial_matches::matches_the_file_in_any_folder();
            }

            mod for_a_path_without_matches {
                use lazy_static::lazy_static;

                use crate::gitlab::File;

                const FILE_CONTENT: &str = r#"
                    # Simulating a CODOWNERS without entries
                "#;

                lazy_static! {
                    static ref FILE: File = File::new(FILE_CONTENT.as_bytes(), None);
                }

                pub fn returns_an_empty_array_for_an_unmatched_path() {
                    let entry = FILE.entries_for_path(String::from("no_matches"));

                    assert_eq!(entry, []);
                }
            }

            fn matches_random_files_to_a_pattern(file: &File) {
                let entries = file.entries_for_path(String::from("app/assets/something.vue"));
                let entry = entries.first().unwrap();

                assert_eq!(entry.pattern, "*");
                assert!(entry.owner_line.contains("default-codeowner"));
            }

            fn uses_the_last_pattern_if_multiple_patterns_match(file: &File) {
                let entries = file.entries_for_path(String::from("hello.rb"));
                let entry = entries.first().unwrap();

                assert_eq!(entry.pattern, "*.rb");
                assert_eq!(entry.owner_line, "@ruby-owner");
            }

            fn returns_the_usernames_for_a_file_matching_a_pattern_with_a_glob(file: &File) {
                let entries = file.entries_for_path(String::from("app/models/repository.rb"));
                let entry = entries.first().unwrap();

                assert_eq!(entry.owner_line, "@ruby-owner");
            }

            fn allows_specifying_multiple_users(file: &File) {
                let entries = file.entries_for_path(String::from("CODEOWNERS"));
                let entry = entries.first().unwrap();

                for owner in ["multiple", "owners", "tab-separated"] {
                    assert!(entry.owner_line.contains(owner));
                }
            }

            fn returns_emails_and_usernames_for_a_matched_pattern(file: &File) {
                let entries = file.entries_for_path(String::from("LICENSE"));
                let entry = entries.first().unwrap();

                for owner in ["legal", "janedoe@gitlab.com"] {
                    assert!(entry.owner_line.contains(owner));
                }
            }

            fn allows_escaping_the_pound_sign_used_for_comments(file: &File) {
                let entries = file.entries_for_path(String::from("examples/#file_with_pound.rb"));
                let entry = entries.first().unwrap();

                assert!(entry.owner_line.contains("owner-file-with-pound"));
            }

            fn returns_the_usernames_for_a_file_nested_in_a_directory(file: &File) {
                let entries = file.entries_for_path(String::from("docs/projects/index.md"));
                let entry = entries.first().unwrap();

                assert!(entry.owner_line.contains("all-docs"));
            }

            fn returns_the_usernames_for_a_pattern_matched_with_a_glob_in_a_folder(file: &File) {
                let entries = file.entries_for_path(String::from("docs/index.md"));
                let entry = entries.first().unwrap();

                assert!(entry.owner_line.contains("root-docs"));
            }

            fn allows_matching_files_nested_anywhere_in_the_repository(file: &File) {
                let lib_entries =
                    file.entries_for_path(String::from("lib/gitlab/git/repository.rb"));
                let lib_entry = lib_entries.first().unwrap();
                let other_lib_entries =
                    file.entries_for_path(String::from("ee/lib/gitlab/git/repository.rb"));
                let other_lib_entry = other_lib_entries.first().unwrap();

                // NOTE: matches failure aggregation in reference
                assert_eq!(
                    (
                        lib_entry.owner_line.contains("lib-owner"),
                        other_lib_entry.owner_line.contains("lib-owner")
                    ),
                    (true, true)
                );
            }

            fn allows_allows_limiting_the_matching_files_to_the_root_of_the_repository_aggregate_failure(
                file: &File,
            ) {
                let config_entries = file.entries_for_path(String::from("config/database.yml"));
                let config_entry = config_entries.first().unwrap();
                let other_config_entries =
                    file.entries_for_path(String::from("other/config/database.yml"));
                let other_config_entry = other_config_entries.first().unwrap();

                // NOTE: matches failure aggregation in reference
                assert_eq!(
                    (
                        config_entry.owner_line.contains("config-owner"),
                        other_config_entry.owner_line.contains("@default-codeowner")
                    ),
                    (true, true)
                );
            }

            fn correctly_matches_paths_with_spaces(file: &File) {
                let entries = file.entries_for_path(String::from("path with spaces/docs.md"));
                let entry = entries.first().unwrap();

                assert_eq!(entry.owner_line, "@space-owner");
            }

            mod paths_with_whitespaces_and_username_lookalikes {
                use lazy_static::lazy_static;

                use crate::gitlab::File;

                const FILE_CONTENT: &str = r#"
                    a/weird\ path\ with/\ @username\ /\ and-email@lookalikes.com\ / @user-1 email@gitlab.org @user-2
                "#;

                lazy_static! {
                    static ref FILE: File = File::new(FILE_CONTENT.as_bytes(), None);
                }

                pub fn parses_correctly() {
                    let entries = FILE.entries_for_path(String::from(
                        "a/weird path with/ @username / and-email@lookalikes.com /test.rb",
                    ));
                    let entry = entries.first().unwrap();

                    for owner in ["user-1", "user-2", "email@gitlab.org"] {
                        assert!(entry.owner_line.contains(owner));
                    }

                    for owner in ["username", "and-email@lookalikes.com"] {
                        assert!(!entry.owner_line.contains(owner));
                    }
                }
            }

            mod a_glob_on_the_root_directory {
                use lazy_static::lazy_static;

                use crate::gitlab::File;

                const FILE_CONTENT: &str = r#"
                    /* @user-1 @user-2
                "#;

                lazy_static! {
                    static ref FILE: File = File::new(FILE_CONTENT.as_bytes(), None);
                }

                pub fn matches_files_in_the_root_directory() {
                    let entries = FILE.entries_for_path(String::from("README.md"));
                    let entry = entries.first().unwrap();

                    for owner in ["user-1", "user-2"] {
                        assert!(entry.owner_line.contains(owner));
                    }
                }

                pub fn does_not_match_nested_files() {
                    let entries = FILE.entries_for_path(String::from("nested/path/README.md"));
                    let entry = entries.first();

                    assert_eq!(entry, None);
                }

                pub mod partial_matches {
                    use lazy_static::lazy_static;

                    use crate::gitlab::File;

                    const FILE_CONTENT: &str = r#"
                        foo/* @user-1 @user-2
                    "#;

                    lazy_static! {
                        static ref FILE: File = File::new(FILE_CONTENT.as_bytes(), None);
                    }

                    pub fn does_not_match_a_file_in_a_folder_that_looks_the_same() {
                        let entries = FILE.entries_for_path(String::from("fufoo/bar"));
                        let entry = entries.first();

                        assert_eq!(entry, None);
                    }

                    pub fn matches_the_file_in_any_folder() {
                        let relative_entries = FILE.entries_for_path(String::from("baz/foo/bar"));
                        let relative_entry = relative_entries.first().unwrap();
                        let root_entries = FILE.entries_for_path(String::from("/foo/bar"));
                        let root_entry = root_entries.first().unwrap();

                        for owner in ["user-1", "user-2"] {
                            assert!(relative_entry.owner_line.contains(owner));
                        }

                        for owner in ["user-1", "user-2"] {
                            assert!(root_entry.owner_line.contains(owner));
                        }
                    }
                }
            }
        }

        #[test]
        fn when_codeowners_file_contains_no_sections() {
            returns_expected_matches::shared_examples(&FILE);
        }

        mod when_codeowners_file_contains_multiple_sections {
            use crate::gitlab::file::tests::FILE_SECTIONAL_CODEOWNERS_EXAMPLE;

            #[test]
            fn returns_expected_matches() {
                super::returns_expected_matches::shared_examples(
                    &FILE_SECTIONAL_CODEOWNERS_EXAMPLE,
                );
            }
        }
    }

    mod valid {
        mod when_codeowners_file_is_correct {
            use crate::gitlab::file::tests::FILE;

            #[test]
            fn does_not_detect_errors() {
                assert!(FILE.valid());
                assert_eq!(FILE.errors(), []);
            }
        }

        mod when_codeowners_file_has_errors {
            use std::path::PathBuf;

            use lazy_static::lazy_static;

            use crate::gitlab::{Error, ErrorType, File};

            const FILE_CONTENT: &str = r#"
                *.rb

                []
                *_spec.rb @gl-test

                ^[Optional][5]
                *.txt @user

                [Invalid section

                [OK section header]
                some_entry not_a_user_not_an_email
            "#;

            lazy_static! {
                static ref FILE: File =
                    File::new(FILE_CONTENT.as_bytes(), Some(PathBuf::from("CODEOWNERS")));
            }

            #[test]
            fn detects_syntax_errors() {
                assert!(!FILE.valid());

                assert_eq!(
                    FILE.errors(),
                    [
                        Error::new(
                            ErrorType::MissingEntryOwner.to_string(),
                            1,
                            PathBuf::from("CODEOWNERS")
                        ),
                        Error::new(
                            ErrorType::MissingSectionName.to_string(),
                            3,
                            PathBuf::from("CODEOWNERS")
                        ),
                        Error::new(
                            ErrorType::InvalidApprovalRequirement.to_string(),
                            6,
                            PathBuf::from("CODEOWNERS")
                        ),
                        Error::new(
                            ErrorType::InvalidSectionFormat.to_string(),
                            9,
                            PathBuf::from("CODEOWNERS")
                        ),
                        Error::new(
                            ErrorType::InvalidEntryOwnerFormat.to_string(),
                            9,
                            PathBuf::from("CODEOWNERS")
                        ),
                        Error::new(
                            ErrorType::InvalidEntryOwnerFormat.to_string(),
                            12,
                            PathBuf::from("CODEOWNERS")
                        )
                    ]
                );
            }
        }
    }
}
