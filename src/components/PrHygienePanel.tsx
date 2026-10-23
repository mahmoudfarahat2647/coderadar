import React from "react";
import {
  AlertTriangle,
  CheckCircle2,
  FileCode2,
  GitPullRequest,
  Layers,
  Ruler,
  ShieldQuestion,
  XCircle,
} from "lucide-react";
import { PrHygieneReport } from "../types";

interface PrHygienePanelProps {
  report: PrHygieneReport | null;
  filesScanned: number;
}

interface HygieneGate {
  rule: string;
  label: string;
  failed: boolean;
  detail: string;
}

const GATE_CLASS = {
  pass: "border-status-clean/30 bg-status-clean/10 text-status-clean",
  fail: "border-status-critical/30 bg-status-critical/10 text-status-critical",
};

const StatChip: React.FC<{ label: string; value: React.ReactNode }> = ({
  label,
  value,
}) => (
  <div className="px-3 py-2 rounded-lg bg-surface-canvas border border-border-subtle">
    <div className="text-[10px] uppercase tracking-wide text-text-muted">{label}</div>
    <div className="font-mono text-sm text-text-primary tabular-nums">{value}</div>
  </div>
);

export const PrHygienePanel: React.FC<PrHygienePanelProps> = ({
  report,
  filesScanned,
}) => {
  if (!report) {
    return (
      <div className="flex-1 flex items-center justify-center p-8">
        <div className="max-w-lg p-6 rounded-lg bg-surface-panel border border-border-subtle text-center">
          <GitPullRequest className="w-8 h-8 text-secondary mx-auto mb-3" />
          <h2 className="font-headline font-semibold text-sm text-text-primary mb-2">
            No Staged Changeset Scanned Yet
          </h2>
          <p className="text-xs text-text-muted leading-relaxed">
            Switch the scan mode to <span className="font-mono">Git Staged Diff</span>{" "}
            and run a scan against a repository to score its pull request hygiene:
            scope creep, public API removals and missing tests.
          </p>
        </div>
      </div>
    );
  }

  const gates: HygieneGate[] = [
    {
      rule: "PR-SCP-001",
      label: "Scope stays focused",
      failed: report.scope_creep,
      detail:
        report.areas_touched.length > 0
          ? `Areas touched: ${report.areas_touched.join(", ")}`
          : "No unrelated top-level areas touched.",
    },
    {
      rule: "PR-BRK-002",
      label: "No public API removal",
      failed: report.breaking_change,
      detail: report.breaking_change
        ? "The staged diff removes or renames a public API surface."
        : "No public API removals detected in the staged diff.",
    },
    {
      rule: "PR-TST-003",
      label: "Tests cover production changes",
      failed: report.missing_tests,
      detail: report.missing_tests
        ? "Production files changed without a matching test change."
        : "Every production change is accompanied by a test change.",
    },
  ];
  const passed = gates.filter((gate) => !gate.failed).length;
  const score = Math.round((passed / gates.length) * 100);
  const clean = passed === gates.length;

  return (
    <div className="flex-1 overflow-y-auto p-4 space-y-4">
      <div className="flex flex-col md:flex-row gap-4">
        <div
          className={`flex-1 rounded-lg border p-4 flex items-center justify-between ${
            clean ? GATE_CLASS.pass : GATE_CLASS.fail
          }`}
        >
          <div>
            <div className="text-[10px] uppercase tracking-wide opacity-80">
              Pull Request Hygiene
            </div>
            <div className="font-headline text-2xl font-semibold tabular-nums">
              {score}%
            </div>
            <div className="text-xs opacity-90 mt-1">
              {clean
                ? "All hygiene gates passed"
                : `${gates.length - passed} of ${gates.length} gates need attention`}
            </div>
          </div>
          {clean ? (
            <CheckCircle2 className="w-8 h-8" />
          ) : (
            <AlertTriangle className="w-8 h-8" />
          )}
        </div>

        <div className="flex-1 rounded-lg bg-surface-panel border border-border-subtle p-4">
          <div className="flex items-center space-x-2 text-xs font-semibold text-text-primary mb-3">
            <GitPullRequest className="w-4 h-4 text-secondary" />
            <span>Changeset Base</span>
            <span className="font-mono text-[10px] px-1.5 py-0.5 rounded bg-surface-elevated border border-border-subtle text-text-muted">
              {report.base}
            </span>
          </div>
          <div className="grid grid-cols-2 lg:grid-cols-4 gap-2">
            <StatChip label="Files changed" value={report.files_changed} />
            <StatChip label="Additions" value={`+${report.additions}`} />
            <StatChip label="Deletions" value={`-${report.deletions}`} />
            <StatChip label="Lines changed" value={report.lines_changed} />
          </div>
        </div>
      </div>

      <div className="grid grid-cols-1 lg:grid-cols-3 gap-3">
        {gates.map((gate) => (
          <div
            key={gate.rule}
            className={`rounded-lg border p-3 ${
              gate.failed ? GATE_CLASS.fail : GATE_CLASS.pass
            }`}
          >
            <div className="flex items-center justify-between mb-1.5">
              <span className="font-mono text-[10px] opacity-80">{gate.rule}</span>
              {gate.failed ? (
                <XCircle className="w-4 h-4" />
              ) : (
                <CheckCircle2 className="w-4 h-4" />
              )}
            </div>
            <div className="text-xs font-semibold mb-1">{gate.label}</div>
            <p className="text-[11px] leading-relaxed opacity-90">{gate.detail}</p>
          </div>
        ))}
      </div>

      <div className="grid grid-cols-1 lg:grid-cols-3 gap-3">
        <div className="rounded-lg bg-surface-panel border border-border-subtle p-3">
          <div className="flex items-center space-x-1.5 text-[11px] text-text-muted mb-2">
            <FileCode2 className="w-3.5 h-3.5" />
            <span>Changed file mix</span>
          </div>
          <div className="text-xs text-text-primary space-y-1">
            <div className="flex items-center justify-between">
              <span>Production</span>
              <span className="font-mono tabular-nums">
                {report.production_files_changed}
              </span>
            </div>
            <div className="flex items-center justify-between">
              <span>Tests</span>
              <span className="font-mono tabular-nums">
                {report.test_files_changed}
              </span>
            </div>
          </div>
        </div>

        <div className="rounded-lg bg-surface-panel border border-border-subtle p-3">
          <div className="flex items-center space-x-1.5 text-[11px] text-text-muted mb-2">
            <Layers className="w-3.5 h-3.5" />
            <span>Areas touched</span>
          </div>
          {report.areas_touched.length === 0 ? (
            <span className="text-xs text-text-muted">Single area only</span>
          ) : (
            <div className="flex flex-wrap gap-1.5">
              {report.areas_touched.map((area) => (
                <span
                  key={area}
                  className="px-1.5 py-0.5 rounded text-[10px] font-mono bg-surface-elevated border border-border-subtle text-text-secondary"
                >
                  {area}
                </span>
              ))}
            </div>
          )}
        </div>

        <div className="rounded-lg bg-surface-panel border border-border-subtle p-3">
          <div className="flex items-center space-x-1.5 text-[11px] text-text-muted mb-2">
            <Ruler className="w-3.5 h-3.5" />
            <span>Files scanned</span>
          </div>
          <div className="font-mono text-sm text-text-primary tabular-nums">
            {filesScanned}
          </div>
          <div className="text-[10px] text-text-muted mt-1">
            Staged text files evaluated by the rule engine
          </div>
        </div>
      </div>

      {!clean && (
        <div className="rounded-lg bg-status-medium/10 border border-status-medium/30 p-3 text-[11px] text-text-secondary flex items-start space-x-2">
          <ShieldQuestion className="w-4 h-4 text-status-medium shrink-0 mt-0.5" />
          <span>
            Every failed gate is also emitted as a finding in the{" "}
            <span className="font-mono">pull_requests</span> category, so it can be
            triaged in the findings feed and exported to the Markdown report.
          </span>
        </div>
      )}
    </div>
  );
};
