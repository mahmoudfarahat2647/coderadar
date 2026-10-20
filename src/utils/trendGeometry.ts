/**
 * CodeRadar Studio — Quality Trend Chart Geometry (Phase 6).
 *
 * Pure, deterministic geometry helpers for the compliance trend chart.
 * All maths lives here (never inside the React component) so the invariants
 * can be verified by a plain Node test script.
 */

export interface TrendPoint {
  /** Absolute SVG x coordinate. */
  x: number;
  /** Absolute SVG y coordinate. */
  y: number;
  /** The raw compliance percentage behind the point. */
  value: number;
  /** Zero-based position of the point in the ordered series. */
  index: number;
}

export interface TrendPadding {
  top: number;
  right: number;
  bottom: number;
  left: number;
}

export interface TrendScale {
  /** Maps a compliance percentage (0..100) to an absolute SVG y coordinate. */
  scale: (value: number) => number;
  /** Width of the plot area, excluding padding. */
  plotWidth: number;
  /** Height of the plot area, excluding padding. */
  plotHeight: number;
}

interface ComplianceSample {
  compliance_percent: number;
}

/** Rounds to two decimals so the emitted SVG strings stay readable. */
function round2(value: number): number {
  return Math.round(value * 100) / 100;
}

/**
 * Builds the linear y scale for a 0..100 compliance axis.
 * `scale(0)` is the bottom of the plot area, `scale(100)` the top.
 */
export function buildTrendScale(
  points: ComplianceSample[],
  width: number,
  height: number,
  padding: TrendPadding
): TrendScale {
  const plotWidth = Math.max(0, width - padding.left - padding.right);
  const plotHeight = Math.max(0, height - padding.top - padding.bottom);

  const scale = (value: number): number => {
    const clamped = Math.max(0, Math.min(100, value));
    return height - padding.bottom - (clamped / 100) * plotHeight;
  };

  // `points` is accepted for symmetry with `trendPoints` and future extensions.
  void points;

  return { scale, plotWidth, plotHeight };
}

/**
 * Projects the ordered compliance series onto absolute SVG coordinates.
 * A single sample is centred on the x axis (never a division by zero).
 */
export function trendPoints(
  points: ComplianceSample[],
  width: number,
  height: number,
  padding: TrendPadding
): TrendPoint[] {
  const { scale, plotWidth } = buildTrendScale(points, width, height, padding);
  const count = points.length;

  return points.map((point, index) => {
    const ratio = count > 1 ? index / (count - 1) : 0.5;
    const x = padding.left + ratio * plotWidth;
    return {
      x: round2(x),
      y: round2(scale(point.compliance_percent)),
      value: point.compliance_percent,
      index,
    };
  });
}

/**
 * Serialises projected points into an SVG `points` attribute value.
 */
export function trendPolyline(points: TrendPoint[]): string {
  return points
    .map((point) => `${round2(point.x)},${round2(point.y)}`)
    .join(" ");
}

/**
 * Returns the compliance delta between the last and the first sample.
 * `null` when fewer than two samples are available.
 */
export function trendDelta(points: ComplianceSample[]): number | null {
  if (points.length < 2) return null;
  return points[points.length - 1].compliance_percent - points[0].compliance_percent;
}
