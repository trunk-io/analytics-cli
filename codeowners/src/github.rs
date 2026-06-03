use std::{
    fmt,
    fs::File,
    io::{BufRead, BufReader, Read},
    path::Path,
    str::FromStr,
};

use lazy_static::lazy_static;
use owners_glob::{PatternOptions, acmatcher::AcMatcher};
#[cfg(feature = "pyo3")]
use pyo3::prelude::*;
#[cfg(feature = "pyo3")]
use pyo3_stub_gen::derive::{gen_stub_pyclass, gen_stub_pymethods};
use regex::Regex;

use crate::{FromPath, FromReader, OwnersOfPath};

/// Various types of owners
///
/// GitHubOwner supports parsing from strings as well as displaying as strings
///
/// # Examples
///
/// ```rust
/// let raw = "@org/team";
/// assert_eq!(
///   raw.parse::<codeowners::GitHubOwner>().unwrap().to_string(),
///   raw
/// );
/// ```
#[derive(Debug, PartialEq, Clone, Eq)]
pub enum GitHubOwner {
    /// Owner in the form @username
    Username(String),
    /// Owner in the form @org/Team
    Team(String),
    /// Owner in the form user@domain.com
    Email(String),
}

impl fmt::Display for GitHubOwner {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let inner = match *self {
            GitHubOwner::Username(ref u) => u,
            GitHubOwner::Team(ref t) => t,
            GitHubOwner::Email(ref e) => e,
        };
        f.write_str(inner.as_str())
    }
}

impl FromStr for GitHubOwner {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        lazy_static! {
            static ref TEAM: Regex = Regex::new(r"^@\S+/\S+").unwrap();
            static ref USERNAME: Regex = Regex::new(r"^@\S+").unwrap();
            static ref EMAIL: Regex = Regex::new(r"^\S+@\S+").unwrap();
        }
        if TEAM.is_match(s) {
            Ok(GitHubOwner::Team(s.into()))
        } else if USERNAME.is_match(s) {
            Ok(GitHubOwner::Username(s.into()))
        } else if EMAIL.is_match(s) {
            Ok(GitHubOwner::Email(s.into()))
        } else {
            Err(String::from("not an owner"))
        }
    }
}

/// Mappings of GitHub owners to path patterns
#[derive(Debug, Clone)]
pub struct GitHubOwners {
    matcher: AcMatcher,
    owners: Vec<Vec<GitHubOwner>>,
}

impl PartialEq for GitHubOwners {
    fn eq(&self, other: &Self) -> bool {
        // Compare the externally observable bits — the input owner mappings.
        // `AcMatcher` is an opaque index over the same data.
        self.owners == other.owners
    }
}

impl Eq for GitHubOwners {}

impl OwnersOfPath for GitHubOwners {
    type Owner = GitHubOwner;

    fn of<P>(&self, path: P) -> Option<Vec<GitHubOwner>>
    where
        P: AsRef<Path>,
    {
        let path_str = path.as_ref().to_string_lossy();
        self.matcher
            .first_match(&path_str)
            .map(|idx| self.owners[idx].clone())
    }
}

impl FromPath for GitHubOwners {
    fn from_path<P>(path: P) -> anyhow::Result<GitHubOwners>
    where
        P: AsRef<Path>,
    {
        Self::from_reader(File::open(path)?)
    }
}

impl FromReader for GitHubOwners {
    /// Parse a CODEOWNERS file from some readable source.
    ///
    /// The format follows GitHub's documentation:
    /// <https://docs.github.com/en/repositories/managing-your-repositorys-settings-and-features/customizing-your-repository/about-code-owners>
    fn from_reader<R>(read: R) -> anyhow::Result<GitHubOwners>
    where
        R: Read,
    {
        let mut entries: Vec<(String, Vec<GitHubOwner>)> = BufReader::new(read)
            .lines()
            /* trunk-ignore(clippy/lines_filter_map_ok) */
            .filter_map(Result::ok)
            .filter(|line| !line.is_empty() && !line.starts_with('#'))
            .filter_map(|line| {
                let mut elements = line.split_whitespace();
                let raw_pattern = elements.next()?.to_owned();
                let owners = elements
                    .filter_map(|o| o.parse().ok())
                    .collect::<Vec<GitHubOwner>>();
                Some((raw_pattern, owners))
            })
            .collect();
        // Last match in the file takes precedence — reverse so index 0 = highest priority.
        entries.reverse();
        let raw_patterns: Vec<&str> = entries.iter().map(|(p, _)| p.as_str()).collect();
        let matcher = AcMatcher::new(raw_patterns, PatternOptions::default())
            .map_err(|e| anyhow::anyhow!("{e}"))?;
        let owners = entries.into_iter().map(|(_, o)| o).collect();
        Ok(GitHubOwners { matcher, owners })
    }
}

