mod entry;
mod error;
mod file;
mod reference_extractor;
mod section;
mod section_parser;
pub mod user;

use std::{
    fmt, fs,
    io::{BufReader, Read},
    path::Path,
};

#[cfg(feature = "pyo3")]
use pyo3::prelude::*;
#[cfg(feature = "pyo3")]
use pyo3_stub_gen::derive::{gen_stub_pyclass, gen_stub_pymethods};

use crate::{FromPath, FromReader, OwnersOfPath};

pub use entry::*;
pub use error::*;
pub use file::*;

pub use reference_extractor::*;
pub use section::*;
pub use section_parser::*;

#[derive(Debug, PartialEq, Clone)]
pub enum GitLabOwner {
    Name(String),
    Email(String),
}

impl fmt::Display for GitLabOwner {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let inner = match *self {
            GitLabOwner::Name(ref n) => n,
            GitLabOwner::Email(ref e) => e,
        };
        f.write_str(inner.as_str())
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GitLabOwners {
    file: File,
}

impl OwnersOfPath for GitLabOwners {
    type Owner = GitLabOwner;

    fn of<P>(&self, path: P) -> Option<Vec<Self::Owner>>
    where
        P: AsRef<Path>,
    {
        self.file
            .entries_for_path(String::from(path.as_ref().to_string_lossy()))
            .iter()
            .try_fold(
                Vec::new(),
                |mut acc, entry| -> anyhow::Result<Vec<GitLabOwner>> {
                    for owner in entry
                        .extractor
                        .emails()
                        .iter()
                        .map(|e| GitLabOwner::Email(String::from(e)))
                        .chain(
                            entry
                                .extractor
                                .names()
                                .iter()
                                .map(|n| GitLabOwner::Name(String::from(n))),
                        )
                    {
                        acc.push(owner);
                    }
                    Ok(acc)
                },
            )
            .ok()
    }
}

impl FromPath for GitLabOwners {
    fn from_path<P>(path: P) -> anyhow::Result<Self>
    where
        P: AsRef<Path>,
    {
        Self::from_reader(fs::File::open(path)?)
    }
}

impl FromReader for GitLabOwners {
    fn from_reader<R>(read: R) -> anyhow::Result<Self>
    where
        R: Read,
    {
        let buf_reader = BufReader::new(read);
        let file = File::new(buf_reader, None);
        if !file.valid() {
            let error_messages: Vec<String> =
                file.errors().iter().map(ToString::to_string).collect();
            return Err(anyhow::Error::msg(error_messages.join("\n")));
        }

        Ok(GitLabOwners { file })
    }
}

#[derive(Debug, PartialEq, Eq)]
#[cfg_attr(feature = "pyo3", gen_stub_pyclass, pyclass)]
pub struct BindingsGitLabOwners(pub GitLabOwners);

#[cfg(feature = "pyo3")]
#[gen_stub_pymethods]
#[pymethods]
impl BindingsGitLabOwners {
    fn of(&self, path: String) -> Option<Vec<String>> {
        let owners = self.0.of(Path::new(&path));
        match owners {
            Some(owners) => Some(owners.iter().map(|owner| owner.to_string()).collect()),
            None => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // NOTE: GitLab's CODEOWNERS syntax is a superset of GitHub's
    const EXAMPLE: &[u8] = include_bytes!("../../test_fixtures/github/codeowners_example");

    #[test]
    fn owner_displays() {
        assert!(GitLabOwner::Name("@user".into()).to_string() == "@user");
        assert!(GitLabOwner::Email("user@domain.com".into()).to_string() == "user@domain.com");
    }

    #[test]
    fn owners_owns_wildcard() {
        let owners = GitLabOwners::from_reader(EXAMPLE).unwrap();
        assert_eq!(
            owners.of("foo.txt"),
            Some(vec![
                GitLabOwner::Name("@global-owner1".into()),
                GitLabOwner::Name("@global-owner2".into()),
            ])
        );
        assert_eq!(
            owners.of("foo/bar.txt"),
            Some(vec![
                GitLabOwner::Name("@global-owner1".into()),
                GitLabOwner::Name("@global-owner2".into()),
            ])
        )
    }

    #[test]
    fn owners_owns_js_extention() {
        let owners = GitLabOwners::from_reader(EXAMPLE).unwrap();
        assert_eq!(
            owners.of("foo.js"),
            Some(vec![GitLabOwner::Name("@js-owner".into())])
        );
        assert_eq!(
            owners.of("foo/bar.js"),
            Some(vec![GitLabOwner::Name("@js-owner".into())])
        )
    }

    #[test]
    fn owners_owns_go_extention() {
        let owners = GitLabOwners::from_reader(EXAMPLE).unwrap();
        assert_eq!(
            owners.of("foo.go"),
            Some(vec![GitLabOwner::Email("docs@example.com".into())])
        );
        assert_eq!(
            owners.of("foo/bar.go"),
            Some(vec![GitLabOwner::Email("docs@example.com".into())])
        )
    }

    #[test]
    fn owners_owns_anchored_build_logs() {
        let owners = GitLabOwners::from_reader(EXAMPLE).unwrap();
        // relative to root
        assert_eq!(
            owners.of("build/logs/foo.go"),
            Some(vec![GitLabOwner::Name("@doctocat".into())])
        );
        assert_eq!(
            owners.of("build/logs/foo/bar.go"),
            Some(vec![GitLabOwner::Name("@doctocat".into())])
        );
        // not relative to root
        assert_eq!(
            owners.of("foo/build/logs/foo.go"),
            Some(vec![GitLabOwner::Email("docs@example.com".into())])
        )
    }

    #[test]
    fn owners_owns_unanchored_docs() {
        let owners = GitLabOwners::from_reader(EXAMPLE).unwrap();
        // docs anywhere
        assert_eq!(
            owners.of("foo/docs/foo.js"),
            Some(vec![GitLabOwner::Email("docs@example.com".into())])
        );
        assert_eq!(
            owners.of("foo/bar/docs/foo.js"),
            Some(vec![GitLabOwner::Email("docs@example.com".into())])
        );
        // but not nested
        assert_eq!(
            owners.of("foo/bar/docs/foo/foo.js"),
            Some(vec![GitLabOwner::Name("@js-owner".into())])
        )
    }

    #[test]
    fn owners_owns_unanchored_apps() {
        let owners = GitLabOwners::from_reader(EXAMPLE).unwrap();
        assert_eq!(
            owners.of("foo/apps/foo.js"),
            Some(vec![GitLabOwner::Name("@octocat".into())])
        )
    }

    #[test]
    fn owners_owns_anchored_docs() {
        let owners = GitLabOwners::from_reader(EXAMPLE).unwrap();
        // relative to root
        assert_eq!(
            owners.of("docs/foo.js"),
            Some(vec![GitLabOwner::Name("@doctocat".into())])
        )
    }

    // NOTE: GitHub does this, but GitLab does not
    #[test]
    fn no_implied_children_owners() {
        let owners = GitLabOwners::from_reader("foo/bar @doug".as_bytes()).unwrap();
        assert_eq!(owners.of("foo/bar/baz.rs"), Some(Vec::new()))
    }
}
