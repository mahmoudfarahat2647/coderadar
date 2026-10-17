use crate::finding::Finding;
use crate::rule::EvaluationEngine;

/// What kind of context a [`CodeChunk`] carries.
///
/// Chunk-scope rules (`CR-SPEC-04 §8`) are evaluated against AST windows, while
/// file/module-scope rules (for example `ARCH-DEP-001`) need the whole file —
/// the scanner emits one [`ChunkKind::FileModule`] chunk per scanned file in
/// addition to its AST windows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum ChunkKind {
    /// A function/class-sized AST window (up to 300 LOC / 12 KB).
    #[default]
    AstWindow,
    /// The complete source of one file, for file/module-scope rules.
    FileModule,
}

/// A window of source code submitted for rule evaluation.
///
/// The chunker (Phase 2) slices files into these windows; use cases and the
/// evaluator ports exchange them without ever touching a filesystem handle.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodeChunk {
    /// Path of the source file the chunk was taken from (relative to scan root).
    pub file_path: String,
    /// Detected language, e.g. `typescript`.
    pub language: String,
    /// Whether this is an AST window or the whole file context.
    pub kind: ChunkKind,
    /// First line of the chunk (1-based, inclusive).
    pub start_line: u32,
    /// Last line of the chunk (1-based, inclusive).
    pub end_line: u32,
    /// Raw source text of the chunk.
    pub content: String,
}

impl CodeChunk {
    /// Maximum number of lines per chunk (see `PHASES.md`, Phase 2).
    pub const MAX_LINES: u32 = 300;
    /// Maximum byte size per chunk (12 KB).
    pub const MAX_BYTES: usize = 12 * 1024;

    /// Build an [`ChunkKind::AstWindow`] chunk from its raw parts.
    pub fn new(
        file_path: impl Into<String>,
        language: impl Into<String>,
        start_line: u32,
        end_line: u32,
        content: impl Into<String>,
    ) -> Self {
        Self {
            file_path: file_path.into(),
            language: language.into(),
            kind: ChunkKind::AstWindow,
            start_line,
            end_line,
            content: content.into(),
        }
    }

    /// Build a whole-file [`ChunkKind::FileModule`] chunk.
    ///
    /// `end_line` is the number of source lines the file contains.
    pub fn file_module(
        file_path: impl Into<String>,
        language: impl Into<String>,
        end_line: u32,
        content: impl Into<String>,
    ) -> Self {
        Self {
            file_path: file_path.into(),
            language: language.into(),
            kind: ChunkKind::FileModule,
            start_line: 1,
            end_line,
            content: content.into(),
        }
    }

    /// Number of lines covered by the chunk (inclusive).
    pub const fn line_count(&self) -> u32 {
        self.end_line.saturating_sub(self.start_line) + 1
    }

    /// Length of the chunk payload in bytes.
    pub fn byte_len(&self) -> usize {
        self.content.len()
    }

    /// `true` when the chunk exceeds the bloat guard and should be split.
    pub fn exceeds_bloat_guard(&self) -> bool {
        self.line_count() > Self::MAX_LINES || self.byte_len() > Self::MAX_BYTES
    }
}

/// The outcome of evaluating a set of rules against one or more chunks.
#[derive(Debug, Clone, PartialEq)]
pub struct RuleEvaluationResult {
    /// Identifier of the rule that produced these findings.
    pub rule_id: String,
    /// Engine that produced the result.
    pub engine: EvaluationEngine,
    /// Findings emitted for this rule (empty when the code is clean).
    pub findings: Vec<Finding>,
}

impl RuleEvaluationResult {
    /// Build an evaluation result.
    pub fn new(
        rule_id: impl Into<String>,
        engine: EvaluationEngine,
        findings: Vec<Finding>,
    ) -> Self {
        Self {
            rule_id: rule_id.into(),
            engine,
            findings,
        }
    }

    /// `true` when the rule found nothing to report.
    pub fn is_clean(&self) -> bool {
        self.findings.is_empty()
    }
}
