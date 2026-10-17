//! Tree-sitter based code chunker for TypeScript / JavaScript.
//!
//! Phase 2 requirement (see issue #3): *"Code chunker: max 300 LOC / 12 KB per
//! chunk with tree-sitter for TS/JS."* The chunker slices a source file into
//! [`CodeChunk`] windows whose `content` is the **exact** source text of
//! `start_line..=end_line`, never exceeding [`CodeChunk::MAX_LINES`] LOC or
//! [`CodeChunk::MAX_BYTES`] bytes.
//!
//! Boundaries are chosen at top-level AST declarations (functions, classes,
//! statements) so that a semantic evaluator receives whole declarations
//! whenever they fit. Files that cannot be parsed cleanly fall back to a
//! deterministic line-window split.
//!
//! Whichever path is taken, the chunks partition the file: every line of the
//! source belongs to exactly one chunk and concatenating the chunk contents in
//! order reproduces the original source byte for byte. The only exception is a
//! single line that alone exceeds [`CodeChunk::MAX_BYTES`]; such a line is
//! split at character boundaries so that the bloat guard is never violated
//! (the scanner filters oversized lines out long before they reach the
//! chunker).

use coderadar_domain::CodeChunk;

/// Language token for TypeScript sources.
pub const TYPESCRIPT: &str = "typescript";
/// Language token for JavaScript sources.
pub const JAVASCRIPT: &str = "javascript";

/// Detect the language of a file from its name/extension.
///
/// Returns `None` for every extension that is not part of the v1.0 language
/// set (TypeScript/JavaScript, see `CR-SPEC-08 §6`).
pub fn detect_language(file_name: &str) -> Option<&'static str> {
    let extension = std::path::Path::new(file_name)
        .extension()
        .and_then(std::ffi::OsStr::to_str)?
        .to_ascii_lowercase();
    match extension.as_str() {
        "ts" | "tsx" | "mts" | "cts" => Some(TYPESCRIPT),
        "js" | "jsx" | "mjs" | "cjs" => Some(JAVASCRIPT),
        _ => None,
    }
}

/// A reusable chunker holding one tree-sitter parser.
///
/// Create one [`Chunker`] per worker thread (the parser is not `Sync`); the
/// language is re-selected per file, so a single instance can chunk both
/// TypeScript and JavaScript.
pub struct Chunker {
    parser: tree_sitter::Parser,
}

impl std::fmt::Debug for Chunker {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.debug_struct("Chunker").finish_non_exhaustive()
    }
}

impl Default for Chunker {
    fn default() -> Self {
        Self::new()
    }
}

impl Chunker {
    /// Create a chunker.
    pub fn new() -> Self {
        Self {
            parser: tree_sitter::Parser::new(),
        }
    }

    /// Slice `source` into chunks of at most 300 LOC / 12 KB.
    ///
    /// `file_path` should be slash-separated and relative to the scan root;
    /// its extension selects the grammar (`.tsx` uses the TSX dialect, the
    /// other TypeScript/JavaScript extensions their base grammars). Unknown
    /// languages and sources with parse errors fall back to a deterministic
    /// line-window split; the resulting chunks still honour both caps.
    ///
    /// Every chunk carries the exact source text of its
    /// `start_line..=end_line` range (including the trailing newline of the
    /// last line), the windows partition the file without gaps or overlaps,
    /// and an empty `source` yields no chunks at all.
    pub fn chunk(&mut self, file_path: &str, language: &str, source: &str) -> Vec<CodeChunk> {
        if source.is_empty() {
            return Vec::new();
        }
        let index = LineIndex::new(source);
        if let Some(tree) = self.parse(file_path, language, source) {
            if !tree.root_node().has_error() {
                return ast_window_chunks(file_path, language, &index, &tree);
            }
        }
        let mut chunks = Vec::new();
        push_line_windows(
            file_path,
            language,
            &index,
            1,
            index.total_lines(),
            &mut chunks,
        );
        chunks
    }

    /// Parse `source` with the grammar selected from `file_path`'s extension.
    ///
    /// Returns `None` for unknown language tokens, unknown extensions and
    /// parser failures; callers then take the deterministic fallback path.
    fn parse(
        &mut self,
        file_path: &str,
        language: &str,
        source: &str,
    ) -> Option<tree_sitter::Tree> {
        let grammar = grammar_for(file_path, language)?;
        self.parser.set_language(&grammar).ok()?;
        self.parser.parse(source, None)
    }
}

