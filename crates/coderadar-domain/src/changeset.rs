//! Git changeset model and the Pull Request hygiene rules (`PR-*`).
//!
//! A [`Changeset`] is the pure description of a staged diff: which files
//! changed, how many lines each side added or removed, and which removed lines
//! looked like public API. The Git engine lives in an outer layer; this module
//! only decides **what the numbers mean** (`CR-SPEC-04 §6`).
//!
//! The three Phase 8 rules are evaluated here, deterministically:
//!
//! | Rule | Trigger |
//! | --- | --- |
//! | `PR-SCP-001` | More than [`SCOPE_CREEP_LINE_THRESHOLD`] changed lines across at least [`SCOPE_CREEP_AREA_THRESHOLD`] unrelated top-level areas. |
//! | `PR-BRK-002` | A removed line looked like an exported or public signature, with no deprecation path in the diff. |
//! | `PR-TST-003` | Production code files changed while no test file changed at all. |
//!
//! The model never echoes removed source text in a violation: only the file,
//! a count, and a stable rule id cross the boundary.

/// More changed lines than this across unrelated areas is scope creep.
pub const SCOPE_CREEP_LINE_THRESHOLD: u32 = 500;

/// At least this many unrelated top-level areas make a large diff unreadable.
pub const SCOPE_CREEP_AREA_THRESHOLD: usize = 2;

/// `true` when `path` looks like a test file.
///
/// The check is deliberately language-agnostic: it covers the conventions used
/// by TypeScript/JavaScript, Rust, Python, Go, Java and the common `tests/`
/// directory layouts.
pub fn is_test_path(path: &str) -> bool {
    let normalised = path.replace('\\', "/").to_ascii_lowercase();
    if normalised.starts_with("tests/")
        || normalised.starts_with("test/")
        || normalised.contains("/tests/")
        || normalised.contains("/test/")
        || normalised.contains("/__tests__/")
        || normalised.contains("/spec/")
    {
        return true;
    }

    let file_name = normalised.rsplit('/').next().unwrap_or(&normalised);
    file_name.contains("_test.")
        || file_name.contains(".test.")
        || file_name.contains(".spec.")
        || file_name.contains("_spec.")
        || file_name.starts_with("test_")
        || file_name.ends_with("_test.rs")
        || matches!(file_name, "tests.rs" | "test.rs")
}

/// `true` when `path` has a source-code extension.
///
/// Used by `PR-TST-003`: a changed README or lockfile is not "production code
/// added without tests".
pub fn is_code_path(path: &str) -> bool {
    let lower = path.to_ascii_lowercase();
    let extension = lower.rsplit('.').next().unwrap_or_default();
    matches!(
        extension,
        "ts" | "tsx"
            | "js"
            | "jsx"
            | "mjs"
            | "cjs"
            | "rs"
            | "py"
            | "go"
            | "java"
            | "kt"
            | "kts"
            | "cs"
            | "rb"
            | "php"
            | "swift"
            | "c"
            | "cc"
            | "cpp"
            | "h"
            | "hpp"
            | "vue"
            | "svelte"
    )
}

/// `true` when a removed line looked like a public API declaration.
///
/// The heuristic is intentionally conservative: it only looks at the start of
/// the line and covers the exported / public keywords of the supported
/// ecosystems. A removed private helper never trips `PR-BRK-002`.
pub fn is_public_api_removal(line: &str) -> bool {
    let trimmed = line.trim_start();
    trimmed.starts_with("export ")
        || trimmed.starts_with("pub ")
        || trimmed.starts_with("public ")
        || trimmed.starts_with("protected ")
}

/// One file of a changeset.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileChange {
    /// Workspace-relative, slash separated path.
    pub path: String,
    /// Lines added by the diff.
    pub additions: u32,
    /// Lines removed by the diff.
    pub deletions: u32,
    /// Removed lines that looked like exported / public signatures.
    pub removed_public_api: Vec<String>,
    /// `true` when the path follows a test-file convention.
    pub is_test: bool,
}

