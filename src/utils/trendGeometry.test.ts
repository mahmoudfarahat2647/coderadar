/**
 * Automated tests for the quality trend chart geometry (Phase 6).
 */

import assert from "node:assert";
import {
  buildTrendScale,
  trendDelta,
  trendPoints,
  trendPolyline,
} from "./trendGeometry";

console.log("Running trend geometry specification tests...");

const WIDTH = 600;
const HEIGHT = 300;
const PADDING = { top: 20, right: 20, bottom: 30, left: 40 };

const samples = (values: number[]) => values.map((v) => ({ compliance_percent: v }));

// 1. Invariant: Y-Scale inversion (0% at the bottom, 100% at the top)
{
  const { scale, plotWidth, plotHeight } = buildTrendScale(
    WIDTH,
    HEIGHT,
    PADDING
  );

  assert.strictEqual(plotWidth, WIDTH - PADDING.left - PADDING.right, "Plot width must exclude padding");
  assert.strictEqual(plotHeight, HEIGHT - PADDING.top - PADDING.bottom, "Plot height must exclude padding");

  assert.strictEqual(scale(0), HEIGHT - PADDING.bottom, "0% must map to the bottom of the plot area");
  assert.strictEqual(scale(100), PADDING.top, "100% must map to the top of the plot area");
  assert(scale(50) > scale(100), "50% must render below 100% (Canvas Y-inversion)");
  assert(scale(50) < scale(0), "50% must render above 0% (Canvas Y-inversion)");

  // Out-of-range values are clamped instead of escaping the plot area.
  assert.strictEqual(scale(-20), scale(0), "Negative compliance must clamp to the bottom");
  assert.strictEqual(scale(140), scale(100), "Compliance above 100 must clamp to the top");

  console.log("✓ Y-scale inversion and clamping verified.");
}

// 2. Invariant: X positions are strictly increasing and stay inside the plot area
{
  const values = [10, 42, 55, 80];
  const projected = trendPoints(samples(values), WIDTH, HEIGHT, PADDING);

  assert.strictEqual(projected.length, values.length, "Every sample must produce a point");
  assert.strictEqual(projected[0].x, PADDING.left, "The first point must sit on the left padding edge");

  for (let i = 1; i < projected.length; i++) {
    assert(
      projected[i].x > projected[i - 1].x,
      `X must strictly increase (index ${i})`
    );
  }

  projected.forEach((point, i) => {
    assert(
      point.x >= PADDING.left && point.x <= WIDTH - PADDING.right,
      `X must stay inside the plot area (index ${i})`
    );
    assert.strictEqual(point.value, values[i], "Point value must mirror the source sample");
    assert.strictEqual(point.index, i, "Point index must mirror the chronological position");
  });

  console.log("✓ X ordering and plot-area containment verified.");
}

// 3. Invariant: A single sample never produces NaN and stays centred
{
  const projected = trendPoints(samples([73]), WIDTH, HEIGHT, PADDING);

  assert.strictEqual(projected.length, 1, "A single sample must produce exactly one point");
  assert(!Number.isNaN(projected[0].x), "X must not be NaN for a single sample");
  assert(!Number.isNaN(projected[0].y), "Y must not be NaN for a single sample");
  assert.strictEqual(projected[0].x, (PADDING.left + (WIDTH - PADDING.right)) / 2, "A single sample must be centred");
  assert.strictEqual(projected[0].value, 73, "A single sample must keep its value");

  console.log("✓ Single-point degeneracy guard verified.");
}

// 4. Invariant: The polyline string carries one coordinate pair per point
{
  const projected = trendPoints(samples([10, 20, 30, 40, 50]), WIDTH, HEIGHT, PADDING);
  const polyline = trendPolyline(projected);
  const coordinates = polyline.split(" ");

  assert.strictEqual(coordinates.length, projected.length, "Polyline must hold one coordinate per point");
  coordinates.forEach((coordinate, i) => {
    const parts = coordinate.split(",");
    assert.strictEqual(parts.length, 2, "Each coordinate must be an x,y pair");
    assert(!Number.isNaN(Number(parts[0])), "Polyline X must be numeric");
    assert(!Number.isNaN(Number(parts[1])), "Polyline Y must be numeric");
    assert.strictEqual(Number(parts[0]), projected[i].x, "Polyline X must match the projected point");
    assert.strictEqual(Number(parts[1]), projected[i].y, "Polyline Y must match the projected point");
  });

  assert.strictEqual(trendPolyline([]), "", "An empty series must produce an empty polyline");

  console.log("✓ Polyline serialisation verified.");
}

// 5. Invariant: The delta is null below two samples and equals last - first otherwise
{
  assert.strictEqual(trendDelta(samples([])), null, "An empty series must have no delta");
  assert.strictEqual(trendDelta(samples([42])), null, "A single sample must have no delta");
  assert.strictEqual(trendDelta(samples([40, 65])), 25, "The delta must equal last - first");
  assert.strictEqual(trendDelta(samples([65, 40])), -25, "A regression must yield a negative delta");
  assert.strictEqual(trendDelta(samples([10, 20, 30])), 20, "The delta must span the whole series");

  console.log("✓ Delta computation verified.");
}

console.log("All trend geometry tests passed successfully!");
