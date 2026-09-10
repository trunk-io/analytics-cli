//! Where a test is *declared*, rather than where a failure surfaced — all an `.xcresult`
//! records (see [`crate::file_attribution`]). `documentSymbol` names the type containing
//! each method and, unlike `workspace/symbol`, needs no index and no build.

use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

use ignore::{WalkBuilder, types::TypesBuilder};
use lsp_types::{DocumentSymbol, SymbolKind};

use crate::{file_attribution::ReportedPath, lsp::LanguageServer, xcrun::xcrun_find};

/// Kinds that can declare a test.
const METHOD_KINDS: [SymbolKind; 3] = [
    SymbolKind::METHOD,
    SymbolKind::CONSTRUCTOR,
    SymbolKind::FUNCTION,
];
/// Kinds that can contain one. `INTERFACE` is how an Objective-C category arrives.
const CONTAINER_KINDS: [SymbolKind; 3] =
    [SymbolKind::CLASS, SymbolKind::INTERFACE, SymbolKind::STRUCT];

const SWIFT_EXTENSIONS: [&str; 1] = ["swift"];
const CLANG_EXTENSIONS: [&str; 5] = ["m", "mm", "c", "cc", "cpp"];

/// Directories holding something other than the repo's own code. Exact target ownership
/// would need a build log, and reading one costs the legacy call this path exists to avoid.
const SKIPPED_DIRECTORIES: [&str; 9] = [
    ".git",
    ".build",
    ".swiftpm",
    "build",
    "checkouts",
    "Carthage",
    "DerivedData",
    "node_modules",
    "Pods",
];

/// Every field is settable from the CLI, because the right value depends on the repo: the
/// clang server answers roughly an order of magnitude slower per file than the Swift one,
/// so an Objective-C heavy checkout needs more of all of them than these defaults give.
#[derive(Debug, Clone, Copy)]
pub struct Limits {
    pub max_files: usize,
    /// Spent per server kind rather than across both, so a large Swift tree cannot leave
    /// the clang server with nothing left to parse Objective-C in.
    pub budget: Duration,
    pub request_timeout: Duration,
    /// How many times a server that stops answering is replaced with a fresh one.
    pub retries: usize,
    /// Skipped above this, unread. `didOpen` carries the whole file, so a generated source
    /// file costs its own size twice over and will usually exhaust `request_timeout` as
    /// well — taking a retry and the rest of the budget with it, for one file's symbols.
    pub max_file_bytes: u64,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            max_files: 2_000,
            budget: Duration::from_secs(60),
            request_timeout: Duration::from_secs(30),
            retries: 1,
            // Comfortably above hand-written source, and low enough that the generated files
            // that reach megabytes -- which declare no tests -- are not read.
            max_file_bytes: 2 * 1_024 * 1_024,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct TestKey {
    suite: Option<String>,
    case: String,
}

impl TestKey {
    /// `Outer/Inner/case()` — only the innermost suite declares the method, and a top-level
    /// swift-testing test has none.
    pub fn from_node_identifier(node_identifier: &str) -> Self {
        let mut components = node_identifier.rsplit('/');
        let case = components.next().unwrap_or_default();
        Self {
            suite: components
                .next()
                .map(|suite| container_name(suite).to_string()),
            case: normalized_case(case),
        }
    }
}

impl TestKey {
    /// `test://com.apple.xcode/<scheme>/<target>/<suite>/<case>` — the second component is
    /// the test bundle, which is the only thing distinguishing two same-named suites.
    pub fn target_from_identifier_url(identifier_url: &str) -> Option<String> {
        let path = identifier_url.split("://").nth(1)?;
        let target = path.split('/').nth(2)?;
        (!target.is_empty()).then(|| percent_decoded(target))
    }
}

fn percent_decoded(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%'
            && let Some(byte) = value
                .get(index + 1..index + 3)
                .and_then(|hex| u8::from_str_radix(hex, 16).ok())
        {
            decoded.push(byte);
            index += 3;
        } else {
            decoded.push(bytes[index]);
            index += 1;
        }
    }
    String::from_utf8(decoded).unwrap_or_else(|_| value.to_string())
}

/// Whether a path lies under a directory named for the target, which is how a candidate is
/// tied to the module that actually ran it.
fn is_in_target(file: &ReportedPath, target: &str) -> bool {
    Path::new(file.as_str())
        .components()
        .any(|component| component.as_os_str().to_string_lossy() == target)
}

