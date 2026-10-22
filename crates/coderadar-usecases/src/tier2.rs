//! Tier 2 generative remediation (`FR-05`, Phase 8).
//!
//! The use case never trusts a model's output: it asks the provider for a
//! draft, then wraps the **complete proposed file** in a [`PatchProposal`] that
//! carries the SHA-256 of the file it was generated from. The user reviews the
//! unified diff and applies it through the same atomic, conflict-checked
//! applier as a deterministic fix (`CR-SPEC-13` section 4).
//!
//! Safety invariants enforced here, independent of the model:
//!
//! * the file must fit the Tier 2 budget ([`MAX_INPUT_BYTES`]);
//! * the proposal must differ from the current file;
//! * the proposal must be non-empty and within [`MAX_OUTPUT_BYTES`];
//! * the payload is proven to be the exact result of its single whole-file
//!   edit before it is returned.

use std::sync::Arc;

use coderadar_domain::{DomainError, LineEdit, PatchProposal};

use crate::ports::{BoxFuture, GenerativeFixProvider, SourceFileReader};

/// Largest file a Tier 2 provider may be asked to rewrite (256 KiB).
pub const MAX_INPUT_BYTES: usize = 256 * 1024;

/// Largest accepted model answer (512 KiB).
pub const MAX_OUTPUT_BYTES: usize = 512 * 1024;

/// The finding metadata a Tier 2 prompt is built from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FixFinding {
    /// Workspace-relative path of the offending file.
    pub file_path: String,
    /// Rule that produced the finding.
    pub rule_id: String,
    /// Short rule title.
    pub title: String,
    /// Explanation shown to the model and the user.
    pub description: String,
    /// First offending line.
    pub line_start: u32,
    /// Last offending line.
    pub line_end: u32,
}

/// Everything a provider needs to build its prompt.
///
/// The content is the raw file: a provider that egresses to the cloud is
/// responsible for its own privacy policy (redaction or refusal).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FixGenerationRequest {
    /// Workspace-relative path of the offending file.
    pub file_path: String,
    /// Detected language token (`typescript`, `rust`, ...).
    pub language: String,
    /// Rule that produced the finding.
    pub rule_id: String,
    /// Short rule title.
    pub title: String,
    /// Explanation of the finding.
    pub description: String,
    /// First offending line.
    pub line_start: u32,
    /// Last offending line.
    pub line_end: u32,
    /// Complete current file content.
    pub file_content: String,
}

/// A provider's parsed answer: a complete replacement file plus its rationale.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GeneratedFixDraft {
    /// Why the change fixes the finding.
    pub explanation: String,
    /// The complete corrected file.
    pub new_content: String,
    /// Provider-specific warnings shown in the diff modal.
    pub warnings: Vec<String>,
}

/// A reviewable Tier 2 remediation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GeneratedFix {
    /// The proposal, carrying the fingerprint of the original file.
    pub proposal: PatchProposal,
    /// Non-fatal warnings shown alongside the diff.
    pub warnings: Vec<String>,
}

/// Use case: turn a finding into a reviewable Tier 2 [`PatchProposal`].
pub struct GenerateFix {
    provider: Arc<dyn GenerativeFixProvider>,
    reader: Arc<dyn SourceFileReader>,
}

impl GenerateFix {
    /// Create the use case on top of a provider and a workspace reader.
    pub fn new(
        provider: Arc<dyn GenerativeFixProvider>,
        reader: Arc<dyn SourceFileReader>,
    ) -> Self {
        Self { provider, reader }
    }

