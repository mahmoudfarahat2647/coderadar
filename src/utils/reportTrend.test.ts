import assert from "node:assert/strict";
import { trendDelta, trendPoints, trendSegments } from "./trendGeometry";

const samples = [80, null, 60, 70, null].map((compliance_percent) => ({ compliance_percent }));
const points = trendPoints(samples, 600, 300, { top: 20, right: 20, bottom: 30, left: 40 });
assert.deepEqual(points.map((point) => point.index), [0, 2, 3]);
assert.deepEqual(trendSegments(points).map((segment) => segment.map((point) => point.index)), [[0], [2, 3]]);
assert.equal(trendDelta(samples), null);
assert.equal(trendDelta([{ compliance_percent: 0 }, { compliance_percent: 100 }]), 100);
assert.deepEqual(trendPoints([{ compliance_percent: null }], 600, 300, { top: 20, right: 20, bottom: 30, left: 40 }), []);
