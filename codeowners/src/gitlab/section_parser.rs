use super::{Entry, ErrorType, ReferenceExtractor, Section};
use indexmap::IndexMap;

pub mod section_parser_regex {
    use fancy_regex::Regex;
    use lazy_static::lazy_static;

    lazy_static! {
        pub static ref OPTIONAL: Regex = Regex::new(r"(?<optional>\^)?").unwrap();
        pub static ref NAME: Regex = Regex::new(r"\[(?<name>.*?)\]").unwrap();
        pub static ref APPROVALS: Regex = Regex::new(r"(?:\[(?<approvals>\d*?)\])?").unwrap();
        pub static ref DEFAULT_OWNERS: Regex =
            Regex::new(r"(?<default_owners>\s+[@\w_.\-\/\s+]*)?").unwrap();
        pub static ref INVALID_NAME: Regex = Regex::new(r"\[[^\]]+?").unwrap();
        pub static ref HEADER_REGEX: Regex = Regex::new(&format!(
            r"^{}{}{}{}",
            OPTIONAL.as_str(),
            NAME.as_str(),
            APPROVALS.as_str(),
            DEFAULT_OWNERS.as_str()
        ))
        .unwrap();
        pub static ref REGEX_INVALID_SECTION: Regex = Regex::new(&format!(
            r"^{}{}$",
            OPTIONAL.as_str(),
            INVALID_NAME.as_str()
        ))
        .unwrap();
    }
}

pub type SectionalData = IndexMap<String, IndexMap<String, Entry>>;

/// Reference: https://gitlab.com/gitlab-org/gitlab/-/blob/master/ee/lib/gitlab/code_owners/section_parser.rb
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SectionParser<'a> {
    line: String,
    sectional_data: &'a SectionalData,
    pub errors: Vec<ErrorType>,
}

impl<'a> SectionParser<'a> {
    pub fn new(line: String, sectional_data: &'a SectionalData) -> Self {
        Self {
            line,
            sectional_data,
            errors: Vec::new(),
        }
    }

    pub fn execute(&mut self) -> Option<Section> {
        if let Some(section) = self.fetch_section() {
            if section.name.is_empty() {
                self.errors.push(ErrorType::MissingSectionName);
            }
            if section.optional && section.approvals > 0 {
                self.errors.push(ErrorType::InvalidApprovalRequirement);
            }
            if !section.default_owners.is_empty()
                && ReferenceExtractor::new(String::from(&section.default_owners))
                    .references()
                    .is_empty()
            {
                self.errors.push(ErrorType::InvalidSectionOwnerFormat);
            }
            return Some(section);
        }

        if self.invalid_section() {
            self.errors.push(ErrorType::InvalidSectionFormat);
        }

        None
    }

    pub fn valid(&self) -> bool {
        self.errors.is_empty()
    }

    fn fetch_section(&self) -> Option<Section> {
        section_parser_regex::HEADER_REGEX
            .captures(&self.line)
            .ok()
            .flatten()
            .map(|r#match| {
                let name = r#match
                    .name("name")
                    .map(|m| String::from(m.as_str()))
                    .unwrap_or_default();
                let optional = Some(r#match.name("optional").is_some());
                let approvals = r#match
                    .name("approvals")
                    .and_then(|m| m.as_str().parse::<usize>().ok());
                let default_owners = r#match
                    .name("default_owners")
                    .map(|m| String::from(m.as_str()));

                Section::new(
                    self.find_section_name(name),
                    optional,
                    approvals,
                    default_owners,
                )
            })
    }

    fn find_section_name<T: AsRef<str>>(&self, name: T) -> String {
        if let Some(last_key) = self.sectional_data.keys().last() {
            if last_key == Section::DEFAULT {
                return String::from(name.as_ref());
            }
        }

        self.sectional_data
            .keys()
            .find(|k| k.eq_ignore_ascii_case(name.as_ref()))
            .map(String::from)
            .unwrap_or_else(|| String::from(name.as_ref()))
    }

    fn invalid_section(&self) -> bool {
        section_parser_regex::REGEX_INVALID_SECTION
            .is_match(&self.line)
            .ok()
            .unwrap_or_default()
    }
}

/// Reference: https://gitlab.com/gitlab-org/gitlab/-/blob/master/ee/spec/lib/gitlab/code_owners/section_parser_spec.rb
#[cfg(test)]
mod tests {
    use super::{SectionParser, SectionalData};
    use lazy_static::lazy_static;

    lazy_static! {
        static ref SECTION: SectionalData = SectionalData::new();
        static ref PARSER: SectionParser<'static> = SectionParser::new(String::new(), &SECTION);
    }

    mod execute {
        use crate::gitlab::{section_parser::tests::SECTION, SectionParser};

        #[test]
        fn when_line_is_not_a_section_header() {
            let mut parser = SectionParser::new(String::from("foo"), &SECTION);
            assert_eq!(parser.execute(), None);
        }

        mod when_line_is_a_section_header {
            use crate::gitlab::{ErrorType, SectionParser, SectionalData};
            use assert_matches::assert_matches;
            use std::iter::FromIterator;

