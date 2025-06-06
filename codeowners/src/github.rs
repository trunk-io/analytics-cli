use std::{
    fmt,
    fs::File,
    io::{BufRead, BufReader, Read},
    path::Path,
    str::FromStr,
};

use glob::Pattern;
use lazy_static::lazy_static;
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
#[derive(Debug, PartialEq, Clone, Eq)]
pub struct GitHubOwners {
    paths: Vec<(Pattern, Vec<GitHubOwner>)>,
}

impl OwnersOfPath for GitHubOwners {
    type Owner = GitHubOwner;

    fn of<P>(&self, path: P) -> Option<Vec<GitHubOwner>>
    where
        P: AsRef<Path>,
    {
        self.paths
            .iter()
            .filter_map(|mapping| {
                let (pattern, owners) = mapping;
                let opts = glob::MatchOptions {
                    case_sensitive: false,
                    require_literal_separator: pattern.as_str().contains('/'),
                    require_literal_leading_dot: false,
                };
                if pattern.matches_path_with(path.as_ref(), opts) {
                    Some(owners)
                } else {
                    // if the path is relative, we need to strip the leading dot
                    // to match the pattern
                    if let Ok(simplified_path) = path.as_ref().strip_prefix("./") {
                        if pattern.matches_path_with(simplified_path, opts) {
                            return Some(owners);
                        }
                    }
                    // this pattern is only meant to match
                    // direct children
                    if pattern.as_str().ends_with("/*") {
                        return None;
                    }
                    // case of implied owned children
                    // foo/bar @owner should indicate that foo/bar/baz.rs is
                    // owned by @owner
                    let mut p = path.as_ref();
                    while let Some(parent) = p.parent() {
                        if pattern.matches_path_with(parent, opts) {
                            return Some(owners);
                        } else {
                            p = parent;
                        }
                    }
                    None
                }
            })
            .next()
            .cloned()
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
    /// Parse a CODEOWNERS file from some readable source
    /// This format is defined in
    /// [Github's documentation](https://help.github.com/articles/about-codeowners/)
    /// The syntax is uses gitgnore
    /// [patterns](https://www.kernel.org/pub/software/scm/git/docs/gitignore.html#_pattern_format)
    /// followed by an identifier for an owner. More information can be found
    /// [here](https://help.github.com/articles/about-codeowners/#codeowners-syntax)
    fn from_reader<R>(read: R) -> anyhow::Result<GitHubOwners>
    where
        R: Read,
    {
        let mut paths = BufReader::new(read)
            .lines()
            /* trunk-ignore(clippy/lines_filter_map_ok) */
            .filter_map(Result::ok)
            .filter(|line| !line.is_empty() && !line.starts_with('#'))
            .try_fold(Vec::new(), |mut paths, line| -> anyhow::Result<_> {
                let mut elements = line.split_whitespace();
                if let Some(path) = elements.next() {
                    let owners = elements.fold(Vec::new(), |mut result, owner| {
                        if let Ok(owner) = owner.parse() {
                            result.push(owner)
                        }
                        result
                    });
                    paths.push((pattern(path)?, owners))
                }
                Ok(paths)
            })?;
        // last match takes precedence
        paths.reverse();
        Ok(GitHubOwners { paths })
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

fn pattern(path: &str) -> anyhow::Result<Pattern> {
    // if pattern starts with anchor or explicit wild card, it should
    // match any prefix
    let prefixed = if path.starts_with('*') || path.starts_with('/') {
        path.to_owned()
    } else {
        format!("**/{}", path)
    };
    // if pattern starts with anchor it should only match paths
    // relative to root
    let mut normalized = prefixed.trim_start_matches('/').to_string();
    // if pattern ends with /, it should match children of that directory
    if normalized.ends_with('/') {
        normalized.push_str("**");
    }
    Pattern::new(&normalized).map_err(anyhow::Error::msg)
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
        assert_eq!(
            owners,
            GitHubOwners {
                paths: vec![
                    (
                        Pattern::new("docs/**").unwrap(),
                        vec![GitHubOwner::Username("@doctocat".into())]
                    ),
                    (
                        Pattern::new("**/apps/**").unwrap(),
                        vec![GitHubOwner::Username("@octocat".into())]
                    ),
                    (
                        Pattern::new("**/docs/*").unwrap(),
                        vec![GitHubOwner::Email("docs@example.com".into())]
                    ),
                    (
                        Pattern::new("build/logs/**").unwrap(),
                        vec![GitHubOwner::Username("@doctocat".into())]
                    ),
                    (
                        Pattern::new("*.go").unwrap(),
                        vec![GitHubOwner::Email("docs@example.com".into())]
                    ),
                    (
                        Pattern::new("*.js").unwrap(),
                        vec![GitHubOwner::Username("@js-owner".into())]
                    ),
                    (
                        Pattern::new("*").unwrap(),
                        vec![
                            GitHubOwner::Username("@global-owner1".into()),
                            GitHubOwner::Username("@global-owner2".into()),
                        ]
                    ),
                ],
            }
        )
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
        // relative to root
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
        // but not nested
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
        // relative to root
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
        assert_eq!(owners.of(".foo/bar/baz.rs"), None,);
        assert_eq!(owners.of("./"), None,);
        assert_eq!(
            owners.of("./foo/bar/baz.rs"),
            Some(vec![GitHubOwner::Username("@doug".into())])
        )
    }
}
