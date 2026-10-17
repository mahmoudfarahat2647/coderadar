/**
 * Automated tests for radar coordinate transformations and invariants (CR-SPEC-05 §2).
 */

import assert from "node:assert";
import {
  projectPolarToCanvas,
  projectFindings,
  clusterRadarNodes,
  hitTestRadar,
} from "./radarGeometry";
import type { FindingDTO } from "../types";

console.log("Running radarGeometry mathematical specification tests...");

// 1. Invariant: Y-Axis Inversion (0° - 90° lands in Top-Right quadrant: x > x0, y < y0)
{
  const center = 200;
  const maxR = 150;
  // 45 deg (Architecture)
  const pt45 = projectPolarToCanvas(0.8, 45, center, center, maxR);
  assert(pt45.x > center, "45 deg X must be > center (Right)");
  assert(pt45.y < center, "45 deg Y must be < center (Top due to Canvas Y-inversion)");

  // 135 deg (Security) -> Top-Left: x < center, y < center
  const pt135 = projectPolarToCanvas(0.8, 135, center, center, maxR);
  assert(pt135.x < center, "135 deg X must be < center (Left)");
  assert(pt135.y < center, "135 deg Y must be < center (Top)");

  // 225 deg (Performance) -> Bottom-Left: x < center, y > center
  const pt225 = projectPolarToCanvas(0.8, 225, center, center, maxR);
  assert(pt225.x < center, "225 deg X must be < center (Left)");
  assert(pt225.y > center, "225 deg Y must be > center (Bottom)");

  // 315 deg (Edge Cases) -> Bottom-Right: x > center, y > center
  const pt315 = projectPolarToCanvas(0.8, 315, center, center, maxR);
  assert(pt315.x > center, "315 deg X must be > center (Right)");
  assert(pt315.y > center, "315 deg Y must be > center (Bottom)");

  console.log("✓ Quadrant mapping and Y-axis inversion verified.");
}

// 2. Invariant: Radius scales with risk score (severe hazards toward perimeter)
{
  const center = 300;
  const maxR = 200;
  const lowRisk = projectPolarToCanvas(0.2, 45, center, center, maxR);
  const medRisk = projectPolarToCanvas(0.5, 45, center, center, maxR);
  const critRisk = projectPolarToCanvas(1.0, 45, center, center, maxR);

  assert(lowRisk.normalizedR < medRisk.normalizedR, "Low risk normalized R must be < Med risk");
  assert(medRisk.normalizedR < critRisk.normalizedR, "Med risk normalized R must be < Crit risk");
  assert(critRisk.normalizedR === 1.0, "Risk score 1.0 must equal perimeter normalized R (1.0)");
  assert(lowRisk.normalizedR >= 0.20, "Baseline normalized R must be >= 0.20");

  console.log("✓ Radius scaling with risk score verified.");
}

// 3. Invariant: Clean Node Omission Invariant (CR-SPEC-05 §2.B)
{
  const dummyFindings: FindingDTO[] = [
    {
      id: "f1",
      file: "clean.ts",
      line_start: 1,
      line_end: 1,
      rule_id: "NONE",
      category: "architecture",
      severity: "CLEAN",
      title: "Clean file",
      description: "Clean",
      radar_r: 0,
      radar_theta: 45,
    },
    {
      id: "f2",
      file: "clean_zero.ts",
      line_start: 1,
      line_end: 1,
      rule_id: "NONE",
      category: "architecture",
      severity: "LOW",
      title: "Clean with 0 risk",
      description: "Clean",
      radar_r: 0.0,
      radar_theta: 45,
    },
    {
      id: "f3",
      file: "vuln.ts",
      line_start: 10,
      line_end: 12,
      rule_id: "SEC-TOK-002",
      category: "security",
      severity: "CRITICAL",
      title: "Token leak",
      description: "Secret token leaked",
      radar_r: 0.95,
      radar_theta: 135,
    },
  ];

  const projected = projectFindings(dummyFindings, 400, 400);
  assert.strictEqual(projected.length, 1, "Only actionable findings (s > 0) must generate nodes");
  assert.strictEqual(projected[0].id, "f3", "Omission invariant preserved");

  console.log("✓ Clean node omission invariant verified.");
}

// 4. Invariant: Clustering activates above 150 nodes (PHASES.md & CR-SPEC-05 §2.D)
{
  const manyNodes = [];
  for (let i = 0; i < 180; i++) {
    manyNodes.push({
      type: "single" as const,
      id: `node-${i}`,
      finding: {
        id: `node-${i}`,
        file: `file_${i}.ts`,
        line_start: i,
        line_end: i,
        rule_id: "RULE",
        category: "security",
        severity: (i % 4 === 0 ? "CRITICAL" : i % 3 === 0 ? "HIGH" : "LOW") as any,
        title: `Finding ${i}`,
        description: `Desc ${i}`,
        radar_r: 0.5 + (i % 20) * 0.02,
        radar_theta: 130 + (i % 10),
      },
      x: 100 + (i % 8) * 3, // Very close spatially
      y: 100 + (i % 8) * 3,
      radius: 6,
      color: "#eab308",
      pulseHalo: false,
      angleDeg: 135,
      normalizedRadius: 0.6,
    });
  }

  // Under threshold (<= 150): no clustering
  const underThreshold = clusterRadarNodes(manyNodes.slice(0, 100), 150);
  assert.strictEqual(underThreshold.length, 100, "Below threshold must not cluster");

  // Over threshold (180 > 150): clustering activates!
  const clustered = clusterRadarNodes(manyNodes, 150, 20);
  assert(clustered.length < 180, "Above threshold must aggregate dense nodes");
  const clusters = clustered.filter((n) => n.type === "cluster");
  assert(clusters.length > 0, "Clusters must be created");
  assert(clusters[0].count > 1, "Cluster count must be > 1");

  console.log(`✓ Clustering above 150 nodes verified (180 nodes aggregated to ${clustered.length} visible targets).`);
}

// 5. Invariant: Hit testing
{
  const nodes = [
    {
      type: "single" as const,
      id: "hit-target",
      finding: { id: "hit-target" } as any,
      x: 250,
      y: 150,
      radius: 8,
      color: "#ef4444",
      pulseHalo: true,
      angleDeg: 45,
      normalizedRadius: 0.8,
    },
  ];

  const hit = hitTestRadar(252, 149, nodes);
  assert(hit !== null, "Pointer within hit radius must detect node");
  assert.strictEqual(hit?.id, "hit-target");

  const miss = hitTestRadar(10, 10, nodes);
  assert.strictEqual(miss, null, "Pointer far away must not hit");

  console.log("✓ Hit testing verified.");
}

console.log("All radar geometry tests passed successfully!");