impl FileChange {
    /// Create a file change; the test flag is derived from the path.
    pub fn new(path: impl Into<String>, additions: u32, deletions: u32) -> Self {
        let path = path.into();
        let is_test = is_test_path(&path);
        Self {
            path,
            additions,
            deletions,
            removed_public_api: Vec::new(),
            is_test,
        }
    }

    /// Attach the removed public signatures observed in the diff.
    #[must_use]
    pub fn with_removed_public_api(mut self, removed: Vec<String>) -> Self {
        self.removed_public_api = removed;
        self
    }

    /// Total changed lines (added plus removed).
    pub const fn lines_changed(&self) -> u32 {
        self.additions + self.deletions
    }

    /// `true` when the file is not a test file.
    pub const fn is_production(&self) -> bool {
        !self.is_test
    }
}

/// A staged Git changeset, as read by the Git engine.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Changeset {
    /// The revision the staged index is compared against (`HEAD`, or `empty`
    /// when the repository has no commit yet).
    pub base: String,
    /// Changed files, in diff order.
    pub files: Vec<FileChange>,
}

impl Changeset {
    /// Create a changeset.
    pub fn new(base: impl Into<String>, files: Vec<FileChange>) -> Self {
        Self {
            base: base.into(),
            files,
        }
    }

    /// `true` when nothing is staged.
    pub fn is_empty(&self) -> bool {
        self.files.is_empty()
    }

    /// Number of changed files.
    pub fn files_changed(&self) -> usize {
        self.files.len()
    }

    /// Total added lines.
    pub fn additions(&self) -> u32 {
        self.files.iter().map(|file| file.additions).sum()
    }

    /// Total removed lines.
    pub fn deletions(&self) -> u32 {
        self.files.iter().map(|file| file.deletions).sum()
    }

    /// Total changed lines (added plus removed).
    pub fn lines_changed(&self) -> u32 {
        self.files.iter().map(FileChange::lines_changed).sum()
    }

    /// Number of changed production source files.
    pub fn production_code_files_changed(&self) -> usize {
        self.files
            .iter()
            .filter(|file| file.is_production() && is_code_path(&file.path))
            .count()
    }

    /// Number of changed test files.
    pub fn test_files_changed(&self) -> usize {
        self.files.iter().filter(|file| file.is_test).count()
    }

    /// Unrelated top-level areas the changeset touches, sorted and deduplicated.
    ///
    /// The first path segment is the area (`src`, `docs`, `crates`); a file at
    /// the repository root belongs to the `(root)` area.
    pub fn areas(&self) -> Vec<String> {
        let mut areas: Vec<String> = self
            .files
            .iter()
            .map(|file| first_segment(&file.path))
            .collect();
        areas.sort_unstable();
        areas.dedup();
        areas
    }

    /// The production code file that best anchors a file-level violation.
    ///
    /// Deterministic: the lexicographically first changed production code file,
    /// or the first changed file when the changeset has no production code.
    fn anchor_file(&self) -> Option<&str> {
        let mut code_files: Vec<&str> = self
            .files
            .iter()
            .filter(|file| file.is_production() && is_code_path(&file.path))
            .map(|file| file.path.as_str())
            .collect();
        code_files.sort_unstable();
        if let Some(path) = code_files.first() {
            return Some(path);
        }
        self.files.first().map(|file| file.path.as_str())
    }

