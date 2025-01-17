use super::{ReferenceExtractor, Section};
use anyhow::Result;
use std::{
    collections::BTreeSet,
    hash::{Hash, Hasher},
    ops::Deref,
    sync::Arc,
};

#[derive(Debug, Clone, Default, PartialEq, Eq, PartialOrd, Ord)]
pub struct Data {
    pub pattern: String,
    pub owner_line: String,
    pub section: String,
    pub optional: bool,
    pub approvals_required: usize,
}

/// Reference: https://gitlab.com/gitlab-org/gitlab/-/blob/master/ee/lib/gitlab/code_owners/entry.rb
#[derive(Debug, Clone, Default, PartialOrd, Ord)]
pub struct Entry {
    data: Data,
    users: Option<BTreeSet<String>>,
    groups: Option<BTreeSet<String>>,
    // NOTE: making this `pub` is not part of the reference implementation, but needed to extract
    // owners.
    pub(crate) extractor: Arc<ReferenceExtractor>,
    names: Option<BTreeSet<String>>,
}

impl Entry {
    pub fn new(
        pattern: String,
        owner_line: String,
        section: Option<String>,
        optional: Option<bool>,
        approvals_required: Option<usize>,
    ) -> Self {
        Self {
            data: Data {
                pattern,
                owner_line: owner_line.clone(),
                section: section.unwrap_or_else(|| String::from(Section::DEFAULT)),
                optional: optional.unwrap_or_default(),
                approvals_required: approvals_required.unwrap_or_default(),
            },
            extractor: Arc::new(ReferenceExtractor::new(owner_line)),
            ..Default::default()
        }
    }

    // NOTE: Allow reference implementation methods for double-checking
    #[allow(dead_code)]
    pub fn users(&self) -> Result<BTreeSet<String>> {
        self.users.clone().ok_or_else(|| {
            anyhow::Error::msg(format!("CodeOwners for {} not loaded", &self.owner_line))
        })
    }

    // NOTE: Allow reference implementation methods for double-checking
    #[allow(dead_code)]
    pub fn groups(&self) -> Result<BTreeSet<String>> {
        self.groups.clone().ok_or_else(|| {
            anyhow::Error::msg(format!(
                "CodeOwners groups for {} not loaded",
                &self.owner_line
            ))
        })
    }

    // NOTE: Allow reference implementation methods for double-checking
    #[allow(dead_code)]
    pub fn add_matching_groups_from(&mut self, new_groups: Vec<String>) {
        for group in new_groups.into_iter() {
            if self.matching_group(&group) {
                self.groups
                    .get_or_insert_with(Default::default)
                    .insert(group.to_lowercase());
            }
        }
    }

    // NOTE: Allow reference implementation methods for double-checking
    #[allow(dead_code)]
    pub fn add_matching_users_from(&mut self, new_users: Vec<String>) {
        for user in new_users.into_iter() {
            if self.matching_user(&user) {
                self.users.get_or_insert_with(Default::default).insert(user);
            }
        }
    }

    // NOTE: Allow reference implementation methods for double-checking
    #[allow(dead_code)]
    fn names(&mut self) -> &BTreeSet<String> {
        let extractor = self.extractor.clone();
        self.names.get_or_insert_with(|| {
            extractor
                .names()
                .iter()
                .map(|name| name.to_lowercase())
                .collect()
        })
    }

    fn matching_group<T: AsRef<str>>(&mut self, group: T) -> bool {
        self.names().contains(&group.as_ref().to_lowercase())
    }

    fn matching_user<T: AsRef<str>>(&mut self, user: T) -> bool {
        self.names().contains(&user.as_ref().to_lowercase())
    }
}

impl Deref for Entry {
    type Target = Data;

    fn deref(&self) -> &Self::Target {
        &self.data
    }
}

impl PartialEq for Entry {
    fn eq(&self, other: &Self) -> bool {
        self.pattern == other.pattern && self.owner_line == other.owner_line
    }
}

impl Eq for Entry {}

impl Hash for Entry {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.pattern.hash(state);
        self.owner_line.hash(state);
    }
}

/// Reference: https://gitlab.com/gitlab-org/gitlab/-/blob/master/ee/spec/lib/gitlab/code_owners/entry_spec.rb
#[cfg(test)]
mod tests {
    use crate::gitlab::entry::Entry;
    use lazy_static::lazy_static;
    use std::{collections::BTreeSet, iter::FromIterator};

    lazy_static! {
        static ref ENTRY: Entry = Entry::new(
            String::from("/**/file"),
            String::from("@user jane@gitlab.org @group @group/nested-group"),
            Some(String::from("Documentation")),
            None,
            None
        );
    }

