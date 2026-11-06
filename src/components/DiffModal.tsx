import React, { useEffect, useMemo, useRef, useState } from "react";
import {
  X,
  Copy,
  CheckCheck,
  FileText,
  ShieldAlert,
  AlertTriangle,
  Loader2,
  Sparkles,
  Cpu,
  Cloud,
} from "lucide-react";
import { FindingDTO, PatchOutcomeDTO, PatchPreviewDTO, Tier2Provider } from "../types";
import { applyPatch, generateFix } from "../services/tauri";

interface DiffModalProps {
  open: boolean;
  preview: PatchPreviewDTO | null;
  rootPath: string;
  finding: FindingDTO | null;
  fixesAvailable: boolean;
  onPreviewGenerated: (preview: PatchPreviewDTO) => void;
  onClose: () => void;
  onApplied: (outcome: PatchOutcomeDTO) => void;
  onError: (message: string) => void;
}

type DiffLineKind = "removed" | "added" | "hunk" | "file" | "context";

/**
 * Classifies a single unified-diff line. File headers are detected before the
 * removal/addition prefixes because "---" and "+++" also start with "-" and "+".
 */
function classifyDiffLine(line: string): DiffLineKind {
  if (line.startsWith("@@")) return "hunk";
  if (line.startsWith("---") || line.startsWith("+++")) return "file";
  if (line.startsWith("-")) return "removed";
  if (line.startsWith("+")) return "added";
  return "context";
}

const DIFF_LINE_CLASS: Record<DiffLineKind, string> = {
  removed: "text-status-critical bg-status-critical/5",
  added: "text-status-clean bg-status-clean/5",
  hunk: "text-tertiary",
  file: "text-text-muted",
  context: "text-text-secondary",
};