#[derive(Debug, PartialEq, Eq)]
#[cfg_attr(feature = "pyo3", gen_stub_pyclass, pyclass)]
pub struct BindingsGitHubOwners(pub GitHubOwners);

#[cfg(feature = "pyo3")]
#[gen_stub_pymethods]
#[pymethods]
impl BindingsGitHubOwners {
    fn of(&self, path: String) -> Option<Vec<String>> {
        let owners = self.0.of(Path::new(&path));
        owners.map(|owners| owners.iter().map(ToString::to_string).collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    const EXAMPLE: &[u8] = include_bytes!("../test_fixtures/github/codeowners_example");

    #[test]
    fn owner_parses() {
        assert!("@user".parse() == Ok(GitHubOwner::Username("@user".into())));
        assert!("@org/team".parse() == Ok(GitHubOwner::Team("@org/team".into())));
        assert!("user@domain.com".parse() == Ok(GitHubOwner::Email("user@domain.com".into())));
        assert!("bogus".parse::<GitHubOwner>() == Err("not an owner".into()));
    }

    #[test]
    fn owner_displays() {
        assert!(GitHubOwner::Username("@user".into()).to_string() == "@user");
        assert!(GitHubOwner::Team("@org/team".into()).to_string() == "@org/team");
        assert!(GitHubOwner::Email("user@domain.com".into()).to_string() == "user@domain.com");
    }

    #[test]
    fn from_reader_parses() {
        let owners = GitHubOwners::from_reader(EXAMPLE).unwrap();
        // 9 rules in the fixture.
        assert_eq!(owners.matcher.len(), 9);
        // Spot-check each rule via behavioral queries.
        assert_eq!(
            owners.of("x/another"),
            Some(vec![GitHubOwner::Username("@bctocat".into())])
        );
        assert_eq!(
            owners.of("x/etc/foo"),
            Some(vec![GitHubOwner::Username("@actocat".into())])
        );
        assert_eq!(
            owners.of("docs/foo.js"),
            Some(vec![GitHubOwner::Username("@doctocat".into())])
        );
        assert_eq!(
            owners.of("x/apps/foo"),
            Some(vec![GitHubOwner::Username("@octocat".into())])
        );
        // docs/* matches files directly inside docs/ at any path depth (unanchored).
        // x/docs/ is not root-level so /docs/ doesn't apply; docs/* → docs@example.com.
        assert_eq!(
            owners.of("x/docs/file.txt"),
            Some(vec![GitHubOwner::Email("docs@example.com".into())])
        );
        assert_eq!(
            owners.of("build/logs/foo"),
            Some(vec![GitHubOwner::Username("@doctocat".into())])
        );
        assert_eq!(
            owners.of("foo.go"),
            Some(vec![GitHubOwner::Email("docs@example.com".into())])
        );
        assert_eq!(
            owners.of("foo.js"),
            Some(vec![GitHubOwner::Username("@js-owner".into())])
        );
        assert_eq!(
            owners.of("anything.txt"),
            Some(vec![
                GitHubOwner::Username("@global-owner1".into()),
                GitHubOwner::Username("@global-owner2".into()),
            ])
        );
    }

    #[test]
    fn owners_owns_wildcard() {
        let owners = GitHubOwners::from_reader(EXAMPLE).unwrap();
        assert_eq!(
            owners.of("foo.txt"),
            Some(vec![
                GitHubOwner::Username("@global-owner1".into()),
                GitHubOwner::Username("@global-owner2".into()),
            ])
        );
        assert_eq!(
            owners.of("foo/bar.txt"),
            Some(vec![
                GitHubOwner::Username("@global-owner1".into()),
                GitHubOwner::Username("@global-owner2".into()),
            ])
        )
    }

    #[test]
    fn owners_owns_js_extention() {
        let owners = GitHubOwners::from_reader(EXAMPLE).unwrap();
        assert_eq!(
            owners.of("foo.js"),
            Some(vec![GitHubOwner::Username("@js-owner".into())])
        );
        assert_eq!(
            owners.of("foo/bar.js"),
            Some(vec![GitHubOwner::Username("@js-owner".into())])
        )
    }

    #[test]
    fn owners_owns_go_extention() {
        let owners = GitHubOwners::from_reader(EXAMPLE).unwrap();
        assert_eq!(
            owners.of("foo.go"),
            Some(vec![GitHubOwner::Email("docs@example.com".into())])
        );
        assert_eq!(
            owners.of("foo/bar.go"),
            Some(vec![GitHubOwner::Email("docs@example.com".into())])
        )
    }

    #[test]
    fn owners_owns_anchored_build_logs() {
        let owners = GitHubOwners::from_reader(EXAMPLE).unwrap();
        assert_eq!(
            owners.of("build/logs/foo.go"),
            Some(vec![GitHubOwner::Username("@doctocat".into())])
        );
        assert_eq!(
            owners.of("build/logs/foo/bar.go"),
            Some(vec![GitHubOwner::Username("@doctocat".into())])
        );
        // not relative to root
        assert_eq!(
            owners.of("foo/build/logs/foo.go"),
            Some(vec![GitHubOwner::Email("docs@example.com".into())])
        )
    }

    #[test]
    fn owners_owns_unanchored_docs() {
        let owners = GitHubOwners::from_reader(EXAMPLE).unwrap();
        // docs anywhere
        assert_eq!(
            owners.of("foo/docs/foo.js"),
            Some(vec![GitHubOwner::Email("docs@example.com".into())])
        );
        assert_eq!(
            owners.of("foo/bar/docs/foo.js"),
            Some(vec![GitHubOwner::Email("docs@example.com".into())])
        );
        // but not nested deeper (docs/* only one level)
        assert_eq!(
            owners.of("foo/bar/docs/foo/foo.js"),
            Some(vec![GitHubOwner::Username("@js-owner".into())])
        )
    }

    #[test]
    fn owners_owns_unanchored_apps() {
        let owners = GitHubOwners::from_reader(EXAMPLE).unwrap();
        assert_eq!(
            owners.of("foo/apps/foo.js"),
            Some(vec![GitHubOwner::Username("@octocat".into())])
        );
        assert_eq!(
            owners.of("./foo/apps/foo.js"),
            Some(vec![GitHubOwner::Username("@octocat".into())])
        )
    }

    #[test]
    fn owners_owns_anchored_docs() {
        let owners = GitHubOwners::from_reader(EXAMPLE).unwrap();
        assert_eq!(
            owners.of("docs/foo.js"),
            Some(vec![GitHubOwner::Username("@doctocat".into())])
        );
        assert_eq!(
            owners.of("./docs/foo.js"),
            Some(vec![GitHubOwner::Username("@doctocat".into())])
        )
    }

    #[test]
    fn implied_children_owners() {
        let owners = GitHubOwners::from_reader("foo/bar @doug".as_bytes()).unwrap();
        assert_eq!(
            owners.of("foo/bar/baz.rs"),
            Some(vec![GitHubOwner::Username("@doug".into())])
        );
    }

    #[test]
    fn relative_path_owners() {
        let owners = GitHubOwners::from_reader("foo/bar @doug".as_bytes()).unwrap();
        assert_eq!(
            owners.of("./foo/bar/baz.rs"),
            Some(vec![GitHubOwner::Username("@doug".into())])
        );
        assert_eq!(owners.of(".foo/bar/baz.rs"), None);
        assert_eq!(owners.of("./"), None);
        assert_eq!(
            owners.of("./foo/bar/baz.rs"),
            Some(vec![GitHubOwner::Username("@doug".into())])
        )
    }

    #[test]
    fn no_implied_children_owners() {
        let owners = GitHubOwners::from_reader("*.go @doug".as_bytes()).unwrap();
        assert_eq!(
            owners.of("foo/bar.go"),
            Some(vec![GitHubOwner::Username("@doug".into())])
        );
        // A file extension pattern should NOT match inside a directory of that name.
        assert_eq!(owners.of("foo/bar.go/baz"), None);
    }

    #[test]
    fn anchored_directory_matches_deeply_nested_files() {
        let codeowners = r#"
/src @owner1
/src/components @owner2
"#;
        let owners = GitHubOwners::from_reader(codeowners.as_bytes()).unwrap();

        assert_eq!(
            owners.of("src/components/ui/buttons/primary_button.rs"),
            Some(vec![GitHubOwner::Username("@owner2".into())])
        );
        assert_eq!(
            owners.of("src/components/foo.rs"),
            Some(vec![GitHubOwner::Username("@owner2".into())])
        );
        assert_eq!(
            owners.of("src/other/file.rs"),
            Some(vec![GitHubOwner::Username("@owner1".into())])
        );
    }

    #[test]
    fn specific_pattern_wins_over_general_for_nested_files() {
        let codeowners = r#"
* @fallback-owner
/src @general-owner @extra-owner
/src/components @specific-owner
/src/components/special @very-specific-owner
"#;
        let owners = GitHubOwners::from_reader(codeowners.as_bytes()).unwrap();
        let result = owners.of("src/components/nested/deeply/file.rs").unwrap();
        assert_eq!(
            result,
            vec![GitHubOwner::Username("@specific-owner".into())]
        );
    }
}