/// Select the tree-sitter grammar for a file.
///
/// The caller-supplied `language` token must be one the chunker knows; the
/// concrete grammar then comes from the file extension, which disambiguates
/// `.ts` (plain TypeScript) from `.tsx` (TypeScript + JSX).
fn grammar_for(file_path: &str, language: &str) -> Option<tree_sitter::Language> {
    if language != TYPESCRIPT && language != JAVASCRIPT {
        return None;
    }
    let extension = std::path::Path::new(file_path)
        .extension()?
        .to_str()?
        .to_ascii_lowercase();
    match extension.as_str() {
        "tsx" => Some(tree_sitter_typescript::LANGUAGE_TSX.into()),
        "ts" | "mts" | "cts" => Some(tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into()),
        "js" | "jsx" | "mjs" | "cjs" => Some(tree_sitter_javascript::LANGUAGE.into()),
        _ => None,
    }
}

/// Chunk a parse-error-free file at top-level declaration boundaries.
///
/// The declarations of the root node define a partition of `1..=total_lines`
/// (leading, interleaved and trailing comment/whitespace lines attach to the
/// neighbouring declaration). Consecutive declarations are then greedily
/// grouped into windows that fit both caps; a declaration that cannot fit on
/// its own is split at line boundaries.
fn ast_window_chunks(
    file_path: &str,
    language: &str,
    index: &LineIndex<'_>,
    tree: &tree_sitter::Tree,
) -> Vec<CodeChunk> {
    let total_lines = index.total_lines();
    let boundaries = declaration_end_lines(tree.root_node(), index);
    let mut chunks = Vec::new();
    if boundaries.is_empty() {
        push_line_windows(file_path, language, index, 1, total_lines, &mut chunks);
        return chunks;
    }

    let mut segments: Vec<(u32, u32)> = Vec::with_capacity(boundaries.len() + 1);
    let mut segment_start = 1;
    for boundary in boundaries {
        segments.push((segment_start, boundary));
        segment_start = boundary + 1;
    }
    if segment_start <= total_lines {
        segments.push((segment_start, total_lines));
    }

    let mut segment = 0;
    while segment < segments.len() {
        let (group_start, mut group_end) = segments[segment];
        while let Some(&(_, candidate_end)) = segments.get(segment + 1) {
            if !index.fits(group_start, candidate_end) {
                break;
            }
            segment += 1;
            group_end = candidate_end;
        }
        push_line_windows(
            file_path,
            language,
            index,
            group_start,
            group_end,
            &mut chunks,
        );
        segment += 1;
    }
    chunks
}

/// End line (1-based, inclusive) of every top-level declaration.
///
/// Only strictly increasing boundaries are kept, so several statements on one
/// source line produce a single boundary and `comment` children (which the
/// grammar exposes as top-level extras) are absorbed by the following
/// declaration.
fn declaration_end_lines(root: tree_sitter::Node<'_>, index: &LineIndex<'_>) -> Vec<u32> {
    let mut boundaries = Vec::new();
    let mut cursor = root.walk();
    for child in root.named_children(&mut cursor) {
        if child.kind() == "comment" {
            continue;
        }
        let end_byte = child.end_byte();
        if end_byte <= child.start_byte() {
            continue;
        }
        let end_line = (index.line_of_byte(end_byte - 1) + 1).min(index.total_lines());
        if boundaries
            .last()
            .is_none_or(|&previous| end_line > previous)
        {
            boundaries.push(end_line);
        }
    }
    boundaries
}

/// Append line-aligned windows covering `start_line..=end_line`.
///
/// Each window holds at most [`CodeChunk::MAX_LINES`] lines and at most
/// [`CodeChunk::MAX_BYTES`] bytes of exact source text. When a single line
/// alone exceeds the byte cap it is split at character boundaries, the only
/// situation in which a line can span more than one chunk.
fn push_line_windows(
    file_path: &str,
    language: &str,
    index: &LineIndex<'_>,
    start_line: u32,
    end_line: u32,
    chunks: &mut Vec<CodeChunk>,
) {
    let mut window_start = start_line;
    while window_start <= end_line {
        let mut window_end = window_start;
        let mut byte_len = index.byte_len(window_start, window_start);
        while window_end < end_line {
            let next_byte_len = index.byte_len(window_end + 1, window_end + 1);
            if window_end + 1 - window_start + 1 > CodeChunk::MAX_LINES
                || byte_len + next_byte_len > CodeChunk::MAX_BYTES
            {
                break;
            }
            window_end += 1;
            byte_len += next_byte_len;
        }
        if byte_len > CodeChunk::MAX_BYTES {
            push_oversized_line(file_path, language, index, window_start, chunks);
        } else {
            chunks.push(CodeChunk::new(
                file_path,
                language,
                window_start,
                window_end,
                index.content(window_start, window_end),
            ));
        }
        window_start = window_end + 1;
    }
}