/** Modal showing a proposed remediation diff and driving the atomic apply. */
export const DiffModal: React.FC<DiffModalProps> = ({
  open,
  preview,
  rootPath,
  finding,
  fixesAvailable,
  onPreviewGenerated,
  onClose,
  onApplied,
  onError,
}) => {
  const [copied, setCopied] = useState(false);
  const [busy, setBusy] = useState(false);
  const [outcome, setOutcome] = useState<PatchOutcomeDTO | null>(null);
  const [provider, setProvider] = useState<Tier2Provider>("ollama");
  const [allowCloud, setAllowCloud] = useState(false);
  const [generating, setGenerating] = useState(false);

  // Keep the latest close handler without re-registering the key listener.
  const onCloseRef = useRef(onClose);
  onCloseRef.current = onClose;

  // Escape closes the modal while it is open.
  useEffect(() => {
    if (!open) return;
    const handleKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") onCloseRef.current();
    };
    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, [open]);

  // A new preview starts from a clean slate.
  useEffect(() => {
    setCopied(false);
    setBusy(false);
    setGenerating(false);
    setOutcome(null);
  }, [preview]);

  const diffLines = useMemo(() => {
    if (!preview || !preview.unified_diff) return [];
    return preview.unified_diff.split("\n");
  }, [preview]);

  if (!open || !preview) return null;

  // A missing proposal means the rule has no deterministic fix: the modal
  // explains why instead of offering an apply action (CR-SPEC-13 §5).
  const proposal = preview.proposal;
  const hasProposal = proposal !== null;

  const handleCopy = () => {
    if (!hasProposal) return;
    navigator.clipboard.writeText(preview.unified_diff);
    setCopied(true);
    setTimeout(() => setCopied(false), 2000);
  };

  const handleApply = async () => {
    if (busy || !proposal || !fixesAvailable) return;
    setBusy(true);
    setOutcome(null);
    try {
      const result = await applyPatch(rootPath, proposal);
      if (!result) {
        onError(
          "Patching is only available in the CodeRadar desktop app (Tauri runtime)."
        );
        return;
      }
      setOutcome(result);
      if (result.status === "applied") {
        onApplied(result);
      }
    } catch (err) {
      onError(typeof err === "string" ? err : String(err));
    } finally {
      setBusy(false);
    }
  };

  const handleGenerate = async () => {
    if (generating || !finding || !fixesAvailable) return;
    if (provider === "gemini" && !allowCloud) {
      onError(
        "Cloud fix generation needs explicit consent: review the privacy notice and tick the consent box first."
      );
      return;
    }
    setGenerating(true);
    try {
      const generated = await generateFix({
        root_path: rootPath,
        finding_id: finding.id,
        file_path: preview.file_path,
        rule_id: preview.rule_id,
        title: finding.title,
        description: finding.description,
        line_start: finding.line_start,
        line_end: finding.line_end,
        provider,
        allow_cloud: provider === "gemini" && allowCloud,
      });
      if (!generated) {
        onError(
          "Tier 2 fix generation is only available in the CodeRadar desktop app (Tauri runtime)."
        );
        return;
      }
      onPreviewGenerated(generated);
    } catch (err) {
      onError(typeof err === "string" ? err : String(err));
    } finally {
      setGenerating(false);
    }
  };

  const isApplied = outcome?.status === "applied";
  const isRejected = outcome?.status === "conflict" || outcome?.status === "rejected";

  return (
    <div
      className="fixed inset-0 bg-black/60 z-50 flex items-center justify-center p-4"
      onClick={onClose}
    >
      <div
        className="w-full max-w-3xl bg-surface-panel border border-border-strong rounded-lg shadow-2xl max-h-[85vh] flex flex-col"
        onClick={(e) => e.stopPropagation()}
      >
        {/* Header */}
        <div className="p-4 border-b border-border-subtle flex items-start justify-between gap-3">
          <div className="flex-1 min-w-0">
            <div className="flex items-center space-x-2 mb-1.5 flex-wrap">
              <span className="px-2 py-0.5 rounded-full text-[10px] font-mono uppercase bg-secondary/15 text-secondary border border-secondary/30">
                {preview.rule_id}
              </span>
              <span className="flex items-center space-x-1.5 text-text-muted text-[11px] min-w-0">
                <FileText className="w-3.5 h-3.5 shrink-0" />
                <span className="font-mono break-all">{preview.file_path}</span>
              </span>
            </div>
            <h2 className="font-headline font-semibold text-sm text-text-primary leading-snug">
              {preview.title || preview.rule_id}
            </h2>
          </div>

          <button
            onClick={onClose}
            className="p-1 rounded-md text-text-muted hover:text-text-primary hover:bg-surface-elevated transition-colors"
            title="Close"
          >
            <X className="w-4 h-4" />
          </button>
        </div>

        {/* Body */}
        <div className="flex-1 overflow-y-auto p-4 space-y-3 text-xs">
          {!fixesAvailable && (
            <div role="alert" className="p-3 rounded-lg bg-status-medium/10 border border-status-medium/30 text-text-secondary">
              Fixes are unavailable for staged scans or while a scan is running.
            </div>
          )}
          {/* Fingerprint strip */}
          <div className="flex items-center flex-wrap gap-2">
            <span className="px-2 py-0.5 rounded-full text-[10px] font-mono uppercase bg-surface-elevated text-text-muted border border-border-subtle">
              orig {preview.original_sha256.slice(0, 12)}
            </span>
            <span className="px-2 py-0.5 rounded-full text-[10px] font-mono uppercase bg-surface-elevated text-text-muted border border-border-subtle">
              new {preview.new_sha256.slice(0, 12)}
            </span>
            <span className="px-2 py-0.5 rounded-full text-[10px] font-mono uppercase bg-status-clean/15 text-status-clean border border-status-clean/30 tabular-nums">
              +{preview.added_lines}
            </span>
            <span className="px-2 py-0.5 rounded-full text-[10px] font-mono uppercase bg-status-critical/15 text-status-critical border border-status-critical/30 tabular-nums">
              -{preview.removed_lines}
            </span>
            <span className="px-2 py-0.5 rounded-full text-[10px] font-mono uppercase bg-surface-elevated text-text-muted border border-border-subtle tabular-nums">
              {preview.original_line_count} lines
            </span>
          </div>

          {/* Explanation */}
          {preview.explanation && (
            <div className="p-3 rounded-lg bg-surface-canvas border border-border-subtle text-text-secondary leading-relaxed text-[11px]">
              {preview.explanation}
            </div>
          )}

          {/* No deterministic fix: offer a Tier 2 (LLM) proposal instead */}
          {!hasProposal && (
            <div className="p-3 rounded-lg bg-status-medium/10 border border-status-medium/30 text-[11px] text-text-secondary space-y-3">
              <div>
                <div className="flex items-center space-x-1.5 font-semibold text-status-medium mb-1">
                  <AlertTriangle className="w-3.5 h-3.5" />
                  <span>No Deterministic Fix Exists For This Rule</span>
                </div>
                <p className="leading-relaxed">
                  The remediation template does not match this code exactly, so no
                  safe deterministic patch can be proposed. A Tier 2 model can
                  generate one for review instead; nothing is written until you
                  apply the resulting diff.
                </p>
              </div>

              <div className="flex flex-wrap items-center gap-2">
                <div className="bg-surface-canvas border border-border-subtle rounded-lg p-0.5 flex items-center text-[11px]">
                  <button
                    onClick={() => setProvider("ollama")}
                    disabled={generating}
                    className={`px-2.5 py-1 rounded-md flex items-center space-x-1.5 transition-colors ${
                      provider === "ollama"
                        ? "bg-surface-elevated text-secondary font-semibold border border-border-subtle"
                        : "text-text-muted hover:text-text-primary"
                    }`}
                  >
                    <Cpu className="w-3.5 h-3.5" />
                    <span>Local Ollama</span>
                  </button>
                  <button
                    onClick={() => setProvider("gemini")}
                    disabled={generating}
                    className={`px-2.5 py-1 rounded-md flex items-center space-x-1.5 transition-colors ${
                      provider === "gemini"
                        ? "bg-surface-elevated text-secondary font-semibold border border-border-subtle"
                        : "text-text-muted hover:text-text-primary"
                    }`}
                  >
                    <Cloud className="w-3.5 h-3.5" />
                    <span>Cloud Gemini</span>
                  </button>
                </div>

                <span className="text-[10px] text-text-muted">
                  {provider === "ollama"
                    ? "Source stays on this machine."
                    : "Only files that pass full secret redaction are sent."}
                </span>
              </div>

              {provider === "gemini" && (
                <label className="flex items-start space-x-2 p-2 rounded-md bg-surface-canvas border border-border-subtle cursor-pointer">
                  <input
                    type="checkbox"
                    checked={allowCloud}
                    onChange={(e) => setAllowCloud(e.target.checked)}
                    disabled={generating}
                    className="mt-0.5"
                  />
                  <span className="text-[10px] leading-relaxed">
                    I consent to sending this fully redacted file to the configured
                    Gemini model. The backend refuses the request when the file
                    contains any secret that cannot be redacted.
                  </span>
                </label>
              )}

              <div className="flex items-center justify-between gap-2">
                <span className="text-[10px] text-text-muted">
                  The generated fix is validated against the on-disk file and shown
                  as a reviewable diff.
                </span>
                <button
                  onClick={handleGenerate}
                  disabled={
                    generating ||
                    !fixesAvailable ||
                    !finding ||
                    (provider === "gemini" && !allowCloud)
                  }
                  className="px-3 py-1.5 rounded-md bg-secondary/15 border border-secondary/40 text-secondary hover:bg-secondary/25 transition-colors disabled:opacity-50 disabled:cursor-not-allowed flex items-center space-x-1.5 text-xs font-semibold shrink-0"
                >
                  {generating ? (
                    <>
                      <Loader2 className="w-3.5 h-3.5 animate-spin" />
                      <span>Generating...</span>
                    </>
                  ) : (
                    <>
                      <Sparkles className="w-3.5 h-3.5" />
                      <span>Generate Fix</span>
                    </>
                  )}
                </button>
              </div>
            </div>
          )}

          {/* Warnings */}
          {preview.warnings.length > 0 && (
            <div className="p-3 rounded-lg bg-status-medium/10 border border-status-medium/30 text-[11px] text-text-secondary">
              <div className="flex items-center space-x-1.5 font-semibold text-status-medium mb-1">
                <AlertTriangle className="w-3.5 h-3.5" />
                <span>Warnings</span>
              </div>
              <ul className="list-disc pl-4 space-y-0.5">
                {preview.warnings.map((warning, index) => (
                  <li key={index}>{warning}</li>
                ))}
              </ul>
            </div>
          )}

          {/* Outcome banner */}
          {isApplied && outcome && (
            <div className="p-3 rounded-lg bg-status-clean/10 border border-status-clean/30 text-[11px] text-status-clean flex items-start space-x-2">
              <ShieldAlert className="w-3.5 h-3.5 shrink-0 mt-0.5" />
              <span>{outcome.message}</span>
            </div>
          )}
          {isRejected && outcome && (
            <div className="p-3 rounded-lg bg-status-critical/10 border border-status-critical/30 text-[11px] text-status-critical flex items-start space-x-2">
              <AlertTriangle className="w-3.5 h-3.5 shrink-0 mt-0.5" />
              <span>{outcome.message}</span>
            </div>
          )}

          {/* Unified diff viewport */}
          <div className="rounded-lg bg-surface-canvas border border-border-subtle overflow-auto max-h-80">
            {diffLines.length === 0 ? (
              <div className="p-3 font-mono text-[11px] text-text-muted">
                {hasProposal
                  ? "No textual diff available for this proposal."
                  : "No diff yet: generate a Tier 2 fix above to preview one."}
              </div>
            ) : (
              <div className="min-w-max py-1 font-mono text-[11px] leading-relaxed">
                {diffLines.map((line, index) => {
                  const kind = classifyDiffLine(line);
                  return (
                    <div
                      key={index}
                      className={`px-3 whitespace-pre ${DIFF_LINE_CLASS[kind]}`}
                    >
                      {line}
                    </div>
                  );
                })}
              </div>
            )}
          </div>
        </div>

        {/* Footer actions */}
        <div className="p-3 border-t border-border-subtle bg-surface-canvas flex items-center justify-end gap-2">
          <button
            onClick={handleCopy}
            disabled={busy || !hasProposal || !fixesAvailable}
            className="px-3 py-1.5 rounded-md bg-surface-elevated border border-border-subtle text-text-primary hover:bg-surface-overlay transition-colors disabled:opacity-50 disabled:cursor-not-allowed flex items-center space-x-1.5 text-xs"
          >
            {copied ? (
              <>
                <CheckCheck className="w-3.5 h-3.5 text-secondary" />
                <span>Copied!</span>
              </>
            ) : (
              <>
                <Copy className="w-3.5 h-3.5" />
                <span>Copy Patch</span>
              </>
            )}
          </button>

          <button
            onClick={handleApply}
            disabled={busy || !hasProposal || !fixesAvailable}
            className="px-3 py-1.5 rounded-md bg-primary hover:bg-[#239f63] text-white text-xs font-semibold flex items-center space-x-1.5 transition-colors shadow-glow disabled:opacity-50 disabled:cursor-not-allowed"
          >
            {busy ? (
              <>
                <Loader2 className="w-3.5 h-3.5 animate-spin" />
                <span>Applying…</span>
              </>
            ) : (
              <span>Apply Patch</span>
            )}
          </button>
        </div>
      </div>
    </div>
  );
};