    #[test]
    fn is_uniq_by_the_pattern_and_owner_line() {
        let equal_entry = ENTRY.clone();
        let other_entry = Entry::new(
            String::from("/**/other_file"),
            String::from("@user jane@gitlab.org @group"),
            None,
            None,
            None,
        );

        assert_eq!(equal_entry, ENTRY.clone());
        assert_eq!(
            BTreeSet::from_iter([ENTRY.clone(), equal_entry.clone(), other_entry.clone()].iter()),
            BTreeSet::from_iter([equal_entry, other_entry].iter())
        );
    }

    mod users {
        use crate::gitlab::entry::tests::ENTRY;
        use std::{collections::BTreeSet, iter::FromIterator};

        #[test]
        fn raises_an_error_if_no_users_have_been_added() {
            assert!(ENTRY
                .users()
                .unwrap_err()
                .to_string()
                .contains("not loaded"));
        }

        #[test]
        fn returns_the_users_in_an_array() {
            let mut entry = ENTRY.clone();

            let user = String::from("@user");
            entry.add_matching_users_from(vec![user.clone()]);

            assert_eq!(
                entry.users().unwrap(),
                BTreeSet::from_iter(vec![user].into_iter())
            );
        }
    }

    mod all_users {
        use crate::gitlab::entry::tests::ENTRY;

        #[test]
        fn raises_an_error_if_users_have_not_been_loaded_for_groups() {
            let mut entry = ENTRY.clone();

            let group = String::from("@group");
            entry.add_matching_groups_from(vec![group]);

            assert!(ENTRY
                .users()
                .unwrap_err()
                .to_string()
                .contains("not loaded"));
        }
    }

    mod groups {
        use crate::gitlab::entry::tests::ENTRY;
        use std::{collections::BTreeSet, iter::FromIterator};

        #[test]
        fn raises_an_error_if_no_groups_have_been_added() {
            assert!(ENTRY
                .groups()
                .unwrap_err()
                .to_string()
                .contains("not loaded"));
        }

        #[test]
        fn returns_mentioned_groups() {
            let mut entry = ENTRY.clone();

            let group = String::from("@group");
            entry.add_matching_groups_from(vec![group.clone()]);

            assert_eq!(
                entry.groups().unwrap(),
                BTreeSet::from_iter(vec![group].into_iter())
            );
        }
    }

    mod add_matching_groups_from {
        use crate::gitlab::entry::tests::ENTRY;
        use std::{collections::BTreeSet, iter::FromIterator};

        #[test]
        fn returns_only_mentioned_groups_case_insensitively() {
            let mut entry = ENTRY.clone();

            let group = String::from("@group");
            let group2 = String::from("@Group");
            let nested_group = format!("{group}/nested-group");
            entry.add_matching_groups_from(vec![group.clone(), group2, nested_group.clone()]);

            assert_eq!(
                entry.groups().unwrap(),
                BTreeSet::from_iter(vec![group, nested_group].into_iter())
            );
        }
    }

    mod add_matching_users_from {
        use crate::gitlab::entry::tests::ENTRY;
        use std::{collections::BTreeSet, iter::FromIterator};

        #[test]
        fn does_not_add_the_same_user_twice() {
            let mut entry = ENTRY.clone();

            let user = String::from("@user");
            for _ in 0..2 {
                entry.add_matching_users_from(vec![user.clone()]);
            }

            assert_eq!(
                entry.users().unwrap(),
                BTreeSet::from_iter(vec![user].into_iter())
            );
        }

        #[test]
        fn only_adds_users_mentioned_in_the_owner_line() {
            let mut entry = ENTRY.clone();

            let other_user = String::from("@other_user");
            let user = String::from("@user");
            entry.add_matching_users_from(vec![other_user, user.clone()]);

            assert_eq!(
                entry.users().unwrap(),
                BTreeSet::from_iter(vec![user].into_iter())
            );
        }

        #[test]
        fn adds_users_by_username_case_insensitively() {
            let mut entry = ENTRY.clone();

            let user = String::from("@USER");
            entry.add_matching_users_from(vec![user.clone()]);

            assert_eq!(
                entry.users().unwrap(),
                BTreeSet::from_iter(vec![user].into_iter())
            );
        }
    }

    mod approvals_required {
        mod when_there_has_approvals_required_params_approvals_required {
            use crate::gitlab::Entry;

            #[test]
            fn returns_2() {
                let entry = Entry::new(
                    String::from("/**/file"),
                    String::from("@user jane@gitlab.org @group @group/nested-group"),
                    Some(String::from("Documentation")),
                    Some(false),
                    Some(2),
                );
                assert_eq!(entry.approvals_required, 2);
            }
        }

        mod when_there_has_no_approvals_required_params {
            use crate::gitlab::Entry;

            #[test]
            fn returns_0() {
                let entry = Entry::new(
                    String::from("/**/file"),
                    String::from("@user jane@gitlab.org @group @group/nested-group"),
                    Some(String::from("Documentation")),
                    None,
                    None,
                );
                assert_eq!(entry.approvals_required, 0);
            }
        }
    }
}
