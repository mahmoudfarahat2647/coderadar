import React, { useEffect, useMemo, useRef, useState } from "react";
import { Header } from "./components/Header";
import { MetricsRibbon } from "./components/MetricsRibbon";
import { ScanBar } from "./components/ScanBar";
import { FiltersBar } from "./components/FiltersBar";
import { FindingsFeed } from "./components/FindingsFeed";
import { PolarRadarCanvas } from "./components/PolarRadarCanvas";
import { FindingDrawer } from "./components/FindingDrawer";
import { ApiKeyDialog } from "./components/ApiKeyDialog";
import { DiffModal } from "./components/DiffModal";
import { PrHygienePanel } from "./components/PrHygienePanel";
import { TrendChart } from "./components/TrendChart";
import { StatusBar } from "./components/StatusBar";
import {
  EngineStatus,
  FilterCategory,
  FilterSeverity,
  FindingBatch,
  FindingDTO,
  HistoryPointDTO,
  PatchPreviewDTO,
  PrHygieneReport,
  ScanMode,
  ScanReport,
  ScanStats,
  SeverityTotals,
} from "./types";
import {
  exportReport,
  getDefaultScanPath,
  getEngineStatus,
  getScanHistory,
  isTauriEnvironment,
  listenToScanBatches,
  openFolderDialog,
  pickReportPath,
  previewPatch,
  startScan,
} from "./services/tauri";
import { AlertCircle, X, Loader2 } from "lucide-react";

