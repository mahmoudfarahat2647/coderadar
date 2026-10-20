//! Unified diff presenter.
//!
//! The diff is what the user reviews before a patch is applied, so it must be
//! deterministic and readable (`CR-SPEC-13 §8`). This module is pure text
//! processing: it never touches the filesystem.

/// A rendered unified diff.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnifiedDiff {
    /// The unified diff text.
    pub text: String,
    /// Number of added lines.
    pub added_lines: usize,
    /// Number of removed lines.
    pub removed_lines: usize,
}

/// Maximum number of cells of the LCS table before the diff falls back to a
/// single replacement hunk. Keeps the memory usage bounded for large files.
const MAX_LCS_CELLS: usize = 4_000_000;

/// Number of context lines around a change.
const CONTEXT_LINES: usize = 3;

/// Build the unified diff between two file versions.
///
/// `label` is the workspace-relative path used in the `---`/`+++` headers.
pub fn unified_diff(original: &str, revised: &str, label: &str) -> UnifiedDiff {
    let old_lines: Vec<&str> = original.lines().collect();
    let new_lines: Vec<&str> = revised.lines().collect();
    let ops = diff_ops(&old_lines, &new_lines);

    let mut text = format!("--- a/{label}\n+++ b/{label}\n");
    let mut added_lines = 0usize;
    let mut removed_lines = 0usize;

    for (hunk_start, hunk_end) in hunks(&ops) {
        let (old_start, old_count, new_start, new_count) = hunk_header(&ops, hunk_start, hunk_end);
        text.push_str(&format!(
            "@@ -{} +{} @@\n",
            range_header(old_start, old_count),
            range_header(new_start, new_count)
        ));
        for op in &ops[hunk_start..hunk_end] {
            match *op {
                Op::Equal(old_index, _new_index) => {
                    text.push_str(&format!(" {}\n", old_lines[old_index]));
                }
                Op::Delete(old_index) => {
                    text.push_str(&format!("-{}\n", old_lines[old_index]));
                    removed_lines += 1;
                }
                Op::Insert(new_index) => {
                    text.push_str(&format!("+{}\n", new_lines[new_index]));
                    added_lines += 1;
                }
            }
        }
    }

    UnifiedDiff {
        text,
        added_lines,
        removed_lines,
    }
}

/// Format one side of a hunk header (`12,3` or `12`).
fn range_header(start: usize, count: usize) -> String {
    if count == 1 {
        format!("{start}")
    } else {
        format!("{start},{count}")
    }
}

/// One operation of the edit script.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Op {
    /// The line is unchanged.
    Equal(usize, usize),
    /// The old line was removed.
    Delete(usize),
    /// The new line was added.
    Insert(usize),
}

/// Compute the edit script between two line sequences.
fn diff_ops(old_lines: &[&str], new_lines: &[&str]) -> Vec<Op> {
    let mut prefix = 0usize;
    while prefix < old_lines.len()
        && prefix < new_lines.len()
        && old_lines[prefix] == new_lines[prefix]
    {
        prefix += 1;
    }

    let mut suffix = 0usize;
    while suffix < old_lines.len() - prefix
        && suffix < new_lines.len() - prefix
        && old_lines[old_lines.len() - 1 - suffix] == new_lines[new_lines.len() - 1 - suffix]
    {
        suffix += 1;
    }

    let old_mid = &old_lines[prefix..old_lines.len() - suffix];
    let new_mid = &new_lines[prefix..new_lines.len() - suffix];

    let mut ops = Vec::with_capacity(old_lines.len() + new_lines.len());
    for index in 0..prefix {
        ops.push(Op::Equal(index, index));
    }

    if old_mid.len().saturating_mul(new_mid.len()) > MAX_LCS_CELLS {
        // Too large for an exact diff: report the block as a replacement.
        for (offset, _) in old_mid.iter().enumerate() {
            ops.push(Op::Delete(prefix + offset));
        }
        for (offset, _) in new_mid.iter().enumerate() {
            ops.push(Op::Insert(prefix + offset));
        }
    } else {
        ops.extend(lcs_ops(old_mid, new_mid, prefix));
    }

    for offset in 0..suffix {
        let old_index = old_lines.len() - suffix + offset;
        let new_index = new_lines.len() - suffix + offset;
        ops.push(Op::Equal(old_index, new_index));
    }

    ops
}