#[derive(Debug, Clone)]
pub struct DeclarationSite {
    pub file: ReportedPath,
    pub line: Option<u32>,
}

#[derive(Debug, Default)]
pub struct TestLocationIndex {
    declarations: HashMap<TestKey, DeclarationSite>,
    /// Where each suite is declared, for a test that its own suite does not declare.
    suites: HashMap<String, DeclarationSite>,
    targets: HashMap<TestKey, String>,
}

impl TestLocationIndex {
    pub fn resolve(repo_root: &Path, keys: &[(TestKey, Option<String>)], limits: Limits) -> Self {
        let targets = keys
            .iter()
            .filter_map(|(key, target)| target.clone().map(|target| (key.clone(), target)))
            .collect::<HashMap<_, _>>();
        let keys = keys.iter().map(|(key, _)| key.clone()).collect::<Vec<_>>();
        let suites = keys
            .iter()
            .filter_map(|key| key.suite.as_deref())
            .collect::<HashSet<_>>();
        let sources = scan_sources(repo_root, &suites, limits.max_files, limits.max_file_bytes);
        let (swift, clang) = sources
            .into_iter()
            .partition::<Vec<_>, _>(|path| has_extension(path, &SWIFT_EXTENSIONS));

        let mut resolver = Resolver {
            index: Self {
                targets,
                ..Self::default()
            },
            unresolved: keys.clone(),
            limits,
        };
        resolver.parse(&swift, &SOURCEKIT_LSP, repo_root);
        resolver.parse(&clang, &CLANGD, repo_root);
        if !resolver.unresolved.is_empty() {
            tracing::debug!(
                "{} of {} test(s) have no declaration in the checkout",
                resolver.unresolved.len(),
                keys.len()
            );
        }
        resolver.index
    }

    /// The file the test is written in, preferring the declaration of the method itself —
    /// a suite split across extensions declares each test in its own file.
    ///
    /// A test run under a suite that does not declare it was inherited: a suite cannot run a
    /// method it does not have, so the declaration is in some base class and that is the file
    /// the test is written in. No inheritance graph is needed to reach that conclusion — the
    /// test having run is the proof.
    ///
    /// One is needed to act on it whenever more than one class declares the case, though,
    /// because an override is a declaration too: see [`Self::inherited_declaration`]. Where
    /// nothing declares it, or several things do, the suite's own file stands in.
    pub fn lookup(&self, key: &TestKey) -> Option<&DeclarationSite> {
        if let Some(site) = self.method_declaration(key) {
            return Some(site);
        }
        if let Some(site) = self.inherited_declaration(key) {
            return Some(site);
        }
        match key.suite.as_ref() {
            Some(suite) => self.suites.get(suite),
            // A top-level swift-testing test has no suite, so the function is all there is.
            None => None,
        }
    }

    /// The one declaration of `key`'s case under some other suite, which for a test that ran
    /// is the base class it was inherited from.
    ///
    /// Only answers when there is exactly one candidate. Nothing here can tell an *ancestor*
    /// that declares the method from an unrelated *sibling* that overrides it — an override
    /// is a declaration of the same name, so by name alone the two are the same fact, and
    /// separating them would need the inheritance graph, which is semantic and would cost a
    /// build. So more than one candidate reports nothing and lets the suite's own file stand
    /// in, rather than picking a file the test may well not be written in.
    ///
    /// Scoped to the target that ran it, since two modules can each declare a case of the
    /// same name and those are never the same method.
    fn inherited_declaration(&self, key: &TestKey) -> Option<&DeclarationSite> {
        let target = self.targets.get(key);
        let mut candidates: Vec<&DeclarationSite> = Vec::new();
        for (candidate, site) in &self.declarations {
            if candidate.case != key.case || candidate.suite == key.suite {
                continue;
            }
            if let Some(target) = target
                && !is_in_target(&site.file, target)
            {
                continue;
            }
            // Two suites declaring it in one file is still one answer.
            if !candidates.iter().any(|found| found.file == site.file) {
                candidates.push(site);
            }
        }

        if candidates.len() > 1 {
            // Sorted because the candidates come out of a `HashMap`, and a warning that
            // names the same files in a different order every run is hard to trust.
            let mut files = candidates
                .iter()
                .map(|site| site.file.as_str())
                .collect::<Vec<_>>();
            files.sort_unstable();
            tracing::warn!(
                "{} declares no {}, and {} places declare one -- any of them could be what it \
                 inherited, so its own file is reported instead of guessing between {}",
                key.suite.as_deref().unwrap_or("a top-level test"),
                key.case,
                files.len(),
                files.join(", ")
            );
            return None;
        }
        candidates.first().copied()
    }

