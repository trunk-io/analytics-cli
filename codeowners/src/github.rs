use std::sync::Arc;
use std::{
    fmt,
    fs::File,
    io::{BufRead, BufReader, Read},
    path::Path,
    str::FromStr,
};

use lazy_static::lazy_static;
use once_cell::sync::Lazy;
#[cfg(feature = "pyo3")]
use pyo3::prelude::*;
#[cfg(feature = "pyo3")]
use pyo3_stub_gen::derive::{gen_stub_pyclass, gen_stub_pymethods};
use regex::Regex;
use zlob::{ZlobFlags, ZlobPattern};

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
        let path_str = path.as_ref().to_string_lossy();
        // Strip ./ prefix once so patterns don't need to account for it.
        let path_lower = path_str
            .strip_prefix("./")
            .unwrap_or(&path_str)
            .to_lowercase();

        self.paths
            .iter()
            .find(|(pattern, _)| pattern.matches_str(&path_lower))
            .map(|(_, owners)| owners.clone())
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

pub struct PatternWithFallback {
    base_str: String,
    base: Arc<ZlobPattern>,
    fallback_str: Option<String>,
    fallback: Option<Arc<ZlobPattern>>,
}

impl PartialEq for PatternWithFallback {
    fn eq(&self, other: &Self) -> bool {
        self.base_str == other.base_str && self.fallback_str == other.fallback_str
    }
}
impl Eq for PatternWithFallback {}

impl Clone for PatternWithFallback {
    fn clone(&self) -> Self {
        Self {
            base_str: self.base_str.clone(),
            base: self.base.clone(),
            fallback_str: self.fallback_str.clone(),
            fallback: self.fallback.clone(),
        }
    }
}

impl fmt::Debug for PatternWithFallback {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("PatternWithFallback")
            .field("base", &self.base_str)
            .field("fallback", &self.fallback_str)
            .finish()
    }
}

const GLOB_FLAGS: ZlobFlags =
    ZlobFlags::from_bits_retain(ZlobFlags::DOUBLESTAR_RECURSIVE.bits() | ZlobFlags::PERIOD.bits());

/// Patterns like `*.js` or `*` start with `*` (not `**`) and have no `/`, so
/// they need a `**/` prefix — zlob's `*` won't cross path separators.
fn normalize_zlob(pattern: &str) -> String {
    if pattern.starts_with('*') && !pattern.starts_with("**") && !pattern.contains('/') {
        format!("**/{}", pattern)
    } else {
        pattern.to_string()
    }
}

fn compile_zlob(pattern: &str) -> anyhow::Result<Arc<ZlobPattern>> {
    ZlobPattern::compile(pattern, GLOB_FLAGS)
        .map(Arc::new)
        .map_err(|e| anyhow::anyhow!("{:?}", e))
}

impl PatternWithFallback {
    pub fn new(base: &str) -> anyhow::Result<Self> {
        let result = if base.ends_with("**") && !base.ends_with("/**") && base.len() > 2 {
            let un_wildcarded = base.strip_suffix("**").unwrap_or(base);
            let base_str = normalize_zlob(&format!("{}*", un_wildcarded).to_lowercase());
            let fallback_str = format!("{}*/**", un_wildcarded).to_lowercase();
            Self {
                base: compile_zlob(&base_str)?,
                fallback: Some(compile_zlob(&fallback_str)?),
                base_str,
                fallback_str: Some(fallback_str),
            }
        } else {
            // Matches anything that ends with neither a slash nor a period nor an asterisk,
            // see test pattern_with_fallback for cases
            static FALLBACK_NEEDED_REGEX: Lazy<Regex> =
                Lazy::new(|| Regex::new(r"^\/?(.*\/)*((\.?)[^\/\.\*]+)$").unwrap());

            let base_str = normalize_zlob(&base.to_lowercase());
            let base_compiled = compile_zlob(&base_str)?;

            let (fallback_str, fallback) = if FALLBACK_NEEDED_REGEX.is_match(base) {
                let f = format!("{}/**", base).to_lowercase();
                let compiled = compile_zlob(&f)?;
                (Some(f), Some(compiled))
            } else {
                (None, None)
            };

            Self {
                base_str,
                base: base_compiled,
                fallback_str,
                fallback,
            }
        };

        Ok(result)
    }

