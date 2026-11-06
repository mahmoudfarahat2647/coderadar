import React, { useMemo } from "react";
import { Activity, TrendingUp, TrendingDown } from "lucide-react";
import { HistoryPointDTO } from "../types";
import { engineModeLabel } from "../utils/engineTone";
import {
  TrendPadding,
  TrendPoint,
  buildTrendScale,
  trendDelta,
  trendPoints,
  trendPolyline,
  trendSegments,
} from "../utils/trendGeometry";

interface TrendChartProps {
  points: HistoryPointDTO[];
  error?: string | null;
  loading?: boolean;
}

/** Fixed SVG coordinate space; the viewport scales responsively. */
const WIDTH = 860;
const HEIGHT = 340;
const PADDING: TrendPadding = { top: 28, right: 28, bottom: 38, left: 46 };

const GRID_VALUES = [0, 25, 50, 75, 100];

/** Quality trend chart: compliance percentage over the recorded scan history. */
export const TrendChart: React.FC<TrendChartProps> = ({ points, error, loading }) => {
  // The backend returns the newest scan first; the chart plots oldest -> newest.
  const ordered = useMemo(
    () =>
      [...points].sort(
        (a, b) => a.finished_at_unix_ms - b.finished_at_unix_ms
      ),
    [points]
  );

  const projected: TrendPoint[] = useMemo(
    () => trendPoints(ordered, WIDTH, HEIGHT, PADDING),
    [ordered]
  );

  const segments = useMemo(() => trendSegments(projected), [projected]);
  const delta = useMemo(() => ordered.some((point) => point.coverage == null) ? null : trendDelta(ordered), [ordered]);
  const { scale } = useMemo(
    () => buildTrendScale(WIDTH, HEIGHT, PADDING),
    []
  );

  if (error || loading) {
    return <div className="flex-1 p-8 text-sm text-text-muted" role={error ? "alert" : "status"}>{error ? `Scan history could not be read: ${error}` : "Loading scan history..."}</div>;
  }

  if (ordered.length === 0) {
    return (
      <div className="flex-1 flex items-center justify-center p-8">
        <div className="w-full max-w-md bg-surface-panel border border-border-subtle rounded-lg p-6 text-center">
          <Activity className="w-6 h-6 text-text-muted mx-auto mb-2" />
          <div className="font-headline font-semibold text-sm text-text-primary mb-1">
            No Scan History Yet
          </div>
          <p className="text-[11px] text-text-muted leading-relaxed">
            The quality trend appears once the first scan has been recorded in the
            local scan history.
          </p>
        </div>
      </div>
    );
  }

  const firstPoint = ordered[0];
  const lastPoint = ordered[ordered.length - 1];
  const latest = lastPoint.compliance_percent;
  const baseline = HEIGHT - PADDING.bottom;

  return (
    <div className="flex-1 flex flex-col min-w-0 overflow-hidden bg-surface-panel/40 p-4 space-y-3">
      {/* Summary row */}
      <div className="flex items-center justify-between flex-wrap gap-2">
        <div className="flex items-center space-x-2">
          <Activity className="w-4 h-4 text-secondary" />
          <span className="font-headline font-semibold text-sm text-text-primary">
            Quality Trend
          </span>
          <span className="px-2 py-0.5 rounded-full text-[10px] font-mono uppercase bg-surface-elevated text-text-muted border border-border-subtle tabular-nums">
            {ordered.length} scans
          </span>
        </div>
        <div className="flex items-center space-x-2">
          <span className="px-2 py-0.5 rounded-full text-[10px] font-mono uppercase bg-secondary/15 text-secondary border border-secondary/30 tabular-nums">
            latest {latest == null ? "unavailable" : `${latest.toFixed(1)}%${lastPoint.coverage == null ? " (legacy)" : ""}`}
          </span>
          {delta !== null && (
            <span
              className={`px-2 py-0.5 rounded-full text-[10px] font-mono uppercase border tabular-nums flex items-center space-x-1 ${
                delta > 0
                  ? "bg-status-clean/15 text-status-clean border-status-clean/30"
                  : delta < 0
                  ? "bg-status-critical/15 text-status-critical border-status-critical/30"
                  : "bg-surface-elevated text-text-muted border-border-subtle"
              }`}
            >
              {delta > 0 ? (
                <TrendingUp className="w-3 h-3" />
              ) : delta < 0 ? (
                <TrendingDown className="w-3 h-3" />
              ) : null}
              <span>
                {delta > 0 ? "+" : ""}
                {delta.toFixed(1)} pts
              </span>
            </span>
          )}
        </div>
      </div>

      <p className="text-[11px] text-text-muted">Share of analyzed files without reported findings. Gaps indicate unavailable scores; older scores with unknown coverage are legacy metrics.</p>
      <details className="text-[11px] text-text-muted">
          <summary>Scan coverage and usage details</summary>
        <ul>
          {ordered.map((point) => (
            <li key={point.id}>
              {new Date(point.finished_at_unix_ms).toLocaleString()} · {`Completion: ${point.completion ?? "Unknown"}`} · {point.compliance_percent == null ? "Score unavailable" : `${point.compliance_percent}%${point.coverage == null ? " (legacy score)" : ""}`} · {point.coverage == null ? "Coverage not recorded" : `${point.coverage.discovered} discovered, ${point.coverage.supported} supported, ${point.coverage.analyzed} analyzed, ${point.coverage.unsupported} unsupported, ${point.coverage.excluded} excluded (accounting ${point.coverage.exclusions_complete === true ? "complete" : point.coverage.exclusions_complete === false ? "incomplete" : "unknown"}), ${point.coverage.unreadable} unreadable, ${point.coverage.oversized} oversized, ${point.coverage.invalid_encoding} invalid encoding, ${point.coverage.incomplete} incomplete`} · {point.usage == null ? "Usage: Unknown" : `Primary batches: ${point.usage.primary_batches}; Fallback batches: ${point.usage.fallback_batches}; Fallback failures: ${point.usage.fallback_failures}`}
            </li>
          ))}
        </ul>
      </details>

      {/* Chart viewport */}
      <div className="flex-1 min-h-0 bg-surface-canvas border border-border-subtle rounded-lg p-3 overflow-auto">
        <svg
          viewBox={`0 0 ${WIDTH} ${HEIGHT}`}
          className="w-full h-auto"
          role="img"
          aria-label="Compliance percentage trend over recorded scans"
        >
          {/* Horizontal gridlines and Y axis labels */}
          {GRID_VALUES.map((value) => {
            const y = scale(value);
            return (
              <g key={value}>
                <line
                  x1={PADDING.left}
                  y1={y}
                  x2={WIDTH - PADDING.right}
                  y2={y}
                  stroke="#282828"
                  strokeWidth={1}
                />
                <text
                  x={PADDING.left - 10}
                  y={y + 3}
                  textAnchor="end"
                  className="fill-text-muted font-mono text-[10px] tabular-nums"
                >
                  {value}
                </text>
              </g>
            );
          })}

          {/* X axis */}
          <line
            x1={PADDING.left}
            y1={baseline}
            x2={WIDTH - PADDING.right}
            y2={baseline}
            stroke="#383838"
            strokeWidth={1}
          />

          {/* Trend line */}
          {segments.filter((segment) => segment.length > 1).map((segment) => (
            <polyline
              key={segment[0].index}
              points={trendPolyline(segment)}
              fill="none"
              stroke="currentColor"
              strokeWidth={2}
              strokeLinejoin="round"
              strokeLinecap="round"
              className="text-secondary"
            />
          ))}

          {/* Data points */}
          {projected.map((point) => (
            <g key={point.index}>
              <title>
                {`${new Date(ordered[point.index].finished_at_unix_ms).toLocaleString()} · ${engineModeLabel(
                  ordered[point.index].engine_mode
                )} · ${ordered[point.index].coverage == null ? "legacy score; coverage unknown" : `${ordered[point.index].coverage!.analyzed} analyzed of ${ordered[point.index].coverage!.supported} supported`} · ${ordered[point.index].total_findings} findings${
                  ordered[point.index].suppressed_heuristic_findings > 0
                    ? ` · ${ordered[point.index].suppressed_heuristic_findings} suppressed`
                    : ""
                }`}
              </title>
              <circle
                cx={point.x}
                cy={point.y}
                r={3.5}
                className="fill-secondary stroke-surface-canvas"
                strokeWidth={1.5}
              />
              <text
                x={point.x}
                y={point.y - 9}
                textAnchor="middle"
                className="fill-text-secondary font-mono text-[9px] tabular-nums"
              >
                {point.value.toFixed(0)}
              </text>
            </g>
          ))}

          {/* First / last date labels */}
          <text
            x={PADDING.left}
            y={HEIGHT - 14}
            textAnchor="start"
            className="fill-text-muted font-mono text-[10px] tabular-nums"
          >
            {new Date(firstPoint.finished_at_unix_ms).toLocaleDateString()}
          </text>
          {ordered.length > 1 && (
            <text
              x={WIDTH - PADDING.right}
              y={HEIGHT - 14}
              textAnchor="end"
              className="fill-text-muted font-mono text-[10px] tabular-nums"
            >
              {new Date(lastPoint.finished_at_unix_ms).toLocaleDateString()}
            </text>
          )}
        </svg>
      </div>
    </div>
  );
};