    /// Only the method's own declaration, which is what decides whether there is still
    /// something worth parsing for.
    ///
    /// Resolution stops once nothing is left to find, so it cannot be driven by
    /// [`Self::lookup`]: a suite is usually declared in the first file ranked for it, and
    /// counting that as an answer would end the scan before the file the *method* is
    /// declared in was ever read — collapsing every test to its suite's file.
    fn method_declaration(&self, key: &TestKey) -> Option<&DeclarationSite> {
        self.declarations.get(key)
    }

    pub fn is_empty(&self) -> bool {
        self.declarations.is_empty()
    }

    fn record(&mut self, key: TestKey, candidate: DeclarationSite) {
        match self.declarations.get(&key) {
            None => {
                self.declarations.insert(key, candidate);
            }
            Some(existing) => {
                if let Some(target) = self.targets.get(&key)
                    && is_in_target(&candidate.file, target)
                    && !is_in_target(&existing.file, target)
                {
                    tracing::debug!(
                        "preferring {} over {} for target {}",
                        candidate.file.as_str(),
                        existing.file.as_str(),
                        target
                    );
                    self.declarations.insert(key, candidate);
                }
            }
        }
    }

    fn collect(&mut self, symbols: &[DocumentSymbol], file: &Path, container: Option<&str>) {
        for symbol in symbols {
            let site = || DeclarationSite {
                file: ReportedPath::new(&file.to_string_lossy()),
                line: declaration_line(symbol),
            };
            if CONTAINER_KINDS.contains(&symbol.kind) {
                // An Objective-C category is reported against the class it extends, which
                // is already declared elsewhere, so the first declaration seen wins.
                self.suites
                    .entry(container_name(&symbol.name).to_string())
                    .or_insert_with(site);
            }
            if METHOD_KINDS.contains(&symbol.kind) {
                let key = TestKey {
                    suite: container.map(|name| container_name(name).to_string()),
                    case: normalized_case(&symbol.name),
                };
                self.record(key, site());
            }
            if let Some(children) = symbol.children.as_deref() {
                self.collect(children, file, Some(&symbol.name));
            }
        }
    }
}

struct ServerKind {
    program: &'static str,
    args: &'static [&'static str],
    language_id: &'static str,
}

const SOURCEKIT_LSP: ServerKind = ServerKind {
    program: "sourcekit-lsp",
    args: &[],
    language_id: "swift",
};

/// Background indexing is the work `documentSymbol` exists to avoid.
const CLANGD: ServerKind = ServerKind {
    program: "clangd",
    args: &["--background-index=false"],
    language_id: "objective-c",
};

struct Resolver {
    index: TestLocationIndex,
    unresolved: Vec<TestKey>,
    limits: Limits,
}