    /// The provider token (`ollama`, `gemini`).
    pub fn provider_name(&self) -> &'static str {
        self.provider.provider_name()
    }

    /// `true` when the provider runs locally.
    pub fn is_local(&self) -> bool {
        self.provider.is_local()
    }

    /// Read the file, ask the provider, and validate its answer.
    ///
    /// # Errors
    /// Returns [`DomainError`] when the file cannot be read, the provider
    /// fails, or its answer cannot be turned into a safe proposal.
    pub fn generate<'a>(
        &'a self,
        root: &'a str,
        finding: &'a FixFinding,
    ) -> BoxFuture<'a, Result<GeneratedFix, DomainError>> {
        Box::pin(async move {
            let source = self.reader.read_source(root, &finding.file_path).await?;
            if source.content.len() > MAX_INPUT_BYTES {
                return Err(DomainError::Invariant(format!(
                    "`{}` is larger than the Tier 2 budget of {} bytes",
                    finding.file_path, MAX_INPUT_BYTES
                )));
            }

            let request = FixGenerationRequest {
                file_path: finding.file_path.clone(),
                language: language_of(&finding.file_path).to_string(),
                rule_id: finding.rule_id.clone(),
                title: finding.title.clone(),
                description: finding.description.clone(),
                line_start: finding.line_start,
                line_end: finding.line_end,
                file_content: source.content.clone(),
            };

            let draft = self.provider.propose_fix(&request).await?;
            let new_content = draft.new_content;

            if new_content.trim().is_empty() {
                return Err(DomainError::Invariant(
                    "the provider returned an empty file".to_string(),
                ));
            }
            if new_content == source.content {
                return Err(DomainError::Invariant(
                    "the provider returned the file unchanged".to_string(),
                ));
            }
            if new_content.len() > MAX_OUTPUT_BYTES {
                return Err(DomainError::Invariant(format!(
                    "the provider answer is larger than the {} byte budget",
                    MAX_OUTPUT_BYTES
                )));
            }

            let end_line = u32::try_from(source.line_count).unwrap_or(u32::MAX).max(1);
            let proposal = PatchProposal::new(
                finding.file_path.clone(),
                finding.rule_id.clone(),
                source.sha256.clone(),
                source.line_count,
                vec![LineEdit::new(
                    1,
                    end_line,
                    0,
                    source.content.len(),
                    new_content.clone(),
                )],
                new_content,
                draft.explanation,
            );

            // Prove that the advertised edit really produces the payload before
            // the user is ever offered an apply button.
            let applied = proposal.apply_edits(&source.content)?;
            if applied != proposal.new_content {
                return Err(DomainError::invariant(
                    "the Tier 2 payload is not the result of its own edit",
                ));
            }

            let mut warnings = draft.warnings;
            warnings.push(format!(
                "Generated by the Tier 2 `{}` provider. Review every line before applying.",
                self.provider.provider_name()
            ));
            if source.content.ends_with('\n') && !proposal.new_content.ends_with('\n') {
                warnings.push("The generated file does not end with a newline.".to_string());
            }

            Ok(GeneratedFix { proposal, warnings })
        })
    }
}