    fn matches_str(&self, path: &str) -> bool {
        self.base.matches(path, GLOB_FLAGS)
            || self
                .fallback
                .as_ref()
                .is_some_and(|fallback| fallback.matches(path, GLOB_FLAGS))
    }

    #[allow(dead_code)]
    pub fn matches_path(&self, path: &Path) -> bool {
        self.matches_str(&path.to_string_lossy().to_lowercase())
    }

    #[allow(dead_code)]
    pub fn is_direct_children(&self) -> bool {
        self.fallback_str.is_none() && self.base_str.ends_with("/*")
    }

    pub fn contains(&self, sub: char) -> bool {
        self.base_str.contains(sub) || self.fallback_str.as_ref().is_some_and(|f| f.contains(sub))
    }

    pub fn base_str(&self) -> &str {
        &self.base_str
    }

    pub fn fallback_str(&self) -> Option<&str> {
        self.fallback_str.as_deref()
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

    impl PatternWithFallback {
        fn with_strs(base: &str, fallback: Option<&str>) -> Self {
            let base_lower = base.to_lowercase();
            let fallback_lower = fallback.map(|f| f.to_lowercase());
            Self {
                base: Arc::new(ZlobPattern::compile(&base_lower, GLOB_FLAGS).unwrap()),
                base_str: base_lower,
                fallback: fallback_lower
                    .as_deref()
                    .map(|f| Arc::new(ZlobPattern::compile(f, GLOB_FLAGS).unwrap())),
                fallback_str: fallback_lower,
            }
        }

        fn base_matches_path(&self, path: &Path) -> bool {
            let path_lower = path.to_string_lossy().to_lowercase();
            self.base.matches(&path_lower, GLOB_FLAGS)
        }
    }

    #[test]
    fn pattern_with_fallback() {
        // Patterns starting with `*` (not `**`) with no `/` get a `**/` prefix
        // so that single `*` doesn't need to cross path separators in zlob.
        assert_eq!(
            PatternWithFallback::new("*").unwrap(),
            PatternWithFallback::with_strs("**/*", None),
        );
        assert_eq!(
            PatternWithFallback::new("*.js").unwrap(),
            PatternWithFallback::with_strs("**/*.js", None),
        );

        assert_eq!(
            PatternWithFallback::new("abc/**").unwrap(),
            PatternWithFallback::with_strs("abc/**", None),
        );

        assert_eq!(
            PatternWithFallback::new(".xyz").unwrap(),
            PatternWithFallback::with_strs(".xyz", Some(".xyz/**")),
        );
        assert_eq!(
            PatternWithFallback::new(".xyz/").unwrap(),
            PatternWithFallback::with_strs(".xyz/", None),
        );
        assert_eq!(
            PatternWithFallback::new(".xyz.js").unwrap(),
            PatternWithFallback::with_strs(".xyz.js", None),
        );

        assert_eq!(
            PatternWithFallback::new("/.abc/xyz").unwrap(),
            PatternWithFallback::with_strs("/.abc/xyz", Some("/.abc/xyz/**")),
        );
        assert_eq!(
            PatternWithFallback::new("/.abc/xyz/").unwrap(),
            PatternWithFallback::with_strs("/.abc/xyz/", None),
        );
        assert_eq!(
            PatternWithFallback::new("/.abc/xyz.js").unwrap(),
            PatternWithFallback::with_strs("/.abc/xyz.js", None),
        );

        assert_eq!(
            PatternWithFallback::new("/abc/.xyz").unwrap(),
            PatternWithFallback::with_strs("/abc/.xyz", Some("/abc/.xyz/**")),
        );
        assert_eq!(
            PatternWithFallback::new("/abc/.xyz/").unwrap(),
            PatternWithFallback::with_strs("/abc/.xyz/", None),
        );
        assert_eq!(
            PatternWithFallback::new("/abc/.xyz.js").unwrap(),
            PatternWithFallback::with_strs("/abc/.xyz.js", None),
        );

        assert_eq!(
            PatternWithFallback::new("abc/**/xyz").unwrap(),
            PatternWithFallback::with_strs("abc/**/xyz", Some("abc/**/xyz/**")),
        );
        assert_eq!(
            PatternWithFallback::new("abc/**/xyz/").unwrap(),
            PatternWithFallback::with_strs("abc/**/xyz/", None),
        );
        assert_eq!(
            PatternWithFallback::new("abc/**/xyz.js").unwrap(),
            PatternWithFallback::with_strs("abc/**/xyz.js", None),
        );

        assert_eq!(
            PatternWithFallback::new("/abc/xyz").unwrap(),
            PatternWithFallback::with_strs("/abc/xyz", Some("/abc/xyz/**")),
        );
        assert_eq!(
            PatternWithFallback::new("/abc/xyz/").unwrap(),
            PatternWithFallback::with_strs("/abc/xyz/", None),
        );
        assert_eq!(
            PatternWithFallback::new("/abc/xyz.js").unwrap(),
            PatternWithFallback::with_strs("/abc/xyz.js", None),
        );

        assert_eq!(
            PatternWithFallback::new("xyz").unwrap(),
            PatternWithFallback::with_strs("xyz", Some("xyz/**")),
        );
        assert_eq!(
            PatternWithFallback::new("xyz/").unwrap(),
            PatternWithFallback::with_strs("xyz/", None),
        );
        assert_eq!(
            PatternWithFallback::new("xyz.js").unwrap(),
            PatternWithFallback::with_strs("xyz.js", None),
        );

        assert_eq!(
            PatternWithFallback::new("abc/xyz").unwrap(),
            PatternWithFallback::with_strs("abc/xyz", Some("abc/xyz/**")),
        );
        assert_eq!(
            PatternWithFallback::new("abc/xyz/").unwrap(),
            PatternWithFallback::with_strs("abc/xyz/", None),
        );
        assert_eq!(
            PatternWithFallback::new("abc/xyz.js").unwrap(),
            PatternWithFallback::with_strs("abc/xyz.js", None),
        );

        assert_eq!(
            PatternWithFallback::new("/a bc/xyz").unwrap(),
            PatternWithFallback::with_strs("/a bc/xyz", Some("/a bc/xyz/**")),
        );
        assert_eq!(
            PatternWithFallback::new("/a bc/xyz/").unwrap(),
            PatternWithFallback::with_strs("/a bc/xyz/", None),
        );
        assert_eq!(
            PatternWithFallback::new("/a bc/xyz.js").unwrap(),
            PatternWithFallback::with_strs("/a bc/xyz.js", None),
        );

        assert_eq!(
            PatternWithFallback::new("/abc/x yz").unwrap(),
            PatternWithFallback::with_strs("/abc/x yz", Some("/abc/x yz/**")),
        );
        assert_eq!(
            PatternWithFallback::new("/abc/x yz/").unwrap(),
            PatternWithFallback::with_strs("/abc/x yz/", None),
        );
        assert_eq!(
            PatternWithFallback::new("/abc/x yz.js").unwrap(),
            PatternWithFallback::with_strs("/abc/x yz.js", None),
        );

        assert_eq!(
            PatternWithFallback::new("/abc**").unwrap(),
            PatternWithFallback::with_strs("/abc*", Some("/abc*/**")),
        );

        assert_eq!(
            PatternWithFallback::new("/**").unwrap(),
            PatternWithFallback::with_strs("/**", None),
        );

        assert_eq!(
            PatternWithFallback::new("**").unwrap(),
            PatternWithFallback::with_strs("**", None),
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
                        PatternWithFallback::with_strs("**/another", Some("**/another/**")),
                        vec![GitHubOwner::Username("@bctocat".into())]
                    ),
                    (
                        PatternWithFallback::with_strs("**/etc/**", None),
                        vec![GitHubOwner::Username("@actocat".into())]
                    ),
                    (
                        PatternWithFallback::with_strs("docs/**", None),
                        vec![GitHubOwner::Username("@doctocat".into())]
                    ),
                    (
                        PatternWithFallback::with_strs("**/apps/**", None),
                        vec![GitHubOwner::Username("@octocat".into())]
                    ),
                    (
                        PatternWithFallback::with_strs("**/docs/*", None),
                        vec![GitHubOwner::Email("docs@example.com".into())]
                    ),
                    (
                        PatternWithFallback::with_strs("build/logs/**", None),
                        vec![GitHubOwner::Username("@doctocat".into())]
                    ),
                    (
                        PatternWithFallback::with_strs("**/*.go", None),
                        vec![GitHubOwner::Email("docs@example.com".into())]
                    ),
                    (
                        PatternWithFallback::with_strs("**/*.js", None),
                        vec![GitHubOwner::Username("@js-owner".into())]
                    ),
                    (
                        PatternWithFallback::with_strs("**/*", None),
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

    #[test]
    fn anchored_directory_matches_deeply_nested_files() {
        let codeowners = r#"
/src @owner1
/src/components @owner2
"#;
        let owners = GitHubOwners::from_reader(codeowners.as_bytes()).unwrap();

        // Deeply nested file should match /src/components (the more specific pattern)
        assert_eq!(
            owners.of("src/components/ui/buttons/primary_button.rs"),
            Some(vec![GitHubOwner::Username("@owner2".into())])
        );

        // Less deeply nested should also work
        assert_eq!(
            owners.of("src/components/foo.rs"),
            Some(vec![GitHubOwner::Username("@owner2".into())])
        );

        // Direct child should work
        assert_eq!(
            owners.of("src/components/file.rs"),
            Some(vec![GitHubOwner::Username("@owner2".into())])
        );

        // src/ alone should match owner1
        assert_eq!(
            owners.of("src/other/file.rs"),
            Some(vec![GitHubOwner::Username("@owner1".into())])
        );
    }

    #[test]
    fn pattern_fallback_for_anchored_directory() {
        // /src/components should create base "src/components" with fallback "src/components/**"
        let pat = pattern("/src/components").unwrap();
        assert_eq!(pat.base_str(), "src/components");
        assert!(pat.fallback_str().is_some());
        assert_eq!(pat.fallback_str().unwrap(), "src/components/**");

        let nested_path = Path::new("src/components/ui/buttons/primary_button.rs");

        // Base alone should NOT match
        assert!(!pat.base_matches_path(nested_path));

        // Fallback should make the combined match succeed
        assert!(
            pat.matches_path(nested_path),
            "Fallback pattern 'src/components/**' should match deeply nested path"
        );
    }

    #[test]
    fn parent_walking_matches_intermediate_paths() {
        let pat = pattern("/src/components").unwrap();

        // The base pattern should match the exact directory path
        assert!(
            pat.base_matches_path(Path::new("src/components")),
            "Base pattern 'src/components' should match exact path 'src/components'"
        );

        // The fallback should match intermediate paths under the directory
        assert!(
            PatternWithFallback::with_strs(pat.fallback_str().unwrap(), None)
                .matches_path(Path::new("src/components/ui")),
            "Fallback 'src/components/**' should match 'src/components/ui'"
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

        // Deeply nested file should match /src/components, not /src or *
        let file_path = "src/components/nested/deeply/file.rs";
        let result = owners.of(file_path);

        assert!(result.is_some(), "File should have owners");
        let result_owners = result.unwrap();

        // Should get exactly 1 owner from /src/components
        assert_eq!(
            result_owners.len(),
            1,
            "Should have 1 owner from /src/components, got: {result_owners:?}",
        );

        let owner_strings: Vec<String> = result_owners.iter().map(|o| o.to_string()).collect();

        // Should NOT contain @extra-owner (that's from /src)
        assert!(
            !owner_strings.iter().any(|o| o.contains("extra-owner")),
            "Should NOT contain @extra-owner (that's from /src). Got: {:?}",
            owner_strings
        );

        // Should NOT contain @fallback-owner (that's from *)
        assert!(
            !owner_strings.iter().any(|o| o.contains("fallback-owner")),
            "Should NOT contain @fallback-owner (that's from *). Got: {:?}",
            owner_strings
        );

        // Should contain the specific owner
        assert!(owner_strings.contains(&"@specific-owner".to_string()));
    }
}