impl Resolver {
    /// Parse `files` with one `kind` of server, replacing it when it stops answering.
    ///
    /// A server that times out is killed rather than resynchronised, so the only way to
    /// carry on is a fresh one. The file that broke it is skipped instead of retried: it
    /// is the reason the last server died, and retrying it would spend the whole budget
    /// re-earning the same timeout.
    fn parse(&mut self, files: &[PathBuf], kind: &ServerKind, root: &Path) {
        if files.is_empty() || self.unresolved.is_empty() {
            return;
        }
        let Some(program) = xcrun_find(kind.program) else {
            tracing::warn!(
                "{} not found; {} source file(s) left unparsed",
                kind.program,
                files.len()
            );
            return;
        };

        let deadline = Instant::now() + self.limits.budget;
        let mut remaining = files;
        let mut parsed = 0;
        for attempt in 0..=self.limits.retries {
            if remaining.is_empty() || self.unresolved.is_empty() || Instant::now() >= deadline {
                break;
            }
            if attempt > 0 {
                tracing::warn!(
                    "{}: restarting it, {} file(s) left to parse",
                    kind.program,
                    remaining.len()
                );
            }
            let mut server =
                match LanguageServer::start(&program, kind.args, root, self.limits.request_timeout)
                {
                    Ok(server) => server,
                    Err(e) => {
                        tracing::warn!("failed to start {}: {}", kind.program, e);
                        return;
                    }
                };

            let mut consumed = 0;
            for file in remaining {
                consumed += 1;
                if self.unresolved.is_empty() {
                    break;
                }
                if Instant::now() >= deadline {
                    tracing::warn!(
                        "{}: out of time after {} file(s), {} left unparsed",
                        kind.program,
                        parsed,
                        files.len() - parsed
                    );
                    return;
                }
                // The file is read inside `document_symbols`, in chunks -- an unreadable or
                // non-UTF-8 one is reported there and answers with no symbols, as it did
                // when the read happened here.
                let symbols =
                    server.document_symbols(file, kind.language_id, self.limits.request_timeout);
                if server.is_broken() {
                    break;
                }
                let Some(symbols) = symbols else {
                    continue;
                };
                parsed += 1;
                self.index.collect(&symbols, file, None);
                let index = &self.index;
                self.unresolved
                    .retain(|key| index.method_declaration(key).is_none());
            }
            remaining = &remaining[consumed.min(remaining.len())..];
            if !server.is_broken() {
                break;
            }
        }
        if !remaining.is_empty() && !self.unresolved.is_empty() {
            tracing::warn!(
                "{}: gave up with {} file(s) unparsed",
                kind.program,
                remaining.len()
            );
        }
        tracing::debug!(
            "{}: parsed {} of {} file(s)",
            kind.program,
            parsed,
            files.len()
        );
    }
}

/// LSP counts lines from zero; everything downstream counts from one.
fn declaration_line(symbol: &DocumentSymbol) -> Option<u32> {
    let range = symbol.selection_range;
    range.start.line.checked_add(1)
}

/// Applied to identifier and symbol alike: Swift spells it `testExample()`, Objective-C
/// `-testExample`.
fn normalized_case(name: &str) -> String {
    name.trim_start_matches(['+', '-'])
        .trim_end_matches(['(', ')'])
        .to_string()
}

/// An Objective-C category comes back as `Suite(Category)`; identifiers name the class.
fn container_name(name: &str) -> &str {
    name.split('(').next().unwrap_or(name)
}

fn has_extension(path: &Path, extensions: &[&str]) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .map(|extension| extensions.contains(&extension))
        .unwrap_or(false)
}

/// Files most likely to pay off first: a suite is overwhelmingly declared in a file named
/// after it, and parsing stops once every test resolves. Symlinked directories are skipped.
///
/// The extensions are registered explicitly rather than taken from `ignore`'s built-in
/// definitions, because those map `.h` to Objective-C and a header is not something the
/// clang server can answer `documentSymbol` for on its own.
///
/// `.gitignore` already excludes most of [`SKIPPED_DIRECTORIES`] in a normal checkout, but
/// nothing guarantees it, so the list stays as an override on top.
fn scan_sources(
    repo_root: &Path,
    suites: &HashSet<&str>,
    max_files: usize,
    max_file_bytes: u64,
) -> Vec<PathBuf> {
    let mut types = TypesBuilder::new();
    for extension in SWIFT_EXTENSIONS.iter().chain(CLANG_EXTENSIONS.iter()) {
        // `add` only fails on a malformed glob, and these are built from literals.
        let _ = types.add("sources", &format!("*.{extension}"));
    }
    types.select("sources");
    let Ok(types) = types.build() else {
        return Vec::new();
    };

    let walker = WalkBuilder::new(repo_root)
        .types(types)
        .hidden(false)
        .follow_links(false)
        .filter_entry(|entry| {
            entry.depth() == 0
                || !SKIPPED_DIRECTORIES.contains(&entry.file_name().to_string_lossy().as_ref())
        })
        .build();

    // Counted rather than logged per file: a repo with a generated-sources directory would
    // otherwise emit a warning for every one of them.
    let mut oversized = 0_usize;
    let mut found = walker
        .flatten()
        .filter(|entry| {
            entry
                .file_type()
                .is_some_and(|file_type| file_type.is_file())
        })
        .filter(|entry| {
            // The walker has already stat'd the entry, so the size costs nothing extra, and
            // failing to read it here means the read would have failed anyway.
            match entry.metadata() {
                Ok(metadata) if metadata.len() > max_file_bytes => {
                    oversized += 1;
                    false
                }
                _ => true,
            }
        })
        .map(|entry| entry.into_path())
        .collect::<Vec<_>>();
    if oversized > 0 {
        tracing::warn!("{oversized} source file(s) skipped for exceeding {max_file_bytes} bytes");
    }
    found.sort_by_cached_key(|path| (rank(path, suites), path.clone()));
    found.truncate(max_files);
    found
}