/// Split a single line that alone exceeds [`CodeChunk::MAX_BYTES`] into
/// byte-cap-sized pieces at character boundaries.
fn push_oversized_line(
    file_path: &str,
    language: &str,
    index: &LineIndex<'_>,
    line: u32,
    chunks: &mut Vec<CodeChunk>,
) {
    let mut rest = index.content(line, line);
    while !rest.is_empty() {
        let mut split = rest.len().min(CodeChunk::MAX_BYTES);
        while !rest.is_char_boundary(split) {
            split -= 1;
        }
        let (piece, remainder) = rest.split_at(split);
        chunks.push(CodeChunk::new(file_path, language, line, line, piece));
        rest = remainder;
    }
}

/// Byte-offset index over the lines of one source string.
///
/// Line counting follows [`str::lines`]: a trailing newline does not open a
/// new line. The byte lengths include each line's terminating newline, so
/// summing the lengths of consecutive lines yields the exact byte size of the
/// corresponding source slice.
struct LineIndex<'a> {
    source: &'a str,
    starts: Vec<usize>,
    lengths: Vec<usize>,
}

impl<'a> LineIndex<'a> {
    /// Index `source`; `starts[i]` is the first byte of line `i + 1` (0-based
    /// index `i`) and `lengths[i]` its size including the terminating newline.
    fn new(source: &'a str) -> Self {
        let mut starts = Vec::new();
        let mut lengths = Vec::new();
        if !source.is_empty() {
            starts.push(0);
            let mut line_start = 0;
            for (offset, byte) in source.bytes().enumerate() {
                if byte == b'\n' {
                    lengths.push(offset + 1 - line_start);
                    line_start = offset + 1;
                    starts.push(line_start);
                }
            }
            if line_start < source.len() {
                lengths.push(source.len() - line_start);
            } else {
                starts.pop();
            }
        }
        Self {
            source,
            starts,
            lengths,
        }
    }

    /// Number of lines according to [`str::lines`] semantics.
    fn total_lines(&self) -> u32 {
        self.starts.len() as u32
    }

    /// Exact source text of `start_line..=end_line` (1-based, inclusive).
    fn content(&self, start_line: u32, end_line: u32) -> &'a str {
        let start = self.starts[start_line as usize - 1];
        let end = if end_line as usize == self.starts.len() {
            self.source.len()
        } else {
            self.starts[end_line as usize]
        };
        &self.source[start..end]
    }

    /// Exact byte size of `start_line..=end_line` (1-based, inclusive).
    fn byte_len(&self, start_line: u32, end_line: u32) -> usize {
        self.lengths[start_line as usize - 1..end_line as usize]
            .iter()
            .sum()
    }

    /// `true` when `start_line..=end_line` honours both chunk caps.
    fn fits(&self, start_line: u32, end_line: u32) -> bool {
        end_line - start_line < CodeChunk::MAX_LINES
            && self.byte_len(start_line, end_line) <= CodeChunk::MAX_BYTES
    }

    /// 0-based line that contains `byte`.
    fn line_of_byte(&self, byte: usize) -> u32 {
        (self.starts.partition_point(|&start| start <= byte) - 1) as u32
    }
}

#[cfg(test)]
mod tests {
    use super::{detect_language, Chunker, JAVASCRIPT, TYPESCRIPT};
    use coderadar_domain::{ChunkKind, CodeChunk};

    const MULTI_FUNCTION_TS: &str = r#"import { helper } from "./helper";

export interface Point {
  x: number;
  y: number;
}

// Adds two numbers together.
export function add(a: number, b: number): number {
  return a + b;
}

export class Accumulator {
  total = 0;

  add(value: number): void {
    this.total += value;
  }

  get value(): number {
    return this.total;
  }
}

export const ORIGIN: Point = { x: 0, y: 0 };
"#;

    /// Assert that `chunks` partition `1..=total_lines` without gaps or
    /// overlaps, reproduce `source` byte for byte and honour the bloat guard.
    fn assert_partitions_source(chunks: &[CodeChunk], source: &str) {
        let total_lines = source.lines().count() as u32;
        assert!(!chunks.is_empty(), "no chunks emitted");
        assert_eq!(chunks[0].start_line, 1, "chunks must start at line 1");
        assert_eq!(
            chunks[chunks.len() - 1].end_line,
            total_lines,
            "chunks must cover the last line"
        );
        for pair in chunks.windows(2) {
            assert_eq!(
                pair[0].end_line + 1,
                pair[1].start_line,
                "chunks must be contiguous"
            );
        }
        let round_trip: String = chunks.iter().map(|chunk| chunk.content.as_str()).collect();
        assert_eq!(round_trip, source, "chunks must reproduce the source");
        for chunk in chunks {
            assert!(
                !chunk.exceeds_bloat_guard(),
                "chunk {}..={} exceeds the bloat guard",
                chunk.start_line,
                chunk.end_line
            );
        }
    }