export const App: React.FC = () => {
  const [targetPath, setTargetPath] = useState<string>("tests/fixtures/known-bad-repo");
  const [scanMode, setScanMode] = useState<ScanMode>("full_scan");
  const [isScanning, setIsScanning] = useState<boolean>(false);
  const [findings, setFindings] = useState<FindingDTO[]>([]);
  const [selectedFinding, setSelectedFinding] = useState<FindingDTO | null>(null);
  const [hoveredFindingId, setHoveredFindingId] = useState<string | null>(null);
  const [activeTab, setActiveTab] = useState<string>("dashboard");
  const [activeSeverity, setActiveSeverity] = useState<FilterSeverity>("ALL");
  const [activeCategory, setActiveCategory] = useState<FilterCategory>("ALL");
  const [searchQuery, setSearchQuery] = useState<string>("");
  const [engineStatus, setEngineStatus] = useState<EngineStatus | null>(null);
  const [scanStats, setScanStats] = useState<ScanStats | null>(null);
  const [errorMessage, setErrorMessage] = useState<string | null>(null);
  const [infoBanner, setInfoBanner] = useState<string | null>(null);
  const [apiKeyDialogOpen, setApiKeyDialogOpen] = useState<boolean>(false);
  const [diffPreview, setDiffPreview] = useState<PatchPreviewDTO | null>(null);
  const [diffFinding, setDiffFinding] = useState<FindingDTO | null>(null);
  const [prHygiene, setPrHygiene] = useState<PrHygieneReport | null>(null);
  const [patchBusy, setPatchBusy] = useState<boolean>(false);
  const [historyPoints, setHistoryPoints] = useState<HistoryPointDTO[]>([]);
  const [scannedPath, setScannedPath] = useState<string>("");

  const refreshEngineStatus = () => {
    getEngineStatus().then((s) => {
      if (s) setEngineStatus(s);
    });
  };

  // Poll the engine status while a scan runs, so a live -> Mock fallback flips
  // the badge the moment Jev drops instead of only at scan end.
  const isScanningRef = useRef(false);
  isScanningRef.current = isScanning;

  // The workspace of the last scan, readable from the async history callbacks
  // (they are registered once, so they would otherwise close over a stale
  // empty path).
  const scannedPathRef = useRef<string>("");
  scannedPathRef.current = scannedPath;

  // The workspace currently in view, readable from the async history
  // callbacks as well.
  const targetPathRef = useRef<string>("");
  targetPathRef.current = targetPath;

  useEffect(() => {
    const interval = setInterval(() => {
      if (isScanningRef.current) refreshEngineStatus();
    }, 2000);
    return () => clearInterval(interval);
  }, []);

  // Initialize path and status
  useEffect(() => {
    getDefaultScanPath().then((p) => {
      if (p) setTargetPath(p);
    });
    getEngineStatus().then((s) => {
      if (s) setEngineStatus(s);
    });

    // Listen to streaming scan events from the Rust 100 ms throttler
    let unlisten: (() => void) | undefined;
    listenToScanBatches(
      (batch: FindingBatch) => {
        setFindings((prev) => {
          const map = new Map<string, FindingDTO>();
          prev.forEach((f) => map.set(f.id, f));
          batch.findings.forEach((f) => {
            if (!map.has(f.id)) {
              map.set(f.id, f);
            }
          });
          return Array.from(map.values());
        });
      },
      (report: ScanReport) => {
        setIsScanning(false);
        setScanStats(report.scan);
        setPrHygiene(report.pr_hygiene ?? null);
        // Refresh the engine badge: a scan may have triggered a Mock fallback.
        refreshEngineStatus();
        // The finished scan is now persisted in the SQLite history.
        getScanHistory(20, scannedPathRef.current || targetPathRef.current).then(
          (points) => {
            setHistoryPoints(points);
          }
        );
        if (report.findings && report.findings.length > 0) {
          setFindings((prev) => {
            const map = new Map<string, FindingDTO>();
            prev.forEach((f) => map.set(f.id, f));
            report.findings.forEach((f) => map.set(f.id, f));
            return Array.from(map.values());
          });
        }
      },
      (err: string) => {
        setIsScanning(false);
        setErrorMessage(err);
      }
    ).then((cleanup) => {
      unlisten = cleanup;
    });

    return () => {
      if (unlisten) unlisten();
    };
  }, []);

  // The trend follows the workspace in view: the history is queried per
  // workspace, so switching projects reloads the points of that project.
  useEffect(() => {
    getScanHistory(20, targetPath.trim()).then((points) => {
      setHistoryPoints(points);
    });
  }, [targetPath]);

  // Handle Tab Switch
  const handleTabChange = (tab: string) => {
    setActiveTab(tab);
    if (tab === "architecture") {
      setActiveCategory("architecture");
    } else if (tab === "security") {
      setActiveCategory("security");
    } else if (tab === "dashboard") {
      setActiveCategory("ALL");
    } else if (tab === "radar") {
      setActiveCategory("ALL");
    } else if (tab === "trend") {
      setActiveCategory("ALL");
    } else if (tab === "prs") {
      setActiveCategory("pull_requests");
    }
  };

  // Open Native Folder Dialog
  const handlePickFolder = async () => {
    try {
      const selected = await openFolderDialog();
      if (selected) {
        setTargetPath(selected);
      }
    } catch (err) {
      setErrorMessage(`Folder picker error: ${err}`);
    }
  };

  // Quick button to load Known-Bad test fixture
  const handleUseFixture = () => {
    setTargetPath("tests/fixtures/known-bad-repo");
  };

  // Toggle Resolve on a finding
  const handleToggleResolve = (id: string, e?: React.MouseEvent) => {
    if (e) e.stopPropagation();
    setFindings((prev) =>
      prev.map((f) => (f.id === id ? { ...f, resolved: !f.resolved } : f))
    );
    if (selectedFinding?.id === id) {
      setSelectedFinding((prev) =>
        prev ? { ...prev, resolved: !prev.resolved } : null
      );
    }
  };

  // Ask the Rust engine for a safe remediation proposal and open the diff modal
  const handleProposeFix = async (finding: FindingDTO) => {
    setErrorMessage(null);
    // Patch the workspace that produced the finding, never a path typed later.
    const root = scannedPath;
    if (!root) {
      setErrorMessage("Run a scan before proposing a remediation.");
      return;
    }
    setPatchBusy(true);
    try {
      const preview = await previewPatch(root, finding.file, finding.rule_id);
      if (!preview) {
        setErrorMessage(
          "Patch preview is only available in the CodeRadar desktop app (Tauri runtime)."
        );
        return;
      }
      setDiffFinding(finding);
      setDiffPreview(preview);
    } catch (err) {
      setErrorMessage(typeof err === "string" ? err : String(err));
    } finally {
      setPatchBusy(false);
    }
  };

  // Export the report of the last scan as a Markdown document (Phase 7).
  const handleExportReport = async () => {
    setErrorMessage(null);
    try {
      const targetPath = await pickReportPath();
      // The user closed the save dialog: nothing to do.
      if (!targetPath) return;

      const outcome = await exportReport(targetPath);
      if (!outcome) {
        setErrorMessage(
          "Report export is only available in the CodeRadar desktop app (Tauri runtime)."
        );
        return;
      }
      setInfoBanner(
        `Report exported to ${outcome.path} (${outcome.findings} findings, ${outcome.bytes} bytes).`
      );
    } catch (err) {
      setErrorMessage(typeof err === "string" ? err : String(err));
    }
  };

  // Start Scan
  const handleStartScan = async () => {
    if (!targetPath.trim()) return;

    setErrorMessage(null);
    setInfoBanner(null);
    setFindings([]);
    setPrHygiene(null);
    setIsScanning(true);
    // The findings are relative to the workspace that is actually scanned:
    // patches must be applied to that root, not to a path typed later on.
    setScannedPath(targetPath.trim());

    if (isTauriEnvironment()) {
      try {
        await startScan(targetPath, scanMode);
      } catch (err) {
        setIsScanning(false);
        setErrorMessage(typeof err === "string" ? err : String(err));
      }
    } else {
      // Browser simulation fallback for 15 fixture findings
      setTimeout(() => {
        const mockFixtureFindings: FindingDTO[] = [
          {
            id: "ARCH-DEP-001:src/domain/user.ts:5",
            file: "src/domain/user.ts",
            line_start: 5,
            line_end: 5,
            rule_id: "ARCH-DEP-001",
            category: "architecture",
            severity: "CRITICAL",
            cwe: "CWE-1068",
            title: "Dependency Rule violation",
            description: "Inner layer imports outer database driver framework (@prisma/client)",
            radar_r: 0.95,
            radar_theta: 45.0,
          },
          {
            id: "SEC-TOK-002:src/auth/config.ts:14",
            file: "src/auth/config.ts",
            line_start: 14,
            line_end: 14,
            rule_id: "SEC-TOK-002",
            category: "security",
            severity: "CRITICAL",
            cwe: "CWE-798",
            title: "Hardcoded secret token detected",
            description: "High entropy AWS secret access key detected: AKIA...[REDACTED]",
            radar_r: 0.92,
            radar_theta: 135.0,
          },
          {
            id: "SEC-JWT-003:src/auth/jwt_verifier.ts:35",
            file: "src/auth/jwt_verifier.ts",
            line_start: 35,
            line_end: 85,
            rule_id: "SEC-JWT-003",
            category: "security",
            severity: "CRITICAL",
            cwe: "CWE-347",
            title: "Unvalidated RSA key header algorithm injection",
            description: "JWT token decoded without signature validation and algorithms restriction",
            radar_r: 0.88,
            radar_theta: 140.0,
          },
          {
            id: "SEC-TLS-005:src/infra/gateway.ts:22",
            file: "src/infra/gateway.ts",
            line_start: 22,
            line_end: 22,
            rule_id: "SEC-TLS-005",
            category: "security",
            severity: "HIGH",
            cwe: "CWE-326",
            title: "Insecure TLS cipher version allowed",
            description: "Insecure plaintext HTTP connection and TLS 1.0 pinned in client options",
            radar_r: 0.75,
            radar_theta: 125.0,
          },
          {
            id: "SEC-INJ-001:src/db/query.ts:18",
            file: "src/db/query.ts",
            line_start: 18,
            line_end: 18,
            rule_id: "SEC-INJ-001",
            category: "security",
            severity: "HIGH",
            cwe: "CWE-89",
            title: "SQL injection vulnerability",
            description: "Direct string concatenation into SQL statement without parameterization",
            radar_r: 0.78,
            radar_theta: 130.0,
          },
          {
            id: "EDGE-NPE-001:src/utils/parser.ts:42",
            file: "src/utils/parser.ts",
            line_start: 42,
            line_end: 42,
            rule_id: "EDGE-NPE-001",
            category: "edge_cases",
            severity: "MEDIUM",
            cwe: "CWE-476",
            title: "Unchecked nullable property dereference",
            description: "Potentially undefined object accessed without optional chaining or null guard",
            radar_r: 0.55,
            radar_theta: 310.0,
          },
          {
            id: "PERF-NPO-001:src/services/order.ts:88",
            file: "src/services/order.ts",
            line_start: 88,
            line_end: 88,
            rule_id: "PERF-NPO-001",
            category: "performance",
            severity: "MEDIUM",
            cwe: "CWE-400",
            title: "Potential N+1 database query in loop",
            description: "Database lookup executed inside map iteration instead of batch query",
            radar_r: 0.52,
            radar_theta: 220.0,
          },
        ];

        setFindings(mockFixtureFindings);
        setIsScanning(false);
        setScanStats({
          files_scanned: 12,
          ast_chunks: 18,
          module_chunks: 12,
          duration_ms: 142,
        });
        if (scanMode === "staged_diff") {
          setPrHygiene({
            base: "HEAD",
            files_changed: 6,
            additions: 148,
            deletions: 37,
            lines_changed: 185,
            production_files_changed: 5,
            test_files_changed: 1,
            areas_touched: ["docs", "src"],
            scope_creep: true,
            breaking_change: false,
            missing_tests: true,
          });
        }
      }, 400);
    }
  };

  // Severity Totals
  const severityTotals: SeverityTotals = useMemo(() => {
    const totals: SeverityTotals = {
      total: findings.length,
      critical: 0,
      high: 0,
      medium: 0,
      low: 0,
    };
    findings.forEach((f) => {
      if (f.severity === "CRITICAL") totals.critical++;
      else if (f.severity === "HIGH") totals.high++;
      else if (f.severity === "MEDIUM") totals.medium++;
      else if (f.severity === "LOW") totals.low++;
    });
    return totals;
  }, [findings]);

  // Filtered Findings
  const filteredFindings = useMemo(() => {
    return findings.filter((f) => {
      // Severity Filter
      if (activeSeverity !== "ALL" && f.severity !== activeSeverity) {
        return false;
      }
      // Category Filter
      if (activeCategory !== "ALL" && f.category !== activeCategory) {
        return false;
      }
      // Search Query
      if (searchQuery.trim()) {
        const query = searchQuery.toLowerCase();
        const matchFile = f.file.toLowerCase().includes(query);
        const matchRule = f.rule_id.toLowerCase().includes(query);
        const matchTitle = f.title.toLowerCase().includes(query);
        const matchCwe = f.cwe?.toLowerCase().includes(query) ?? false;
        if (!matchFile && !matchRule && !matchTitle && !matchCwe) {
          return false;
        }
      }
      return true;
    });
  }, [findings, activeSeverity, activeCategory, searchQuery]);

  // Derived Telemetry
  const underReviewCount = findings.filter((f) => !f.resolved).length;
  const resolvedCount = findings.filter((f) => f.resolved).length;
  const blockedCount = findings.filter(
    (f) => !f.resolved && (f.severity === "CRITICAL" || f.severity === "HIGH")
  ).length;

  const totalFiles = scanStats ? scanStats.files_scanned : 12;
  const filesWithFindings = new Set(findings.map((f) => f.file)).size;
  const passedPercent =
    totalFiles > 0
      ? Math.max(0, ((totalFiles - filesWithFindings) / totalFiles) * 100)
      : 100;
  const analyzedLoc = scanStats ? scanStats.files_scanned * 320 : 12480;

  return (
    <div className="flex flex-col h-screen w-screen overflow-hidden bg-surface-canvas text-text-primary">
      {/* Top Header */}
      <Header
        engineStatus={engineStatus}
        activeTab={activeTab}
        onTabChange={handleTabChange}
        onExportReport={handleExportReport}
        exportDisabled={isScanning}
      />

      {/* Info Banner */}
      {infoBanner && (
        <div className="bg-tertiary/10 border-b border-tertiary/20 px-4 py-2 text-xs text-tertiary flex items-center justify-between select-none">
          <span>{infoBanner}</span>
          <button
            onClick={() => setInfoBanner(null)}
            className="text-text-muted hover:text-text-primary p-0.5"
          >
            <X className="w-3.5 h-3.5" />
          </button>
        </div>
      )}

      {/* Error Banner */}
      {errorMessage && (
        <div className="bg-status-critical/15 border-b border-status-critical/30 px-4 py-2 text-xs text-status-critical flex items-center justify-between select-none">
          <div className="flex items-center space-x-2">
            <AlertCircle className="w-4 h-4 shrink-0" />
            <span>{errorMessage}</span>
          </div>
          <button
            onClick={() => setErrorMessage(null)}
            className="text-status-critical/80 hover:text-status-critical p-0.5"
          >
            <X className="w-3.5 h-3.5" />
          </button>
        </div>
      )}

      {/* Patch Proposal Progress */}
      {patchBusy && (
        <div className="bg-tertiary/10 border-b border-tertiary/20 px-4 py-2 text-xs text-tertiary flex items-center space-x-2 select-none">
          <Loader2 className="w-3.5 h-3.5 animate-spin" />
          <span>Preparing remediation proposal…</span>
        </div>
      )}

      {/* Scan Controller Bar */}
      <ScanBar
        targetPath={targetPath}
        onPathChange={setTargetPath}
        onPickFolder={handlePickFolder}
        scanMode={scanMode}
        onScanModeChange={setScanMode}
        onStartScan={handleStartScan}
        isScanning={isScanning}
        onUseFixture={handleUseFixture}
      />

      {/* Live Metrics Ribbon */}
      <MetricsRibbon
        analyzedLoc={analyzedLoc}
        underReview={underReviewCount}
        resolved={resolvedCount}
        passedPercent={passedPercent}
        blockedCount={blockedCount}
        isScanning={isScanning}
      />

      {/* Filter and Category Ribbon */}
      <FiltersBar
        activeSeverity={activeSeverity}
        onSeverityChange={setActiveSeverity}
        activeCategory={activeCategory}
        onCategoryChange={setActiveCategory}
        searchQuery={searchQuery}
        onSearchChange={setSearchQuery}
        severityTotals={severityTotals}
      />

      {/* Main Content Area (360° Polar Radar Canvas + Findings Feed + Remediation Drawer) */}
      <div className="flex-1 flex overflow-hidden relative">
        {/* Left Pane: Polar Radar Canvas */}
        {activeTab !== "trend" && activeTab !== "prs" && (
          <div
            className={`flex flex-col shrink-0 transition-all duration-300 relative border-r border-border-subtle ${
              activeTab === "radar"
                ? "flex-1 w-full"
                : "w-[420px] lg:w-[480px] xl:w-[540px]"
            }`}
          >
            <PolarRadarCanvas
              findings={filteredFindings}
              hoveredFindingId={hoveredFindingId}
              selectedFinding={selectedFinding}
              onHoverFinding={setHoveredFindingId}
              onSelectFinding={setSelectedFinding}
              cleanPassedPercent={passedPercent}
              isScanning={isScanning}
            />
          </div>
        )}

        {/* Right Pane: Structured Findings Feed */}
        {activeTab !== "radar" && activeTab !== "trend" && activeTab !== "prs" && (
          <div className="flex-1 flex flex-col min-w-0 overflow-hidden bg-surface-panel/40">
            <FindingsFeed
              findings={filteredFindings}
              selectedFinding={selectedFinding}
              hoveredFindingId={hoveredFindingId}
              onSelectFinding={setSelectedFinding}
              onHoverFinding={setHoveredFindingId}
              onToggleResolve={handleToggleResolve}
              isScanning={isScanning}
            />
          </div>
        )}

        {/* Quality Trend Chart (Phase 6) */}
        {activeTab === "trend" && <TrendChart points={historyPoints} />}

        {/* Pull Request Hygiene Scorecard (Phase 8) */}
        {activeTab === "prs" && (
          <PrHygienePanel
            report={prHygiene}
            filesScanned={scanStats?.files_scanned ?? 0}
          />
        )}

        {/* Slide-over Finding Detail & Remediation Drawer */}
        {selectedFinding && (
          <FindingDrawer
            finding={selectedFinding}
            onClose={() => setSelectedFinding(null)}
            onToggleResolve={(id) => handleToggleResolve(id)}
            onProposeFix={handleProposeFix}
          />
        )}
      </div>

      {/* Bottom Telemetry Status Bar */}
      <StatusBar
        engineStatus={engineStatus}
        scanStats={scanStats}
        isScanning={isScanning}
        onOpenApiKey={() => setApiKeyDialogOpen(true)}
      />

      {/* API Key Settings */}
      <ApiKeyDialog
        open={apiKeyDialogOpen}
        engineStatus={engineStatus}
        onClose={() => setApiKeyDialogOpen(false)}
        onStatusChange={setEngineStatus}
        onError={setErrorMessage}
      />

      {/* Remediation Diff Modal */}
      <DiffModal
        open={diffPreview !== null}
        preview={diffPreview}
        rootPath={scannedPath}
        finding={diffFinding}
        onPreviewGenerated={setDiffPreview}
        onClose={() => {
          setDiffPreview(null);
          setDiffFinding(null);
        }}
        onApplied={(outcome) => {
          setInfoBanner(outcome.message);
          setDiffPreview(null);
          setDiffFinding(null);
        }}
        onError={(message) => setErrorMessage(message)}
      />
    </div>
  );
};