            #[test]
            fn parses_all_section_properties() {
                for (line, name, optional, approvals, default_owners, sectional_data, errors) in
                    vec![
                        (
                            "[]",
                            "",
                            false,
                            0,
                            "",
                            SectionalData::new(),
                            vec![ErrorType::MissingSectionName],
                        ),
                        (
                            "[Doc]",
                            "Doc",
                            false,
                            0,
                            "",
                            SectionalData::new(),
                            Vec::new(),
                        ),
                        (
                            "[Doc]",
                            "doc",
                            false,
                            0,
                            "",
                            SectionalData::from_iter(
                                vec![(String::from("doc"), Default::default())].into_iter(),
                            ),
                            Vec::new(),
                        ),
                        (
                            "[Doc]",
                            "Doc",
                            false,
                            0,
                            "",
                            SectionalData::from_iter(
                                vec![(String::from("foo"), Default::default())].into_iter(),
                            ),
                            Vec::new(),
                        ),
                        (
                            "^[Doc]",
                            "Doc",
                            true,
                            0,
                            "",
                            SectionalData::new(),
                            Vec::new(),
                        ),
                        (
                            "[Doc][1]",
                            "Doc",
                            false,
                            1,
                            "",
                            SectionalData::new(),
                            Vec::new(),
                        ),
                        (
                            "^[Doc][1]",
                            "Doc",
                            true,
                            1,
                            "",
                            SectionalData::new(),
                            vec![ErrorType::InvalidApprovalRequirement],
                        ),
                        (
                            "^[Doc][1] @doc",
                            "Doc",
                            true,
                            1,
                            "@doc",
                            SectionalData::new(),
                            vec![ErrorType::InvalidApprovalRequirement],
                        ),
                        (
                            "^[Doc][1] @doc @dev",
                            "Doc",
                            true,
                            1,
                            "@doc @dev",
                            SectionalData::new(),
                            vec![ErrorType::InvalidApprovalRequirement],
                        ),
                        (
                            "^[Doc][1] @gl/doc-1",
                            "Doc",
                            true,
                            1,
                            "@gl/doc-1",
                            SectionalData::new(),
                            vec![ErrorType::InvalidApprovalRequirement],
                        ),
                        (
                            "[Doc][1] @doc",
                            "Doc",
                            false,
                            1,
                            "@doc",
                            SectionalData::new(),
                            Vec::new(),
                        ),
                        (
                            "[Doc] @doc",
                            "Doc",
                            false,
                            0,
                            "@doc",
                            SectionalData::new(),
                            Vec::new(),
                        ),
                        (
                            "^[Doc] @doc",
                            "Doc",
                            true,
                            0,
                            "@doc",
                            SectionalData::new(),
                            Vec::new(),
                        ),
                        (
                            "[Doc] @doc @rrr.dev @dev",
                            "Doc",
                            false,
                            0,
                            "@doc @rrr.dev @dev",
                            SectionalData::new(),
                            Vec::new(),
                        ),
                        (
                            "^[Doc] @doc @rrr.dev @dev",
                            "Doc",
                            true,
                            0,
                            "@doc @rrr.dev @dev",
                            SectionalData::new(),
                            Vec::new(),
                        ),
                        (
                            "[Doc][2] @doc @rrr.dev @dev",
                            "Doc",
                            false,
                            2,
                            "@doc @rrr.dev @dev",
                            SectionalData::new(),
                            Vec::new(),
                        ),
                        (
                            "[Doc] malformed",
                            "Doc",
                            false,
                            0,
                            "malformed",
                            SectionalData::new(),
                            vec![ErrorType::InvalidSectionOwnerFormat],
                        ),
                    ]
                    .into_iter()
                {
                    let mut parser = SectionParser::new(String::from(line), &sectional_data);
                    let section = assert_matches!(parser.execute(), Some(s) => s);
                    assert_eq!(section.name, String::from(name));
                    assert_eq!(section.optional, optional);
                    assert_eq!(section.approvals, approvals);
                    assert_eq!(section.default_owners, default_owners);
                    if errors.is_empty() {
                        assert_eq!(parser.valid(), true);
                    } else {
                        assert_eq!(parser.valid(), false);
                        assert_eq!(parser.errors, errors);
                    }
                }
            }
        }

        mod when_section_header_is_invalid {
            use crate::gitlab::{section_parser::tests::SECTION, ErrorType, SectionParser};

            #[test]
            fn validates_section_correctness() {
                for (line, status, errors) in vec![
                    ("^[Invalid", false, vec![ErrorType::InvalidSectionFormat]),
                    ("[Invalid", false, vec![ErrorType::InvalidSectionFormat]),
                ]
                .into_iter()
                {
                    let mut parser = SectionParser::new(String::from(line), &SECTION);
                    assert_eq!(parser.execute(), None);

                    assert_eq!(parser.valid(), status);
                    assert_eq!(parser.errors, errors);
                }
            }
        }
    }
}
