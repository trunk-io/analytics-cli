use std::{
    fmt,
    fs::File,
    io::{BufRead, BufReader, Read},
    path::Path,
    str::FromStr,
};

use glob::{MatchOptions, Pattern};
use lazy_static::lazy_static;
use once_cell::sync::Lazy;
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
    paths: Vec<(PatternWithFallback, Vec<GitHubOwner>)>,
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
                    require_literal_separator: pattern.contains('/'),
                    require_literal_leading_dot: false,
                };
                if pattern.matches_path_with(path.as_ref(), opts) {
                    Some(owners)
                } else {
                    // NOTE: if the path is relative, we need to strip the leading dot
                    // to match the pattern. We are doing this as a workaround for
                    // cases where the provided path is relative, like `./foo/bar/baz.rs`
                    if let Ok(simplified_path) = path.as_ref().strip_prefix("./") {
                        if pattern.matches_path_with(simplified_path, opts) {
                            return Some(owners);
                        }
                    }
                    if pattern.is_direct_children() {
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

#[derive(Debug, PartialEq, Clone, Eq)]
pub struct PatternWithFallback {
    base: Pattern,
    fallback: Option<Pattern>,
}

impl PatternWithFallback {
    pub fn new(base: &str) -> anyhow::Result<Self> {
        let result = if base.ends_with("**") && !base.ends_with("/**") && base.len() > 2 {
            let un_wildcarded = base.strip_suffix("**").unwrap_or(base);
            let base_pattern =
                Pattern::new(format!("{}*", un_wildcarded).as_str()).map_err(anyhow::Error::msg)?;
            let fallback_pattern = Pattern::new(format!("{}*/**", un_wildcarded).as_str())
                .map_err(anyhow::Error::msg)?;

            Self {
                base: base_pattern,
                fallback: Some(fallback_pattern),
            }
        } else {
            // Matches anything that ends with neither a slash nor a period nor an asterisk,
            // see test pattern_with_fallback for cases
            static FALLBACK_NEEDED_REGEX: Lazy<Regex> =
                Lazy::new(|| Regex::new(r"^\/?(.*\/)*((\.?)[^\/\.\*]+)$").unwrap());

            let base_pattern = Pattern::new(base).map_err(anyhow::Error::msg)?;
            let mut fallback_pattern = None;
            if FALLBACK_NEEDED_REGEX.is_match(base) {
                let mut subdir_match = base.to_string();
                subdir_match.push_str("/**");
                fallback_pattern = Pattern::new(&subdir_match).ok();
            }

            Self {
                base: base_pattern,
                fallback: fallback_pattern,
            }
        };

        Ok(result)
    }

    pub fn matches_path_with(&self, path: &Path, options: MatchOptions) -> bool {
        self.base.matches_path_with(path, options)
            || self
                .fallback
                .as_ref()
                .is_some_and(|fallback| fallback.matches_path_with(path, options))
    }

    pub fn is_direct_children(&self) -> bool {
        self.fallback.is_none() && self.base.as_str().ends_with("/*")
    }

    pub fn contains(&self, sub: char) -> bool {
        self.base.as_str().contains(sub)
            || self
                .fallback
                .as_ref()
                .is_some_and(|fallback| fallback.as_str().contains(sub))
    }
}

fn pattern(path: &str) -> anyhow::Result<PatternWithFallback> {
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
    PatternWithFallback::new(&normalized).map_err(anyhow::Error::msg)
}

#[cfg(test)]
mod tests {
    use super::*;
    const EXAMPLE: &[u8] = include_bytes!("../test_fixtures/github/codeowners_example");

    #[test]
    fn pattern_with_fallback() {
        assert_eq!(
            PatternWithFallback::new("*").unwrap(),
            PatternWithFallback {
                base: Pattern::new("*").unwrap(),
                fallback: None,
            },
        );

        assert_eq!(
            PatternWithFallback::new("*.js").unwrap(),
            PatternWithFallback {
                base: Pattern::new("*.js").unwrap(),
                fallback: None,
            },
        );

        assert_eq!(
            PatternWithFallback::new("abc/**").unwrap(),
            PatternWithFallback {
                base: Pattern::new("abc/**").unwrap(),
                fallback: None,
            },
        );

        assert_eq!(
            PatternWithFallback::new(".xyz").unwrap(),
            PatternWithFallback {
                base: Pattern::new(".xyz").unwrap(),
                fallback: Some(Pattern::new(".xyz/**").unwrap()),
            },
        );
        assert_eq!(
            PatternWithFallback::new(".xyz/").unwrap(),
            PatternWithFallback {
                base: Pattern::new(".xyz/").unwrap(),
                fallback: None,
            },
        );
        assert_eq!(
            PatternWithFallback::new(".xyz.js").unwrap(),
            PatternWithFallback {
                base: Pattern::new(".xyz.js").unwrap(),
                fallback: None,
            },
        );

        assert_eq!(
            PatternWithFallback::new("/.abc/xyz").unwrap(),
            PatternWithFallback {
                base: Pattern::new("/.abc/xyz").unwrap(),
                fallback: Some(Pattern::new("/.abc/xyz/**").unwrap()),
            },
        );
        assert_eq!(
            PatternWithFallback::new("/.abc/xyz/").unwrap(),
            PatternWithFallback {
                base: Pattern::new("/.abc/xyz/").unwrap(),
                fallback: None,
            },
        );
        assert_eq!(
            PatternWithFallback::new("/.abc/xyz.js").unwrap(),
            PatternWithFallback {
                base: Pattern::new("/.abc/xyz.js").unwrap(),
                fallback: None,
            },
        );

        assert_eq!(
            PatternWithFallback::new(".xyz").unwrap(),
            PatternWithFallback {
                base: Pattern::new(".xyz").unwrap(),
                fallback: Some(Pattern::new(".xyz/**").unwrap()),
            },
        );
        assert_eq!(
            PatternWithFallback::new(".xyz/").unwrap(),
            PatternWithFallback {
                base: Pattern::new(".xyz/").unwrap(),
                fallback: None,
            },
        );
        assert_eq!(
            PatternWithFallback::new(".xyz.js").unwrap(),
            PatternWithFallback {
                base: Pattern::new(".xyz.js").unwrap(),
                fallback: None,
            },
        );

        assert_eq!(
            PatternWithFallback::new("/abc/.xyz").unwrap(),
            PatternWithFallback {
                base: Pattern::new("/abc/.xyz").unwrap(),
                fallback: Some(Pattern::new("/abc/.xyz/**").unwrap()),
            },
        );
        assert_eq!(
            PatternWithFallback::new("/abc/.xyz/").unwrap(),
            PatternWithFallback {
                base: Pattern::new("/abc/.xyz/").unwrap(),
                fallback: None,
            },
        );
        assert_eq!(
            PatternWithFallback::new("/abc/.xyz.js").unwrap(),
            PatternWithFallback {
                base: Pattern::new("/abc/.xyz.js").unwrap(),
                fallback: None,
            },
        );

        assert_eq!(
            PatternWithFallback::new("abc/**/xyz").unwrap(),
            PatternWithFallback {
                base: Pattern::new("abc/**/xyz").unwrap(),
                fallback: Some(Pattern::new("abc/**/xyz/**").unwrap()),
            },
        );
        assert_eq!(
            PatternWithFallback::new("abc/**/xyz/").unwrap(),
            PatternWithFallback {
                base: Pattern::new("abc/**/xyz/").unwrap(),
                fallback: None,
            },
        );
        assert_eq!(
            PatternWithFallback::new("abc/**/xyz.js").unwrap(),
            PatternWithFallback {
                base: Pattern::new("abc/**/xyz.js").unwrap(),
                fallback: None,
            },
        );

        assert_eq!(
            PatternWithFallback::new("/abc/xyz").unwrap(),
            PatternWithFallback {
                base: Pattern::new("/abc/xyz").unwrap(),
                fallback: Some(Pattern::new("/abc/xyz/**").unwrap()),
            },
        );
        assert_eq!(
            PatternWithFallback::new("/abc/xyz/").unwrap(),
            PatternWithFallback {
                base: Pattern::new("/abc/xyz/").unwrap(),
                fallback: None,
            },
        );
        assert_eq!(
            PatternWithFallback::new("/abc/xyz.js").unwrap(),
            PatternWithFallback {
                base: Pattern::new("/abc/xyz.js").unwrap(),
                fallback: None,
            },
        );

        assert_eq!(
            PatternWithFallback::new("xyz").unwrap(),
            PatternWithFallback {
                base: Pattern::new("xyz").unwrap(),
                fallback: Some(Pattern::new("xyz/**").unwrap()),
            },
        );
        assert_eq!(
            PatternWithFallback::new("xyz/").unwrap(),
            PatternWithFallback {
                base: Pattern::new("xyz/").unwrap(),
                fallback: None,
            },
        );
        assert_eq!(
            PatternWithFallback::new("xyz.js").unwrap(),
            PatternWithFallback {
                base: Pattern::new("xyz.js").unwrap(),
                fallback: None,
            },
        );

        assert_eq!(
            PatternWithFallback::new("abc/xyz").unwrap(),
            PatternWithFallback {
                base: Pattern::new("abc/xyz").unwrap(),
                fallback: Some(Pattern::new("abc/xyz/**").unwrap()),
            },
        );
        assert_eq!(
            PatternWithFallback::new("abc/xyz/").unwrap(),
            PatternWithFallback {
                base: Pattern::new("abc/xyz/").unwrap(),
                fallback: None,
            },
        );
        assert_eq!(
            PatternWithFallback::new("abc/xyz.js").unwrap(),
            PatternWithFallback {
                base: Pattern::new("abc/xyz.js").unwrap(),
                fallback: None,
            },
        );

        assert_eq!(
            PatternWithFallback::new("/a bc/xyz").unwrap(),
            PatternWithFallback {
                base: Pattern::new("/a bc/xyz").unwrap(),
                fallback: Some(Pattern::new("/a bc/xyz/**").unwrap()),
            },
        );
        assert_eq!(
            PatternWithFallback::new("/a bc/xyz/").unwrap(),
            PatternWithFallback {
                base: Pattern::new("/a bc/xyz/").unwrap(),
                fallback: None,
            },
        );
        assert_eq!(
            PatternWithFallback::new("/a bc/xyz.js").unwrap(),
            PatternWithFallback {
                base: Pattern::new("/a bc/xyz.js").unwrap(),
                fallback: None,
            },
        );

        assert_eq!(
            PatternWithFallback::new("/abc/x yz").unwrap(),
            PatternWithFallback {
                base: Pattern::new("/abc/x yz").unwrap(),
                fallback: Some(Pattern::new("/abc/x yz/**").unwrap()),
            },
        );
        assert_eq!(
            PatternWithFallback::new("/abc/x yz/").unwrap(),
            PatternWithFallback {
                base: Pattern::new("/abc/x yz/").unwrap(),
                fallback: None,
            },
        );
        assert_eq!(
            PatternWithFallback::new("/abc/x yz.js").unwrap(),
            PatternWithFallback {
                base: Pattern::new("/abc/x yz.js").unwrap(),
                fallback: None,
            },
        );

        assert_eq!(
            PatternWithFallback::new("/abc**").unwrap(),
            PatternWithFallback {
                base: Pattern::new("/abc*").unwrap(),
                fallback: Some(Pattern::new("/abc*/**").unwrap()),
            },
        );

        assert_eq!(
            PatternWithFallback::new("/**").unwrap(),
            PatternWithFallback {
                base: Pattern::new("/**").unwrap(),
                fallback: None,
            },
        );

        assert_eq!(
            PatternWithFallback::new("**").unwrap(),
            PatternWithFallback {
                base: Pattern::new("**").unwrap(),
                fallback: None,
            },
        );
    }

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
                        PatternWithFallback {
                            base: Pattern::new("**/another").unwrap(),
                            fallback: Some(Pattern::new("**/another/**").unwrap()),
                        },
                        vec![GitHubOwner::Username("@bctocat".into())]
                    ),
                    (
                        PatternWithFallback {
                            base: Pattern::new("**/etc/**").unwrap(),
                            fallback: None,
                        },
                        vec![GitHubOwner::Username("@actocat".into())]
                    ),
                    (
                        PatternWithFallback {
                            base: Pattern::new("docs/**").unwrap(),
                            fallback: None,
                        },
                        vec![GitHubOwner::Username("@doctocat".into())]
                    ),
                    (
                        PatternWithFallback {
                            base: Pattern::new("**/apps/**").unwrap(),
                            fallback: None,
                        },
                        vec![GitHubOwner::Username("@octocat".into())]
                    ),
                    (
                        PatternWithFallback {
                            base: Pattern::new("**/docs/*").unwrap(),
                            fallback: None,
                        },
                        vec![GitHubOwner::Email("docs@example.com".into())]
                    ),
                    (
                        PatternWithFallback {
                            base: Pattern::new("build/logs/**").unwrap(),
                            fallback: None,
                        },
                        vec![GitHubOwner::Username("@doctocat".into())]
                    ),
                    (
                        PatternWithFallback {
                            base: Pattern::new("*.go").unwrap(),
                            fallback: None,
                        },
                        vec![GitHubOwner::Email("docs@example.com".into())]
                    ),
                    (
                        PatternWithFallback {
                            base: Pattern::new("*.js").unwrap(),
                            fallback: None,
                        },
                        vec![GitHubOwner::Username("@js-owner".into())]
                    ),
                    (
                        PatternWithFallback {
                            base: Pattern::new("*").unwrap(),
                            fallback: None,
                        },
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
        assert_eq!(owners.of(".foo/bar/baz.rs"), None);
        assert_eq!(owners.of("./"), None);
        assert_eq!(
            owners.of("./foo/bar/baz.rs"),
            Some(vec![GitHubOwner::Username("@doug".into())])
        )
    }
}