fn rank(path: &Path, suites: &HashSet<&str>) -> u8 {
    let Some(stem) = path
        .file_stem()
        .map(|stem| stem.to_string_lossy().to_string())
    else {
        return 2;
    };
    if suites.contains(stem.as_str()) {
        0
    } else if suites.iter().any(|suite| stem.contains(suite)) {
        1
    } else {
        2
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use rstest::rstest;
    use serde_json::{Value, json};
    use temp_testdir::TempDir;

    use super::*;

    const SWIFT_FILE: &str = "/repo/Tests/SnapshotReproTests.swift";

    fn key(suite: Option<&str>, case: &str) -> TestKey {
        TestKey {
            suite: suite.map(String::from),
            case: String::from(case),
        }
    }

    fn symbols(value: Value) -> Vec<DocumentSymbol> {
        serde_json::from_value(value).unwrap()
    }

    fn span(from: u64, to: u64) -> Value {
        json!({
            "start": { "line": from, "character": 0 },
            "end": { "line": to, "character": 0 }
        })
    }

    fn method(name: &str, line: u64) -> Value {
        json!({
            "name": name,
            "kind": SymbolKind::METHOD,
            "range": span(line, line),
            "selectionRange": span(line, line)
        })
    }

    fn container(name: &str, kind: SymbolKind, lines: (u64, u64), children: Vec<Value>) -> Value {
        json!({
            "name": name,
            "kind": kind,
            "range": span(lines.0, lines.1),
            "selectionRange": span(lines.0, lines.0),
            "children": children
        })
    }

    fn indexed(file: &str, value: Value) -> TestLocationIndex {
        let mut index = TestLocationIndex::default();
        index.collect(&symbols(value), Path::new(file), None);
        index
    }

    #[rstest]
    #[case::swift_xctest(
        "SnapshotReproTests/testExample()",
        Some("SnapshotReproTests"),
        "testExample"
    )]
    #[case::objc_has_no_parens(
        "ObjcXCTestTests/testExample",
        Some("ObjcXCTestTests"),
        "testExample"
    )]
    #[case::top_level_swift_testing("failingSnapshot()", None, "failingSnapshot")]
    #[case::only_the_innermost_suite_declares(
        "OuterSuite/InnerSuite/testExample()",
        Some("InnerSuite"),
        "testExample"
    )]
    #[case::parameterized_keeps_its_labels(
        "SnapshotReproTests/testExample(input:)",
        Some("SnapshotReproTests"),
        "testExample(input:"
    )]
    fn an_identifier_names_a_suite_and_a_case(
        #[case] node_identifier: &str,
        #[case] suite: Option<&str>,
        #[case] case: &str,
    ) {
        assert_eq!(
            TestKey::from_node_identifier(node_identifier),
            key(suite, case)
        );
    }

    // The identifier and the symbol are spelled differently in every language, so what
    // matters is that normalizing both lands them on the same key.
    #[rstest]
    #[case::swift("SnapshotReproTests/testExample()", "testExample()")]
    #[case::objc("ObjcXCTestTests/testExample", "-testExample")]
    #[case::objc_class_method("ObjcXCTestTests/testExample", "+testExample")]
    #[case::parameterized("Suite/testExample(input:)", "testExample(input:)")]
    fn an_identifier_and_its_symbol_normalize_alike(
        #[case] node_identifier: &str,
        #[case] symbol_name: &str,
    ) {
        assert_eq!(
            TestKey::from_node_identifier(node_identifier).case,
            normalized_case(symbol_name)
        );
    }

    // Resolution stops when nothing is left to find, so if the suite fallback counted as
    // an answer the scan would end at the first file naming the suite and never read the
    // one declaring the method. The two lookups have to disagree here.
    #[test]
    fn the_suite_fallback_does_not_count_as_a_resolved_declaration() {
        let index = indexed(
            SWIFT_FILE,
            json!([container(
                "SnapshotReproTests",
                SymbolKind::CLASS,
                (0, 2),
                vec![]
            )]),
        );
        let key = key(Some("SnapshotReproTests"), "testExample");
        assert!(
            index.method_declaration(&key).is_none(),
            "the method is declared nowhere yet, so there is still work to do"
        );
        assert!(
            index.lookup(&key).is_some(),
            "but the suite is known, so a file can still be reported for it"
        );
    }

    // A suite cannot run a method it does not have, so a case arriving under a suite that
    // declares no such method was inherited, and the one declaration of that name is the base
    // class it came from. Nothing else in the checkout can be the answer.
    #[test]
    fn a_suite_that_does_not_declare_its_case_inherited_it() {
        let index = indexed(
            SWIFT_FILE,
            json!([container(
                "BaseTests",
                SymbolKind::CLASS,
                (0, 2),
                vec![method("testExample()", 1)]
            )]),
        );
        assert_eq!(
            index
                .lookup(&key(Some("ChildTests"), "testExample"))
                .map(|site| site.line),
            Some(Some(2)),
            "the inherited method resolves to the line it is written on"
        );
    }

    // An override is its own declaration, so it answers before anything is inherited.
    #[test]
    fn an_override_keeps_its_own_declaration() {
        let index = indexed(
            SWIFT_FILE,
            json!([
                container(
                    "BaseTests",
                    SymbolKind::CLASS,
                    (0, 2),
                    vec![method("testExample()", 1)]
                ),
                container(
                    "ChildTests",
                    SymbolKind::CLASS,
                    (3, 5),
                    vec![method("testExample()", 4)]
                )
            ]),
        );
        assert_eq!(
            index
                .lookup(&key(Some("ChildTests"), "testExample"))
                .map(|site| site.line),
            Some(Some(5))
        );
    }

    // The shape that matters in practice, and the reason this cannot be done by name: a
    // sibling overriding the method declares it too. `ChildA` inherits from `Base` and
    // `ChildB` overrides, so `testInherited` is declared in two files and neither the
    // ancestor nor the sibling can be told apart from the case name -- which is what the
    // `swift-test-xunit` fixture captures.
    #[test]
    fn a_sibling_override_makes_the_inherited_declaration_ambiguous() {
        let mut index = indexed(
            "/repo/Tests/MyCLITests/BaseTests.swift",
            json!([container(
                "BaseTests",
                SymbolKind::CLASS,
                (0, 2),
                vec![method("testInherited()", 1)]
            )]),
        );
        index.collect(
            &symbols(json!([container(
                "ChildBTests",
                SymbolKind::CLASS,
                (0, 2),
                vec![method("testInherited()", 1)]
            )])),
            Path::new("/repo/Tests/MyCLITests/ChildBTests.swift"),
            None,
        );
        index.suites.insert(
            String::from("ChildATests"),
            site("/repo/Tests/MyCLITests/ChildATests.swift"),
        );
        let key = key(Some("ChildATests"), "testInherited");
        index
            .targets
            .insert(key.clone(), String::from("MyCLITests"));

        assert!(
            index.inherited_declaration(&key).is_none(),
            "the ancestor and the override are indistinguishable by name"
        );
        assert_eq!(
            index.lookup(&key).map(|site| site.file.as_str().to_owned()),
            Some(String::from("/repo/Tests/MyCLITests/ChildATests.swift")),
            "so the suite's own file stands in"
        );
    }

    // Two unrelated suites declare the case in different files, so which one a third
    // inherited it from is unknowable here. Guessing would report a file the test is not
    // written in, so the suite's own file stands in instead.
    #[test]
    fn an_ambiguous_inheritance_declines_rather_than_guessing() {
        let mut index = indexed(
            SWIFT_FILE,
            json!([container(
                "OneTests",
                SymbolKind::CLASS,
                (0, 2),
                vec![method("testExample()", 1)]
            )]),
        );
        index.collect(
            &symbols(json!([container(
                "TwoTests",
                SymbolKind::CLASS,
                (0, 2),
                vec![method("testExample()", 1)]
            )])),
            Path::new("/repo/Tests/TwoTests.swift"),
            None,
        );
        index.suites.insert(
            String::from("ThirdTests"),
            site("/repo/Tests/ThirdTests.swift"),
        );

        assert_eq!(
            index
                .lookup(&key(Some("ThirdTests"), "testExample"))
                .map(|site| site.file.as_str().to_owned()),
            Some(String::from("/repo/Tests/ThirdTests.swift"))
        );
    }

    // The same-named case in another module is not what this one inherited, so the target
    // has to bound the search even when only one candidate exists.
    #[test]
    fn a_case_of_the_same_name_in_another_target_is_not_inherited() {
        let mut index = indexed(
            "/repo/Tests/OtherFeatureTests/OtherTests.swift",
            json!([container(
                "OtherTests",
                SymbolKind::CLASS,
                (0, 2),
                vec![method("testExample()", 1)]
            )]),
        );
        let key = key(Some("ChildTests"), "testExample");
        index
            .targets
            .insert(key.clone(), String::from("MyFeatureTests"));

        assert!(
            index.lookup(&key).is_none(),
            "no declaration under MyFeatureTests, and no suite either"
        );
    }

    #[test]
    fn the_scan_ranks_suite_named_files_first_and_skips_vendored_directories() {
        let root = TempDir::default();
        for relative in [
            "Sources/Alpha.swift",
            "Tests/SnapshotReproTests.swift",
            "Tests/SnapshotReproTestsHelper.swift",
            "Pods/Vendored.swift",
            ".build/checkouts/Dep/Dep.swift",
        ] {
            let path = root.join(relative);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, "").unwrap();
        }
        let suites = HashSet::from(["SnapshotReproTests"]);
        let scanned = scan_sources(root.as_ref(), &suites, 10, u64::MAX)
            .iter()
            .map(|path| path.file_name().unwrap().to_string_lossy().to_string())
            .collect::<Vec<_>>();
        assert_eq!(
            scanned,
            vec![
                "SnapshotReproTests.swift",
                "SnapshotReproTestsHelper.swift",
                "Alpha.swift"
            ]
        );
    }

    #[test]
    fn the_scan_stops_at_the_file_cap() {
        let root = TempDir::default();
        for name in ["A.swift", "B.swift", "C.swift"] {
            fs::write(root.join(name), "").unwrap();
        }
        assert_eq!(
            scan_sources(root.as_ref(), &HashSet::new(), 2, u64::MAX).len(),
            2
        );
    }

    // The size cap is on the scan rather than the read, so an outsized file never reaches
    // `read_to_string` at all -- and the cap is a ceiling, not a rounding.
    #[test]
    fn the_scan_skips_a_file_over_the_byte_cap() {
        let root = TempDir::default();
        fs::write(root.join("Small.swift"), "ab").unwrap();
        fs::write(root.join("Big.swift"), "abcd").unwrap();

        let scanned = |max_file_bytes| {
            scan_sources(root.as_ref(), &HashSet::new(), 10, max_file_bytes)
                .iter()
                .map(|path| path.file_name().unwrap().to_string_lossy().to_string())
                .collect::<Vec<_>>()
        };

        assert_eq!(scanned(2), vec![String::from("Small.swift")]);
        assert_eq!(scanned(4).len(), 2, "a file exactly at the cap is parsed");
    }

    fn site(file: &str) -> DeclarationSite {
        DeclarationSite {
            file: ReportedPath::new(file),
            line: None,
        }
    }

    fn recorded(target: Option<&str>, files: [&str; 2]) -> String {
        let test_key = key(Some("FooTests"), "testThing");
        let mut index = TestLocationIndex {
            targets: target
                .map(|target| HashMap::from([(test_key.clone(), String::from(target))]))
                .unwrap_or_default(),
            ..Default::default()
        };
        for file in files {
            index.record(test_key.clone(), site(file));
        }
        index.lookup(&test_key).unwrap().file.as_str().to_owned()
    }

    const IN_TARGET: &str = "/repo/Tests/AppTests/FooTests.swift";
    const OTHER_TARGET: &str = "/repo/Tests/LibTests/FooTests.swift";

    // Two modules can declare the same `(suite, case)`, and the scan order that decides it is
    // arbitrary, so the target the test actually ran under has to break the tie.
    #[rstest]
    #[case::target_scanned_second(Some("AppTests"), [OTHER_TARGET, IN_TARGET], IN_TARGET)]
    #[case::target_scanned_first(Some("AppTests"), [IN_TARGET, OTHER_TARGET], IN_TARGET)]
    #[case::no_target_keeps_the_first(None, [OTHER_TARGET, IN_TARGET], OTHER_TARGET)]
    #[case::no_candidate_matches(Some("TestsNobodyHas"), [OTHER_TARGET, IN_TARGET], OTHER_TARGET)]
    fn a_collision_resolves_to_the_target_that_ran_the_test(
        #[case] target: Option<&str>,
        #[case] files: [&str; 2],
        #[case] expected: &str,
    ) {
        assert_eq!(recorded(target, files), expected);
    }
}