/// Map a file extension to the language token used in the prompt.
fn language_of(path: &str) -> &'static str {
    let extension = path.rsplit('.').next().unwrap_or_default();
    match extension.to_ascii_lowercase().as_str() {
        "ts" | "tsx" => "typescript",
        "js" | "jsx" | "mjs" | "cjs" => "javascript",
        "rs" => "rust",
        "py" => "python",
        "go" => "go",
        "java" => "java",
        "kt" | "kts" => "kotlin",
        "cs" => "csharp",
        "rb" => "ruby",
        "php" => "php",
        "swift" => "swift",
        "c" => "c",
        "cc" | "cpp" | "hpp" => "cpp",
        "h" => "c",
        "vue" => "vue",
        "svelte" => "svelte",
        _ => "text",
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use coderadar_domain::DomainError;

    use super::{FixFinding, GenerateFix, GeneratedFixDraft, MAX_OUTPUT_BYTES};
    use crate::ports::{BoxFuture, GenerativeFixProvider, SourceFile, SourceFileReader};
    use crate::tier2::FixGenerationRequest;

    struct FakeReader {
        content: String,
    }

    impl SourceFileReader for FakeReader {
        fn read_source<'a>(
            &'a self,
            _root: &'a str,
            _relative_path: &'a str,
        ) -> BoxFuture<'a, Result<SourceFile, DomainError>> {
            let content = self.content.clone();
            Box::pin(async move {
                Ok(SourceFile {
                    sha256: format!("sha-{}", content.len()),
                    line_count: content.lines().count(),
                    content,
                })
            })
        }
    }

    struct FakeProvider {
        draft: GeneratedFixDraft,
        local: bool,
    }

    impl GenerativeFixProvider for FakeProvider {
        fn provider_name(&self) -> &'static str {
            "fake"
        }

        fn is_local(&self) -> bool {
            self.local
        }

        fn propose_fix<'a>(
            &'a self,
            _request: &'a FixGenerationRequest,
        ) -> BoxFuture<'a, Result<GeneratedFixDraft, DomainError>> {
            let draft = self.draft.clone();
            Box::pin(async move { Ok(draft) })
        }
    }

    fn finding() -> FixFinding {
        FixFinding {
            file_path: "src/auth.ts".to_string(),
            rule_id: "SEC-JWT-003".to_string(),
            title: "Unvalidated JWT Header".to_string(),
            description: "jwt.decode without verification".to_string(),
            line_start: 2,
            line_end: 2,
        }
    }

    fn engine(draft: GeneratedFixDraft) -> GenerateFix {
        GenerateFix::new(
            Arc::new(FakeProvider { draft, local: true }),
            Arc::new(FakeReader {
                content: "const a = 1;\nconst token = jwt.decode(x);\n".to_string(),
            }),
        )
    }

    #[test]
    fn a_draft_becomes_a_fingerprinted_whole_file_proposal() {
        let fix = engine(GeneratedFixDraft {
            explanation: "verify the token".to_string(),
            new_content: "const a = 1;\nconst token = jwt.verify(x, key);\n".to_string(),
            warnings: vec!["local model".to_string()],
        });
        let generated = crate::test_support::block_on(fix.generate("/repo", &finding()))
            .expect("generation succeeds");

        assert_eq!(generated.proposal.file_path, "src/auth.ts");
        assert_eq!(generated.proposal.original_line_count, 2);
        assert_eq!(generated.proposal.original_sha256, "sha-42");
        assert_eq!(generated.proposal.edits.len(), 1);
        assert_eq!(generated.proposal.edits[0].start_offset, 0);
        assert_eq!(generated.proposal.edits[0].end_offset, 42);
        assert_eq!(
            generated
                .proposal
                .apply_edits("const a = 1;\nconst token = jwt.decode(x);\n")
                .expect("edit applies"),
            generated.proposal.new_content
        );
        assert!(generated
            .warnings
            .iter()
            .any(|warning| warning.contains("local model")));
        assert!(generated
            .warnings
            .iter()
            .any(|warning| warning.contains("tier 2") || warning.contains("Tier 2")));
    }

    #[test]
    fn an_unchanged_file_is_rejected() {
        let fix = engine(GeneratedFixDraft {
            explanation: "no change".to_string(),
            new_content: "const a = 1;\nconst token = jwt.decode(x);\n".to_string(),
            warnings: Vec::new(),
        });
        let error = crate::test_support::block_on(fix.generate("/repo", &finding()))
            .expect_err("unchanged files are rejected");
        assert!(error.to_string().contains("unchanged"), "{error}");
    }

    #[test]
    fn an_empty_file_is_rejected() {
        let fix = engine(GeneratedFixDraft {
            explanation: "nothing".to_string(),
            new_content: "   \n".to_string(),
            warnings: Vec::new(),
        });
        let error = crate::test_support::block_on(fix.generate("/repo", &finding()))
            .expect_err("empty files are rejected");
        assert!(error.to_string().contains("empty"), "{error}");
    }

    #[test]
    fn an_oversized_answer_is_rejected() {
        let fix = engine(GeneratedFixDraft {
            explanation: "too much".to_string(),
            new_content: "x".repeat(MAX_OUTPUT_BYTES + 1),
            warnings: Vec::new(),
        });
        let error = crate::test_support::block_on(fix.generate("/repo", &finding()))
            .expect_err("oversized answers are rejected");
        assert!(error.to_string().contains("budget"), "{error}");
    }

    #[test]
    fn a_missing_trailing_newline_is_warned() {
        let fix = engine(GeneratedFixDraft {
            explanation: "fix".to_string(),
            new_content: "const a = 1;".to_string(),
            warnings: Vec::new(),
        });
        let generated = crate::test_support::block_on(fix.generate("/repo", &finding()))
            .expect("generation succeeds");
        assert!(generated
            .warnings
            .iter()
            .any(|warning| warning.contains("newline")));
    }
}
