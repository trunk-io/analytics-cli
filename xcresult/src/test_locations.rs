//! Where a test is *declared*, rather than where a failure surfaced — all an `.xcresult`
//! records (see [`crate::file_attribution`]). `documentSymbol` names the type containing
//! each method and, unlike `workspace/symbol`, needs no index and no build.

use std::{
    collections::{HashMap, HashSet},
    fs,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

use lazy_static::lazy_static;
use serde::Deserialize;

use crate::{file_attribution::ReportedPath, lsp::LanguageServer, xcrun::find_program};

/// LSP `SymbolKind`s that can declare a test: Method, Constructor, Function.
const METHOD_KINDS: [u64; 3] = [6, 9, 12];
/// Kinds that can contain one: Class, Interface (an Objective-C category), Struct.
const CONTAINER_KINDS: [u64; 3] = [5, 11, 23];

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

#[derive(Debug, Clone, Copy)]
pub struct Limits {
    pub max_files: usize,
    pub budget: Duration,
    pub request_timeout: Duration,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            max_files: 2_000,
            budget: Duration::from_secs(60),
            request_timeout: Duration::from_secs(30),
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
    /// `classname` is the target plus the dot-qualified suite path (`MyCLITests.Outer.Inner`),
    /// collapsing to the bare target for a top-level `@Test func`, which declares no suite.
    pub fn from_junit_classname(classname: &str, name: &str) -> Self {
        let mut components = classname.rsplit('.');
        let innermost = components.next().unwrap_or_default();
        let has_suite = components.next().is_some();
        Self {
            suite: has_suite.then(|| container_name(innermost).to_string()),
            case: normalized_case(name),
        }
    }

    /// The first component, which tells same-named suites in different modules apart.
    pub fn target_from_junit_classname(classname: &str) -> Option<String> {
        let target = classname.split('.').next()?;
        (!target.is_empty()).then(|| target.to_string())
    }

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
    supertypes: HashMap<String, String>,
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
        let sources = scan_sources(repo_root, &suites, limits.max_files);
        let (swift, clang) = sources
            .into_iter()
            .partition::<Vec<_>, _>(|path| has_extension(path, &SWIFT_EXTENSIONS));

        let mut resolver = Resolver {
            index: Self {
                targets,
                ..Self::default()
            },
            unresolved: keys.clone(),
            deadline: Instant::now() + limits.budget,
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

    /// A test can be declared on a base class and run under a subclass.
    pub fn lookup(&self, key: &TestKey) -> Option<&DeclarationSite> {
        let mut suite = key.suite.clone();
        let mut seen = HashSet::new();
        while let Some(current) = suite {
            if !seen.insert(current.clone()) {
                break;
            }
            let inherited = TestKey {
                suite: Some(current.clone()),
                case: key.case.clone(),
            };
            if let Some(site) = self.declarations.get(&inherited) {
                return Some(site);
            }
            suite = self.supertypes.get(&current).cloned();
        }
        self.declarations.get(&TestKey {
            suite: None,
            case: key.case.clone(),
        })
    }

    pub fn is_empty(&self) -> bool {
        self.declarations.is_empty()
    }

    /// Seed an index without a language server, so the code downstream of it can be tested
    /// off macOS.
    #[cfg(test)]
    pub(crate) fn declaring(mut self, node_identifier: &str, file: &str) -> Self {
        self.declarations.insert(
            TestKey::from_node_identifier(node_identifier),
            DeclarationSite {
                file: ReportedPath::new(file),
                line: None,
            },
        );
        self
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

    fn collect(
        &mut self,
        symbols: &[DocumentSymbol],
        file: &Path,
        text: &str,
        container: Option<&str>,
    ) {
        for symbol in symbols {
            if CONTAINER_KINDS.contains(&symbol.kind) {
                let name = container_name(&symbol.name);
                if let Some(supertype) = superclass(text, &symbol.range)
                    && supertype != name
                {
                    self.supertypes.entry(name.to_string()).or_insert(supertype);
                }
            }
            if METHOD_KINDS.contains(&symbol.kind) {
                let key = TestKey {
                    suite: container.map(|name| container_name(name).to_string()),
                    case: normalized_case(&symbol.name),
                };
                let candidate = DeclarationSite {
                    file: ReportedPath::new(&file.to_string_lossy()),
                    line: symbol.declaration_line(),
                };
                self.record(key, candidate);
            }
            self.collect(&symbol.children, file, text, Some(&symbol.name));
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
    deadline: Instant,
    limits: Limits,
}

impl Resolver {
    fn parse(&mut self, files: &[PathBuf], kind: &ServerKind, root: &Path) {
        if files.is_empty() || self.unresolved.is_empty() {
            return;
        }
        let Some(program) = find_program(kind.program) else {
            tracing::warn!(
                "{} not found; {} source file(s) left unparsed",
                kind.program,
                files.len()
            );
            return;
        };
        let mut server =
            match LanguageServer::start(&program, kind.args, root, self.limits.request_timeout) {
                Ok(server) => server,
                Err(e) => {
                    tracing::warn!("failed to start {}: {}", kind.program, e);
                    return;
                }
            };

        let mut parsed = 0;
        for file in files {
            if self.unresolved.is_empty() || server.is_broken() {
                break;
            }
            if Instant::now() >= self.deadline {
                tracing::warn!(
                    "{}: out of time after {} file(s), {} left unparsed",
                    kind.program,
                    parsed,
                    files.len() - parsed
                );
                break;
            }
            let Ok(text) = fs::read_to_string(file) else {
                continue;
            };
            let Some(response) =
                server.document_symbols(file, kind.language_id, &text, self.limits.request_timeout)
            else {
                continue;
            };
            parsed += 1;
            match serde_json::from_value::<Vec<DocumentSymbol>>(response) {
                Ok(symbols) => self.index.collect(&symbols, file, &text, None),
                Err(e) => tracing::debug!("unusable symbols for {}: {}", file.display(), e),
            }
            let index = &self.index;
            self.unresolved.retain(|key| index.lookup(key).is_none());
        }
        tracing::debug!(
            "{}: parsed {} of {} file(s)",
            kind.program,
            parsed,
            files.len()
        );
    }
}

#[derive(Debug, Deserialize)]
struct DocumentSymbol {
    name: String,
    kind: u64,
    range: Range,
    #[serde(rename = "selectionRange")]
    selection_range: Option<Range>,
    #[serde(default)]
    children: Vec<DocumentSymbol>,
}

impl DocumentSymbol {
    /// LSP counts lines from zero; everything downstream counts from one.
    fn declaration_line(&self) -> Option<u32> {
        let range = self.selection_range.as_ref().unwrap_or(&self.range);
        u32::try_from(range.start.line)
            .ok()
            .map(|line| line.saturating_add(1))
    }
}

#[derive(Debug, Deserialize)]
struct Range {
    start: Position,
    end: Position,
}

#[derive(Debug, Deserialize)]
struct Position {
    line: u64,
}

lazy_static! {
    // `\b` sits inside the alternation: before `@interface` it would demand a word
    // character ahead of the `@` and never match a declaration starting a line.
    static ref SUPERCLASS: regex::Regex =
        regex::Regex::new(r"(?:\bclass|@interface)\s+\w+\s*:\s*([A-Za-z_]\w*)").unwrap();
}

/// Enough to carry an inheritance clause, so a large class body is never searched.
const DECLARATION_HEAD_LINES: usize = 5;

fn superclass(text: &str, range: &Range) -> Option<String> {
    let span = (range.end.line.saturating_sub(range.start.line) as usize).saturating_add(1);
    let head = text
        .lines()
        .skip(range.start.line as usize)
        .take(span.min(DECLARATION_HEAD_LINES))
        .collect::<Vec<_>>()
        .join("\n");
    SUPERCLASS
        .captures(&head)
        .and_then(|captures| captures.get(1))
        .map(|name| name.as_str().to_string())
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
fn scan_sources(repo_root: &Path, suites: &HashSet<&str>, max_files: usize) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let mut stack = vec![repo_root.to_path_buf()];
    while let Some(directory) = stack.pop() {
        let Ok(entries) = fs::read_dir(&directory) else {
            continue;
        };
        for entry in entries.flatten() {
            let Ok(file_type) = entry.file_type() else {
                continue;
            };
            if file_type.is_dir() {
                if !SKIPPED_DIRECTORIES.contains(&entry.file_name().to_string_lossy().as_ref()) {
                    stack.push(entry.path());
                }
            } else if file_type.is_file() {
                let path = entry.path();
                if has_extension(&path, &SWIFT_EXTENSIONS)
                    || has_extension(&path, &CLANG_EXTENSIONS)
                {
                    found.push(path);
                }
            }
        }
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

    fn method(name: &str, line: u64) -> Value {
        json!({
            "name": name,
            "kind": 6,
            "range": { "start": { "line": line }, "end": { "line": line } },
            "selectionRange": { "start": { "line": line }, "end": { "line": line } }
        })
    }

    fn container(name: &str, kind: u64, lines: (u64, u64), children: Vec<Value>) -> Value {
        json!({
            "name": name,
            "kind": kind,
            "range": { "start": { "line": lines.0 }, "end": { "line": lines.1 } },
            "children": children
        })
    }

    fn indexed(file: &str, text: &str, value: Value) -> TestLocationIndex {
        let mut index = TestLocationIndex::default();
        index.collect(&symbols(value), Path::new(file), text, None);
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

    #[test]
    fn a_method_is_recorded_against_the_type_declaring_it() {
        let index = indexed(
            SWIFT_FILE,
            "final class SnapshotReproTests: XCTestCase {\n    func testExample() {}\n}",
            json!([container(
                "SnapshotReproTests",
                5,
                (0, 2),
                vec![method("testExample()", 1)]
            )]),
        );
        let site = index
            .lookup(&key(Some("SnapshotReproTests"), "testExample"))
            .expect("the test's own declaration");
        assert_eq!(site.file.as_str(), SWIFT_FILE);
        assert_eq!(site.line, Some(2));
    }

    #[test]
    fn a_category_records_against_the_class_it_extends() {
        let index = indexed(
            "/repo/Tests/ObjcXCTestTests+Extra.m",
            "@interface ObjcXCTestTests (ExtraTests)\n- (void)testExample;\n@end",
            json!([container(
                "ObjcXCTestTests(ExtraTests)",
                11,
                (0, 2),
                vec![method("-testExample", 1)]
            )]),
        );
        assert!(
            index
                .lookup(&key(Some("ObjcXCTestTests"), "testExample"))
                .is_some()
        );
    }

    #[test]
    fn a_top_level_test_is_found_without_a_suite() {
        let index = indexed(
            "/repo/Tests/TopLevel.swift",
            "@Test func failingSnapshot() {}",
            json!([method("failingSnapshot()", 0)]),
        );
        assert!(index.lookup(&key(None, "failingSnapshot")).is_some());
    }

    // The run reports the subclass, but only the base class file declares the method.
    #[test]
    fn a_test_inherited_from_a_base_class_resolves_to_the_base_class_file() {
        let mut index = indexed(
            "/repo/Tests/BaseTests.swift",
            "class BaseTests: XCTestCase {\n    func testInherited() {}\n}",
            json!([container(
                "BaseTests",
                5,
                (0, 2),
                vec![method("testInherited()", 1)]
            )]),
        );
        index.collect(
            &symbols(json!([container("SubclassTests", 5, (0, 0), vec![])])),
            Path::new("/repo/Tests/SubclassTests.swift"),
            "final class SubclassTests: BaseTests {}",
            None,
        );
        assert_eq!(
            index
                .lookup(&key(Some("SubclassTests"), "testInherited"))
                .map(|site| site.file.as_str().to_owned()),
            Some(String::from("/repo/Tests/BaseTests.swift"))
        );
    }

    #[test]
    fn an_unrelated_suite_does_not_borrow_another_suites_case() {
        let index = indexed(
            SWIFT_FILE,
            "final class SnapshotReproTests: XCTestCase {\n    func testExample() {}\n}",
            json!([container(
                "SnapshotReproTests",
                5,
                (0, 2),
                vec![method("testExample()", 1)]
            )]),
        );
        assert!(
            index
                .lookup(&key(Some("OtherTests"), "testExample"))
                .is_none()
        );
    }

    // A cycle would otherwise be walked forever; `typealias`ed bases produce one.
    #[test]
    fn a_cyclic_superclass_chain_terminates() {
        let mut index = TestLocationIndex::default();
        index
            .supertypes
            .insert(String::from("A"), String::from("B"));
        index
            .supertypes
            .insert(String::from("B"), String::from("A"));
        assert!(index.lookup(&key(Some("A"), "testExample")).is_none());
    }

    #[rstest]
    #[case::swift("final class SubclassTests: BaseTests {", Some("BaseTests"))]
    #[case::objc("@interface SubclassTests : BaseTests", Some("BaseTests"))]
    #[case::no_inheritance_clause("struct PlainTests {", None)]
    fn a_declaration_head_yields_its_supertype(#[case] text: &str, #[case] expected: Option<&str>) {
        let range = Range {
            start: Position { line: 0 },
            end: Position { line: 0 },
        };
        assert_eq!(superclass(text, &range).as_deref(), expected);
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
        let scanned = scan_sources(root.as_ref(), &suites, 10)
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
        assert_eq!(scan_sources(root.as_ref(), &HashSet::new(), 2).len(), 2);
    }

    #[rstest]
    #[case::plain(
        "test://com.apple.xcode/InRepoHelper/InRepoHelperTests/Suite/case()",
        Some("InRepoHelperTests")
    )]
    #[case::percent_encoded(
        "test://com.apple.xcode/swift%20testing/swift%20testing%20exampleTests/helloWorld()",
        Some("swift testing exampleTests")
    )]
    #[case::too_short("test://com.apple.xcode/OnlyAScheme", None)]
    #[case::not_a_url("Suite/case()", None)]
    fn an_identifier_url_names_the_target(#[case] url: &str, #[case] expected: Option<&str>) {
        assert_eq!(
            TestKey::target_from_identifier_url(url).as_deref(),
            expected
        );
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

    #[rstest]
    #[case::top_level_function("MyCLITests", "helloworld()", None, "helloworld")]
    #[case::in_a_suite("MyCLITests.AlphaSuite", "shared()", Some("AlphaSuite"), "shared")]
    #[case::nested_suite("MyCLITests.AlphaSuite.Inner", "deep()", Some("Inner"), "deep")]
    #[case::other_suite_same_case("MyCLITests.BetaSuite", "shared()", Some("BetaSuite"), "shared")]
    #[case::parameterized(
        "MyCLITests.ParamSuite",
        "squares(n:)",
        Some("ParamSuite"),
        "squares(n:"
    )]
    #[case::objc_style(
        "MyCLITests.LegacyXCTests",
        "testOldStyle",
        Some("LegacyXCTests"),
        "testOldStyle"
    )]
    fn a_junit_classname_names_a_suite_and_a_case(
        #[case] classname: &str,
        #[case] name: &str,
        #[case] suite: Option<&str>,
        #[case] case: &str,
    ) {
        assert_eq!(
            TestKey::from_junit_classname(classname, name),
            key(suite, case)
        );
    }

    #[rstest]
    #[case::with_suite("MyCLITests.AlphaSuite", Some("MyCLITests"))]
    #[case::bare_target("MyCLITests", Some("MyCLITests"))]
    #[case::empty("", None)]
    fn a_junit_classname_names_the_target(#[case] classname: &str, #[case] expected: Option<&str>) {
        assert_eq!(
            TestKey::target_from_junit_classname(classname).as_deref(),
            expected
        );
    }

    // The two build `suite` from different places — an identifier's second-to-last component
    // versus a classname's innermost — so nothing else catches them drifting apart.
    #[rstest]
    #[case::top_level("helloworld()", "MyCLITests", "helloworld()")]
    #[case::in_a_suite("AlphaSuite/shared()", "MyCLITests.AlphaSuite", "shared()")]
    #[case::nested_suite(
        "OuterSuite/InnerSuite/deep()",
        "MyCLITests.OuterSuite.InnerSuite",
        "deep()"
    )]
    #[case::parameterized("ParamSuite/squares(n:)", "MyCLITests.ParamSuite", "squares(n:)")]
    #[case::no_argument_overload("OverloadSuite/check()", "MyCLITests.OverloadSuite", "check()")]
    #[case::labelled_overload("OverloadSuite/check(a:)", "MyCLITests.OverloadSuite", "check(a:)")]
    #[case::swift_xctest_method(
        "LegacyXCTests/testOldStyle()",
        "MyCLITests.LegacyXCTests",
        "testOldStyle"
    )]
    #[case::objc_xctest_method(
        "ObjcXCTestTests/testFailsInsideSharedHelper",
        "ObjcXCTestTests.ObjcXCTestTests",
        "testFailsInsideSharedHelper"
    )]
    fn an_xcresult_identifier_and_a_junit_classname_key_alike(
        #[case] node_identifier: &str,
        #[case] classname: &str,
        #[case] name: &str,
    ) {
        assert_eq!(
            TestKey::from_node_identifier(node_identifier),
            TestKey::from_junit_classname(classname, name)
        );
    }

    // Different fields, and the collision tie-break depends on them agreeing.
    #[rstest]
    #[case::in_a_suite(
        "test://com.apple.xcode/MyCLI/MyCLITests/AlphaSuite/shared()",
        "MyCLITests.AlphaSuite"
    )]
    #[case::top_level("test://com.apple.xcode/MyCLI/MyCLITests/helloworld()", "MyCLITests")]
    fn an_xcresult_url_and_a_junit_classname_name_the_same_target(
        #[case] identifier_url: &str,
        #[case] classname: &str,
    ) {
        assert_eq!(
            TestKey::target_from_identifier_url(identifier_url),
            TestKey::target_from_junit_classname(classname)
        );
    }
}
