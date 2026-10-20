import React, { useMemo } from "react";
import { Activity, TrendingUp, TrendingDown } from "lucide-react";
import { HistoryPointDTO } from "../types";
import {
  TrendPadding,
  TrendPoint,
  buildTrendScale,
  trendDelta,
  trendPoints,
  trendPolyline,
} from "../utils/trendGeometry";

interface TrendChartProps {
  points: HistoryPointDTO[];
}

/** Fixed SVG coordinate space; the viewport scales responsively. */
const WIDTH = 860;
const HEIGHT = 340;
const PADDING: TrendPadding = { top: 28, right: 28, bottom: 38, left: 46 };

const GRID_VALUES = [0, 25, 50, 75, 100];

/** Quality trend chart: compliance percentage over the recorded scan history. */
export const TrendChart: React.FC<TrendChartProps> = ({ points }) => {
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

  const polyline = useMemo(() => trendPolyline(projected), [projected]);
  const delta = useMemo(() => trendDelta(ordered), [ordered]);
  const { scale } = useMemo(
    () => buildTrendScale(WIDTH, HEIGHT, PADDING),
    []
  );

  const areaPath = useMemo(() => {
    if (projected.length === 0) return "";
    const baseline = HEIGHT - PADDING.bottom;
    const line = projected
      .map((point) => `L ${point.x} ${point.y}`)
      .join(" ");
    const first = projected[0];
    const last = projected[projected.length - 1];
    return `M ${first.x} ${baseline} ${line} L ${last.x} ${baseline} Z`;
  }, [projected]);

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
            latest {latest.toFixed(1)}%
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

          {/* Area fill under the line */}
          {projected.length > 1 && (
            <path d={areaPath} className="fill-primary/10" />
          )}

          {/* Trend line */}
          {projected.length > 1 && (
            <polyline
              points={polyline}
              fill="none"
              stroke="currentColor"
              strokeWidth={2}
              strokeLinejoin="round"
              strokeLinecap="round"
              className="text-secondary"
            />
          )}

          {/* Data points */}
          {projected.map((point) => (
            <g key={point.index}>
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
            x={projected[0].x}
            y={HEIGHT - 14}
            textAnchor="start"
            className="fill-text-muted font-mono text-[10px] tabular-nums"
          >
            {new Date(firstPoint.finished_at_unix_ms).toLocaleDateString()}
          </text>
          {projected.length > 1 && (
            <text
              x={projected[projected.length - 1].x}
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