    /// Byte range of `start_line..=end_line` recomputed independently from the
    /// chunker's own [`super::LineIndex`].
    fn expected_slice(source: &str, start_line: u32, end_line: u32) -> &str {
        let mut starts = vec![0usize];
        for (offset, byte) in source.bytes().enumerate() {
            if byte == b'\n' {
                starts.push(offset + 1);
            }
        }
        let start = starts[start_line as usize - 1];
        let end = if end_line as usize == source.lines().count() {
            source.len()
        } else {
            starts[end_line as usize]
        };
        &source[start..end]
    }

    fn generate_functions(count: usize) -> String {
        let mut source = String::new();
        for index in 0..count {
            source.push_str(&format!("export function fn{index}(): number {{\n"));
            source.push_str("  const a = 1;\n");
            source.push_str("  const b = a + 1;\n");
            source.push_str("  const c = b + 1;\n");
            source.push_str("  const d = c + 1;\n");
            source.push_str("  const e = d + 1;\n");
            source.push_str("  const f = e + 1;\n");
            source.push_str("  const g = f + 1;\n");
            source.push_str("  return g;\n");
            source.push_str("}\n");
        }
        source
    }

    #[test]
    fn detects_typescript_extensions() {
        for name in ["a.ts", "a.tsx", "a.mts", "a.cts"] {
            assert_eq!(detect_language(name), Some(TYPESCRIPT), "{name}");
        }
    }

    #[test]
    fn detects_javascript_extensions() {
        for name in ["a.js", "a.jsx", "a.mjs", "a.cjs"] {
            assert_eq!(detect_language(name), Some(JAVASCRIPT), "{name}");
        }
    }

    #[test]
    fn rejects_unsupported_extensions() {
        for name in ["a.py", "a.rs", "a.json", "Dockerfile", "a.ts.bak"] {
            assert_eq!(detect_language(name), None, "{name}");
        }
    }

    #[test]
    fn selects_grammar_for_supported_extensions_only() {
        assert!(super::grammar_for("src/app.ts", TYPESCRIPT).is_some());
        assert!(super::grammar_for("src/app.tsx", TYPESCRIPT).is_some());
        assert!(super::grammar_for("src/app.mts", TYPESCRIPT).is_some());
        assert!(super::grammar_for("src/app.cts", TYPESCRIPT).is_some());
        assert!(super::grammar_for("src/app.js", JAVASCRIPT).is_some());
        assert!(super::grammar_for("src/app.jsx", JAVASCRIPT).is_some());
        assert!(super::grammar_for("src/app.mjs", JAVASCRIPT).is_some());
        assert!(super::grammar_for("src/app.cjs", JAVASCRIPT).is_some());
        assert!(super::grammar_for("src/app.py", TYPESCRIPT).is_none());
        assert!(super::grammar_for("src/app.ts", "python").is_none());
    }

    #[test]
    fn chunks_small_typescript_file_with_exact_round_trip() {
        let mut chunker = Chunker::new();
        let chunks = chunker.chunk("src/math.ts", TYPESCRIPT, MULTI_FUNCTION_TS);

        assert_partitions_source(&chunks, MULTI_FUNCTION_TS);
        for chunk in &chunks {
            assert_eq!(chunk.file_path, "src/math.ts");
            assert_eq!(chunk.language, TYPESCRIPT);
            assert_eq!(chunk.kind, ChunkKind::AstWindow);
            assert_eq!(
                chunk.content,
                expected_slice(MULTI_FUNCTION_TS, chunk.start_line, chunk.end_line)
            );
        }
    }