/// Exact edit script of the two middle blocks, using a longest common
/// subsequence.
fn lcs_ops(old_mid: &[&str], new_mid: &[&str], offset: usize) -> Vec<Op> {
    let n = old_mid.len();
    let m = new_mid.len();
    let mut table = vec![0u32; (n + 1) * (m + 1)];

    for i in (0..n).rev() {
        for j in (0..m).rev() {
            table[i * (m + 1) + j] = if old_mid[i] == new_mid[j] {
                table[(i + 1) * (m + 1) + (j + 1)] + 1
            } else {
                table[(i + 1) * (m + 1) + j].max(table[i * (m + 1) + (j + 1)])
            };
        }
    }

    let mut ops = Vec::new();
    let (mut i, mut j) = (0usize, 0usize);
    while i < n && j < m {
        if old_mid[i] == new_mid[j] {
            ops.push(Op::Equal(offset + i, offset + j));
            i += 1;
            j += 1;
        } else if table[(i + 1) * (m + 1) + j] >= table[i * (m + 1) + (j + 1)] {
            ops.push(Op::Delete(offset + i));
            i += 1;
        } else {
            ops.push(Op::Insert(offset + j));
            j += 1;
        }
    }
    while i < n {
        ops.push(Op::Delete(offset + i));
        i += 1;
    }
    while j < m {
        ops.push(Op::Insert(offset + j));
        j += 1;
    }
    ops
}

/// Group the edit script into hunks with `CONTEXT_LINES` of context.
///
/// Every hunk is an inclusive index range of the edit script.
fn hunks(ops: &[Op]) -> Vec<(usize, usize)> {
    let changed: Vec<usize> = ops
        .iter()
        .enumerate()
        .filter(|(_, op)| !matches!(op, Op::Equal(_, _)))
        .map(|(index, _)| index)
        .collect();
    if changed.is_empty() {
        return Vec::new();
    }

    let mut groups: Vec<(usize, usize)> = Vec::new();
    let mut current_start = changed[0].saturating_sub(CONTEXT_LINES);
    let mut current_end = (changed[0] + CONTEXT_LINES + 1).min(ops.len());

    for index in changed.into_iter().skip(1) {
        let start = index.saturating_sub(CONTEXT_LINES);
        let end = (index + CONTEXT_LINES + 1).min(ops.len());
        if start <= current_end {
            current_end = current_end.max(end);
        } else {
            groups.push((current_start, current_end));
            current_start = start;
            current_end = end;
        }
    }
    groups.push((current_start, current_end));
    groups
}

/// Compute the `@@ -a,b +c,d @@` header of a hunk.
fn hunk_header(ops: &[Op], start: usize, end: usize) -> (usize, usize, usize, usize) {
    let mut old_start = usize::MAX;
    let mut new_start = usize::MAX;
    let mut old_count = 0usize;
    let mut new_count = 0usize;

    for op in &ops[start..end] {
        match *op {
            Op::Equal(old_index, new_index) => {
                old_start = old_start.min(old_index + 1);
                new_start = new_start.min(new_index + 1);
                old_count += 1;
                new_count += 1;
            }
            Op::Delete(old_index) => {
                old_start = old_start.min(old_index + 1);
                old_count += 1;
            }
            Op::Insert(new_index) => {
                new_start = new_start.min(new_index + 1);
                new_count += 1;
            }
        }
    }

    if old_start == usize::MAX {
        old_start = 1;
    }
    if new_start == usize::MAX {
        new_start = 1;
    }
    (old_start, old_count, new_start, new_count)
}

#[cfg(test)]
mod tests {
    use super::unified_diff;

    #[test]
    fn an_unchanged_file_has_no_hunk() {
        let diff = unified_diff("a\nb\n", "a\nb\n", "src/a.ts");
        assert_eq!(diff.added_lines, 0);
        assert_eq!(diff.removed_lines, 0);
        assert!(!diff.text.contains("@@"));
        assert!(diff.text.starts_with("--- a/src/a.ts\n+++ b/src/a.ts\n"));
    }

    #[test]
    fn a_single_changed_line_produces_one_hunk() {
        let diff = unified_diff("a\nb\nc\n", "a\nB\nc\n", "src/a.ts");
        assert_eq!(diff.added_lines, 1);
        assert_eq!(diff.removed_lines, 1);
        assert!(diff.text.contains("-b"));
        assert!(diff.text.contains("+B"));
        assert!(diff.text.contains("@@ -1,3 +1,3 @@"));
    }

    #[test]
    fn a_pure_insertion_is_reported() {
        let diff = unified_diff("a\nc\n", "a\nb\nc\n", "src/a.ts");
        assert_eq!(diff.added_lines, 1);
        assert_eq!(diff.removed_lines, 0);
        assert!(diff.text.contains("+b"));
    }

    #[test]
    fn a_pure_deletion_is_reported() {
        let diff = unified_diff("a\nb\nc\n", "a\nc\n", "src/a.ts");
        assert_eq!(diff.added_lines, 0);
        assert_eq!(diff.removed_lines, 1);
        assert!(diff.text.contains("-b"));
    }

    #[test]
    fn far_apart_changes_produce_two_hunks() {
        let old = "a\nb\nc\nd\ne\nf\ng\nh\ni\nj\n";
        let new = "A\nb\nc\nd\ne\nf\ng\nh\ni\nJ\n";
        let diff = unified_diff(old, new, "src/a.ts");
        assert_eq!(diff.text.matches("@@ -").count(), 2);
        assert_eq!(diff.added_lines, 2);
        assert_eq!(diff.removed_lines, 2);
    }
}
