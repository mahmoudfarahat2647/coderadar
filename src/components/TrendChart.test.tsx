import assert from "node:assert/strict";
import React from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { TrendChart } from "./TrendChart";
import type { HistoryPointDTO } from "../types";

const point: HistoryPointDTO = {
  id: "1", root_dir: "root", mode: "full_scan", finished_at_unix_ms: 1,
  files_scanned: 1, total_findings: 0, critical: 0, high: 0, medium: 0, low: 0,
  suppressed_heuristic_findings: 0, engine_mode: null, compliance_percent: null,
  coverage: null, completion: null,
};
const render = (points: HistoryPointDTO[], error: string | null = null) =>
  renderToStaticMarkup(React.createElement(TrendChart, { points, error }));

assert.match(render([point]), /Completion: Unknown/);
assert.match(render([{ ...point, completion: "partial" }]), /Completion: partial/);
assert.match(render([], "database unavailable"), /database unavailable/);
assert.doesNotMatch(render([], "database unavailable"), /No Scan History Yet/);
assert.match(render([point]), /Usage: Unknown/);
assert.match(render([{ ...point, usage: { primary_batches: 3, fallback_batches: 2, fallback_failures: 1 } }]), /Primary batches: 3; Fallback batches: 2; Fallback failures: 1/);
assert.match(render([{ ...point, usage: { primary_batches: 0, fallback_batches: 0, fallback_failures: 0 } }]), /Primary batches: 0; Fallback batches: 0; Fallback failures: 0/);
