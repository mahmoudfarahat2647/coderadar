import React, { useState } from "react";
import { X, Check, Copy, ShieldAlert, FileText, CheckCheck, Wand2 } from "lucide-react";
import { FindingDTO } from "../types";

interface FindingDrawerProps {
  finding: FindingDTO | null;
  onClose: () => void;
  onToggleResolve: (id: string) => void;
  onProposeFix: (finding: FindingDTO) => void;
}

export const FindingDrawer: React.FC<FindingDrawerProps> = ({
  finding,
  onClose,
  onToggleResolve,
  onProposeFix,
}) => {
  const [copied, setCopied] = useState(false);

  if (!finding) return null;

  const lineSpan =
    finding.line_start === finding.line_end
      ? `line ${finding.line_start}`
      : `lines ${finding.line_start}–${finding.line_end}`;

  const getRemediationDiff = () => {
    switch (finding.rule_id) {
      case "SEC-JWT-003":
        return `[-] const decoded = jwt.decode(token, { complete: true });
[-] if (decoded.header.alg === 'none') { ... }
[+] const verified = jwt.verify(token, pubKey, {
[+]   algorithms: ['RS256']
[+] });`;
      case "SEC-TOK-002":
        return `[-] const API_KEY = "sk-live-9a8b7c6d5e4f3a2b1c0d";
[+] const API_KEY = process.env.CODERADAR_API_KEY;
[+] if (!API_KEY) throw new Error("Missing required API key environment variable");`;
      case "SEC-TLS-005":
        return `[-] const client = axios.create({ baseURL: "http://api.internal.service" });
[-] client.defaults.httpsAgent = new https.Agent({ rejectUnauthorized: false });
[+] const client = axios.create({ baseURL: "https://api.internal.service" });
[+] client.defaults.httpsAgent = new https.Agent({ minVersion: "TLSv1.2" });`;
      case "ARCH-DEP-001":
        return `[-] import { PrismaClient } from "@prisma/client"; // Violation: Domain depends on database driver
[+] // Depend inward on an abstract repository interface defined in the domain layer:
[+] import type { UserRepository } from "./ports/user_repository";`;
      default:
        return `[-] // Detected anti-pattern or vulnerability at ${finding.file}:${finding.line_start}
[+] // Remediate following Clean Architecture and defensive standards:
[+] // ${finding.description}`;
    }
  };

  const copyPatch = () => {
    navigator.clipboard.writeText(getRemediationDiff());
    setCopied(true);
    setTimeout(() => setCopied(false), 2000);
  };

  return (
    <div className="w-full lg:w-[480px] border-l border-border-subtle bg-surface-panel/95 backdrop-blur flex flex-col h-full shadow-2xl z-30 select-none animate-in slide-in-from-right duration-200">
      {/* Header */}
      <div className="p-4 border-b border-border-subtle flex items-start justify-between gap-3">
        <div className="flex-1 min-w-0">
          <div className="flex items-center space-x-2 mb-1.5">
            <span
              className={`px-2 py-0.5 rounded text-[10px] font-mono font-bold uppercase border ${
                finding.severity === "CRITICAL"
                  ? "bg-status-critical/15 text-status-critical border-status-critical/30"
                  : finding.severity === "HIGH"
                  ? "bg-status-high/15 text-status-high border-status-high/30"
                  : finding.severity === "MEDIUM"
                  ? "bg-status-medium/15 text-status-medium border-status-medium/30"
                  : "bg-status-low/15 text-status-low border-status-low/30"
              }`}
            >
              {finding.severity}
            </span>
            {finding.cwe && (
              <span className="px-1.5 py-0.5 rounded text-[10px] font-mono text-tertiary bg-tertiary/10 border border-tertiary/25">
                {finding.cwe}
              </span>
            )}
            <span className="font-mono text-xs text-text-muted">{finding.rule_id}</span>
          </div>
          <h2 className="font-headline font-semibold text-sm text-text-primary leading-snug">
            {finding.title || finding.rule_id}
          </h2>
        </div>

        <button
          onClick={onClose}
          className="p-1 rounded-md text-text-muted hover:text-text-primary hover:bg-surface-elevated transition-colors"
          title="Close Drawer"
        >
          <X className="w-4 h-4" />
        </button>
      </div>

      {/* Body content */}
      <div className="flex-1 overflow-y-auto p-4 space-y-4 text-xs">
        {/* Location Box */}
        <div className="p-3 rounded-lg bg-surface-elevated border border-border-subtle">
          <div className="text-[11px] text-text-muted mb-1 flex items-center space-x-1.5">
            <FileText className="w-3.5 h-3.5 text-text-muted" />
            <span>Target File Location</span>
          </div>
          <div className="font-mono text-xs text-secondary font-semibold break-all">
            {finding.file}:{finding.line_start}
          </div>
          <div className="text-[11px] text-text-muted mt-1">
            Analyzing {lineSpan} (Risk Score: {(finding.radar_r * 100).toFixed(0)}%)
          </div>
        </div>

        {/* Detailed Explanation */}
        <div>
          <div className="text-xs font-semibold text-text-primary mb-1">
            Architectural Analysis & Threat Model
          </div>
          <div className="p-3 rounded-lg bg-surface-canvas border border-border-subtle text-text-secondary leading-relaxed text-[11px]">
            {finding.description}
          </div>
        </div>

        {/* Remediation Diff Snippet */}
        <div>
          <div className="flex items-center justify-between mb-1">
            <span className="text-xs font-semibold text-text-primary">
              Recommended Remediation Patch
            </span>
            <button
              onClick={copyPatch}
              className="text-[11px] text-secondary hover:underline flex items-center space-x-1"
            >
              {copied ? (
                <>
                  <CheckCheck className="w-3 h-3 text-secondary" />
                  <span>Copied!</span>
                </>
              ) : (
                <>
                  <Copy className="w-3 h-3" />
                  <span>Copy Patch</span>
                </>
              )}
            </button>
          </div>
          <pre className="p-3 rounded-lg bg-surface-canvas border border-border-subtle font-mono text-[11px] overflow-x-auto text-text-primary leading-relaxed">
            {getRemediationDiff()
              .split("\n")
              .map((line, idx) => {
                if (line.startsWith("[-]")) {
                  return (
                    <div key={idx} className="text-status-critical bg-status-critical/5">
                      {line}
                    </div>
                  );
                } else if (line.startsWith("[+]")) {
                  return (
                    <div key={idx} className="text-status-clean bg-status-clean/5 font-medium">
                      {line}
                    </div>
                  );
                }
                return <div key={idx} className="text-text-muted">{line}</div>;
              })}
          </pre>
        </div>

        {/* Governance / Clean Architecture note */}
        <div className="p-3 rounded-lg bg-primary/10 border border-primary/20 text-[11px] text-text-secondary">
          <div className="flex items-center space-x-1.5 font-semibold text-secondary mb-1">
            <ShieldAlert className="w-3.5 h-3.5" />
            <span>Strict Architectural Invariant</span>
          </div>
          <p>
            Dependencies must point only inward toward higher-level policies. No outer framework or
            volatile dependency can be imported by inner enterprise layers.
          </p>
        </div>
      </div>

      {/* Footer Actions */}
      <div className="p-3 border-t border-border-subtle bg-surface-canvas space-y-2">
        <button
          onClick={() => onProposeFix(finding)}
          className="w-full py-2 px-3 rounded-lg bg-secondary/15 border border-secondary/40 text-secondary hover:bg-secondary/25 transition-colors text-xs font-semibold flex items-center justify-center space-x-1.5"
          title="Ask the remediation engine for a safe, reviewable patch (no deterministic fix may exist for this rule)"
        >
          <Wand2 className="w-3.5 h-3.5" />
          <span>Propose Fix</span>
        </button>

        <div className="flex items-center justify-between gap-2">
          <button
            onClick={() => onToggleResolve(finding.id)}
            className={`flex-1 py-2 px-3 rounded-lg border text-xs font-semibold flex items-center justify-center space-x-1.5 transition-colors ${
              finding.resolved
                ? "bg-status-clean/20 border-status-clean/40 text-status-clean"
                : "bg-surface-elevated hover:bg-surface-elevated/80 border-border-subtle text-text-primary"
            }`}
          >
            <Check className="w-3.5 h-3.5" />
            <span>{finding.resolved ? "Resolved (Re-open)" : "Mark as Resolved"}</span>
          </button>

          <button
            onClick={copyPatch}
            className="flex-1 py-2 px-3 rounded-lg bg-primary hover:bg-[#239f63] text-white text-xs font-semibold flex items-center justify-center space-x-1.5 transition-colors shadow-glow"
          >
            <Copy className="w-3.5 h-3.5" />
            <span>Copy Patch</span>
          </button>
        </div>
      </div>
    </div>
  );
};