    /// Evaluate the Pull Request hygiene rules against this changeset.
    pub fn assess(&self) -> PullRequestHygiene {
        let areas_touched = self.areas();
        let lines_changed = self.lines_changed();
        let production_files_changed = self.production_code_files_changed();
        let test_files_changed = self.test_files_changed();

        let scope_creep = lines_changed > SCOPE_CREEP_LINE_THRESHOLD
            && areas_touched.len() >= SCOPE_CREEP_AREA_THRESHOLD;
        let breaking_change = self
            .files
            .iter()
            .any(|file| !file.removed_public_api.is_empty());
        let missing_tests =
            production_files_changed > 0 && test_files_changed == 0 && self.additions() > 0;

        let mut violations = Vec::new();

        if scope_creep {
            let file = self.anchor_file().unwrap_or("(changeset)").to_string();
            violations.push(PrViolation {
                rule_id: "PR-SCP-001",
                file_path: file,
                line: 1,
                description: format!(
                    "The changeset modifies {lines_changed} lines across {} unrelated areas ({}); split it into reviewable pull requests.",
                    areas_touched.len(),
                    areas_touched.join(", ")
                ),
                confidence: 0.9,
            });
        }

        if breaking_change {
            for file in &self.files {
                if file.removed_public_api.is_empty() {
                    continue;
                }
                violations.push(PrViolation {
                    rule_id: "PR-BRK-002",
                    file_path: file.path.clone(),
                    line: 1,
                    description: format!(
                        "{} exported or public signature(s) were removed from `{}` without a deprecation path.",
                        file.removed_public_api.len(),
                        file.path
                    ),
                    confidence: 0.9,
                });
            }
        }

        if missing_tests {
            let file = self.anchor_file().unwrap_or("(changeset)").to_string();
            violations.push(PrViolation {
                rule_id: "PR-TST-003",
                file_path: file,
                line: 1,
                description: format!(
                    "{production_files_changed} production code file(s) changed, but no test file changed."
                ),
                confidence: 0.95,
            });
        }

        violations.sort_by(|left, right| {
            (left.rule_id, &left.file_path).cmp(&(right.rule_id, &right.file_path))
        });

        PullRequestHygiene {
            base: self.base.clone(),
            files_changed: self.files_changed(),
            additions: self.additions(),
            deletions: self.deletions(),
            lines_changed,
            production_files_changed,
            test_files_changed,
            areas_touched,
            scope_creep,
            breaking_change,
            missing_tests,
            violations,
        }
    }
}

/// The first path segment of a workspace-relative path.
fn first_segment(path: &str) -> String {
    let normalised = path.replace('\\', "/");
    normalised
        .split('/')
        .next()
        .filter(|segment| !segment.is_empty())
        .unwrap_or("(root)")
        .to_string()
}

/// A rule violation found by the Pull Request hygiene assessment.
///
/// It carries no severity or title: those are authoritative in
/// `rules/manifest.json` and are attached by the use case.
#[derive(Debug, Clone, PartialEq)]
pub struct PrViolation {
    /// Stable rule identifier (`PR-SCP-001`, ...).
    pub rule_id: &'static str,
    /// File the violation is anchored to.
    pub file_path: String,
    /// Line the violation is anchored to (file-level rules use `1`).
    pub line: u32,
    /// Human readable explanation that never echoes source text.
    pub description: String,
    /// Assessment confidence in `[0.0, 1.0]`.
    pub confidence: f32,
}

/// The Pull Request hygiene scorecard and the violations behind it.
#[derive(Debug, Clone, PartialEq)]
pub struct PullRequestHygiene {
    /// The revision the staged index was compared against.
    pub base: String,
    /// Number of changed files.
    pub files_changed: usize,
    /// Number of added lines.
    pub additions: u32,
    /// Number of removed lines.
    pub deletions: u32,
    /// Total changed lines.
    pub lines_changed: u32,
    /// Changed production source files.
    pub production_files_changed: usize,
    /// Changed test files.
    pub test_files_changed: usize,
    /// Unrelated top-level areas touched, sorted.
    pub areas_touched: Vec<String>,
    /// `PR-SCP-001` trigger.
    pub scope_creep: bool,
    /// `PR-BRK-002` trigger.
    pub breaking_change: bool,
    /// `PR-TST-003` trigger.
    pub missing_tests: bool,
    /// The violations, sorted by rule id then file.
    pub violations: Vec<PrViolation>,
}

impl PullRequestHygiene {
    /// `true` when no `PR-*` rule fired.
    pub fn is_clean(&self) -> bool {
        self.violations.is_empty()
    }
}
