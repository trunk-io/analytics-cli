use std::{collections::BTreeSet, iter::FromIterator};

pub mod reference_extractor_regex {
    use fancy_regex::Regex;
    use lazy_static::lazy_static;

    use crate::gitlab::user;

    lazy_static! {
        pub static ref EMAIL_REGEXP: Regex =
            Regex::new(r"[^@\s]{1,100}@[^@\s]{1,255}(?<!\W)").unwrap();
        pub static ref USER_REGEXP: Regex = user::REFERENCE_PATTERN.clone();
    }
}

/// Reference: https://gitlab.com/gitlab-org/gitlab/-/blob/master/ee/lib/gitlab/code_owners/reference_extractor.rb
#[derive(Debug, Clone, Default, PartialEq, Eq, PartialOrd, Ord)]
pub struct ReferenceExtractor {
    text: String,
}

impl ReferenceExtractor {
    pub fn new(text: String) -> Self {
        Self { text }
    }

    pub fn names(&self) -> BTreeSet<String> {
        self.matches().names
    }

    pub fn emails(&self) -> BTreeSet<String> {
        self.matches().emails
    }

    pub fn references(&self) -> BTreeSet<String> {
        if self.text.is_empty() {
            return BTreeSet::new();
        }

        let ReferenceExtractorMatches { emails, names } = self.matches();

        emails.union(&names).cloned().collect()
    }

    fn matches(&self) -> ReferenceExtractorMatches {
        let emails = reference_extractor_regex::EMAIL_REGEXP
            .find_iter(&self.text)
            .filter_map(|m| m.ok())
            .map(|m| String::from(m.as_str()));
        let names = reference_extractor_regex::USER_REGEXP
            .find_iter(&self.text)
            .filter_map(|m| m.ok())
            .map(|m| String::from(m.as_str()));

        ReferenceExtractorMatches {
            emails: BTreeSet::from_iter(emails),
            names: BTreeSet::from_iter(names),
        }
    }
}

#[derive(Debug)]
struct ReferenceExtractorMatches {
    emails: BTreeSet<String>,
    names: BTreeSet<String>,
}

/// Reference: https://gitlab.com/gitlab-org/gitlab/-/blob/master/ee/spec/lib/gitlab/code_owners/reference_extractor_spec.rb
#[cfg(test)]
mod tests {
    use lazy_static::lazy_static;

    use super::ReferenceExtractor;

    const TEXT: &str = r#"
        This is a long text that mentions some users.
        @user-1, @user-2 and user@gitlab.org take a walk in the park.
        There they meet @user-4 that was out with other-user@gitlab.org.
        @user-1 thought it was late, so went home straight away not to
        run into some @group @group/nested-on/other-group
    "#;

    lazy_static! {
        static ref EXTRACTOR: ReferenceExtractor = ReferenceExtractor::new(String::from(TEXT));
    }

    mod emails {
        use std::{collections::BTreeSet, iter::FromIterator};

        use crate::gitlab::reference_extractor::tests::EXTRACTOR;

        #[test]
        fn includes_all_mentioned_email_addresses() {
            assert_eq!(
                EXTRACTOR.emails(),
                BTreeSet::from_iter(
                    vec![
                        String::from("user@gitlab.org"),
                        String::from("other-user@gitlab.org")
                    ]
                    .into_iter()
                )
            );
        }

        mod redos_vulnerability {
            use rand::{
                distributions::{Alphanumeric, DistString},
                thread_rng,
            };

            fn generate_email(left_length: usize, right_length: usize) -> String {
                let mut rng = thread_rng();
                let left = Alphanumeric.sample_string(&mut rng, left_length);
                let right = Alphanumeric.sample_string(&mut rng, right_length);
                format!("{left}@{right}")
            }

            mod when_valid_email_length {
                use std::{collections::BTreeSet, iter::FromIterator};

                use crate::gitlab::ReferenceExtractor;

                #[test]
                fn includes_the_email() {
                    let email = super::generate_email(100, 255);
                    let extractor = ReferenceExtractor::new(email.clone());

                    assert_eq!(
                        extractor.emails(),
                        BTreeSet::from_iter(vec![email].into_iter())
                    );
                }
            }

            mod when_invalid_email_first_part_length {
                use std::{collections::BTreeSet, iter::FromIterator};

                use crate::gitlab::ReferenceExtractor;

                #[test]
                fn doesnt_include_the_email() {
                    let email = super::generate_email(101, 255);
                    let extractor = ReferenceExtractor::new(email.clone());

                    assert_ne!(
                        extractor.emails(),
                        BTreeSet::from_iter(vec![email].into_iter())
                    );
                }
            }

            mod when_invalid_email_second_part_length {
                use std::{collections::BTreeSet, iter::FromIterator};

                use crate::gitlab::ReferenceExtractor;

                #[test]
                fn doesnt_include_the_email() {
                    let email = super::generate_email(100, 256);
                    let extractor = ReferenceExtractor::new(email.clone());

                    assert_ne!(
                        extractor.emails(),
                        BTreeSet::from_iter(vec![email].into_iter())
                    );
                }
            }
        }
    }

    mod names {
        use std::{collections::BTreeSet, iter::FromIterator};

        use crate::gitlab::reference_extractor::tests::EXTRACTOR;

        #[test]
        fn includes_all_mentioned_usernames_and_groupnames() {
            assert_eq!(
                EXTRACTOR.names(),
                BTreeSet::from_iter(
                    vec![
                        String::from("@user-1"),
                        String::from("@user-2"),
                        String::from("@user-4"),
                        String::from("@group"),
                        String::from("@group/nested-on/other-group")
                    ]
                    .into_iter()
                )
            );
        }
    }

    mod references {
        use std::{collections::BTreeSet, iter::FromIterator};

        use crate::gitlab::reference_extractor::tests::EXTRACTOR;

        #[test]
        fn includes_all_user_references_once() {
            assert_eq!(
                EXTRACTOR.references(),
                BTreeSet::from_iter(
                    vec![
                        String::from("@user-1"),
                        String::from("@user-2"),
                        String::from("user@gitlab.org"),
                        String::from("@user-4"),
                        String::from("other-user@gitlab.org"),
                        String::from("@group"),
                        String::from("@group/nested-on/other-group")
                    ]
                    .into_iter()
                )
            );
        }
    }
}