    #[test]
    fn chunks_tsx_source_cleanly() {
        let source = "export const App = () => <div className=\"app\">hello</div>;\n";
        let mut chunker = Chunker::new();
        let chunks = chunker.chunk("src/App.tsx", TYPESCRIPT, source);

        assert_partitions_source(&chunks, source);
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0].content, source);
    }

    #[test]
    fn splits_many_small_functions_at_declaration_boundaries() {
        let source = generate_functions(40);
        let mut chunker = Chunker::new();
        let chunks = chunker.chunk("src/many.ts", TYPESCRIPT, &source);

        assert_partitions_source(&chunks, &source);
        assert_eq!(chunks.len(), 2);
        assert_eq!(chunks[0].end_line, 300);
        assert_eq!(chunks[0].line_count(), CodeChunk::MAX_LINES);
        assert_eq!(chunks[1].start_line, 301);
        assert_eq!(chunks[1].end_line, 400);

        let lines: Vec<&str> = source.lines().collect();
        for chunk in &chunks {
            assert_eq!(
                lines[chunk.end_line as usize - 1].trim(),
                "}",
                "chunk must end at a top-level declaration"
            );
        }
    }

    #[test]
    fn splits_an_oversized_single_function_respecting_both_caps() {
        let mut source = String::from("export function big(): number {\n");
        source.push_str("  let total = 0;\n");
        for index in 0..600 {
            source.push_str(&format!("  total += {index};\n"));
        }
        source.push_str("  return total;\n");
        source.push_str("}\n");

        let mut chunker = Chunker::new();
        let chunks = chunker.chunk("src/big.ts", TYPESCRIPT, &source);

        assert_partitions_source(&chunks, &source);
        assert_eq!(chunks.len(), 3);
        assert_eq!(chunks[0].start_line, 1);
        assert_eq!(chunks[0].end_line, 300);
        assert_eq!(chunks[1].start_line, 301);
        assert_eq!(chunks[1].end_line, 600);
        assert_eq!(chunks[2].start_line, 601);
        assert_eq!(chunks[2].end_line, 604);
        for chunk in &chunks {
            assert!(chunk.line_count() <= CodeChunk::MAX_LINES);
            assert!(chunk.byte_len() <= CodeChunk::MAX_BYTES);
        }
    }

    #[test]
    fn falls_back_to_line_windows_on_parse_errors() {
        let mut chunker = Chunker::new();

        let broken = "}{ )(";
        let chunks = chunker.chunk("broken.ts", TYPESCRIPT, broken);
        assert_partitions_source(&chunks, broken);
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0].content, broken);

        let mut long_broken = String::new();
        for _ in 0..350 {
            long_broken.push_str("}{ )(\n");
        }
        let chunks = chunker.chunk("broken.ts", TYPESCRIPT, &long_broken);
        assert_partitions_source(&chunks, &long_broken);
        assert_eq!(chunks.len(), 2);
        assert_eq!(chunks[0].end_line, 300);
        assert_eq!(chunks[1].start_line, 301);
        assert_eq!(chunks[1].end_line, 350);
    }

    #[test]
    fn fallback_respects_the_byte_cap() {
        let mut source = String::new();
        for _ in 0..200 {
            source.push_str(&"}{ )(".repeat(19));
            source.push('\n');
        }

        let mut chunker = Chunker::new();
        let chunks = chunker.chunk("broken.ts", TYPESCRIPT, &source);

        assert_partitions_source(&chunks, &source);
        assert_eq!(chunks.len(), 2);
        assert_eq!(chunks[0].end_line, 128);
        assert_eq!(chunks[0].byte_len(), CodeChunk::MAX_BYTES);
        assert_eq!(chunks[1].start_line, 129);
        assert_eq!(chunks[1].end_line, 200);
    }

    #[test]
    fn falls_back_for_unknown_languages() {
        let source = "def add(a, b):\n    return a + b\n";
        let mut chunker = Chunker::new();
        let chunks = chunker.chunk("script.py", "python", source);

        assert_partitions_source(&chunks, source);
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0].content, source);
    }

    #[test]
    fn empty_source_yields_no_chunks() {
        let mut chunker = Chunker::new();
        assert!(chunker.chunk("empty.ts", TYPESCRIPT, "").is_empty());
        assert!(chunker.chunk("empty.js", JAVASCRIPT, "").is_empty());
    }

    #[test]
    fn tree_sitter_grammars_parse_ts_and_js() {
        let mut parser = tree_sitter::Parser::new();
        let ts: tree_sitter::Language = tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into();
        let tsx: tree_sitter::Language = tree_sitter_typescript::LANGUAGE_TSX.into();
        let js: tree_sitter::Language = tree_sitter_javascript::LANGUAGE.into();

        for (language, source) in [
            (
                ts,
                "export function add(a: number, b: number): number { return a + b; }",
            ),
            (tsx, "export const App = () => <div>hello</div>;"),
            (js, "export function add(a, b) { return a + b; }"),
        ] {
            parser.set_language(&language).unwrap();
            let tree = parser.parse(source, None).unwrap();
            assert!(!tree.root_node().has_error(), "grammar parsed with errors");
        }
    }
}
