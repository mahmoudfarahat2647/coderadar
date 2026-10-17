/**
 * CodeRadar Studio — Polar Radar Canvas Geometry & Coordinate Transformation
 * Implements mathematical specifications defined in CR-SPEC-05 §2.
 */

import type { FindingDTO, Severity } from "../types";

export interface CanvasDimensions {
  width: number;
  height: number;
  pixelRatio: number;
}

export interface RadarPoint {
  x: number;
  y: number;
}

export interface ProjectedFindingNode {
  type: "single";
  id: string;
  finding: FindingDTO;
  x: number;
  y: number;
  radius: number;
  color: string;
  pulseHalo: boolean;
  angleDeg: number;
  normalizedRadius: number;
}

export interface ClusteredRadarNode {
  type: "cluster";
  id: string;
  findings: FindingDTO[];
  count: number;
  dominantSeverity: Severity;
  x: number;
  y: number;
  radius: number;
  color: string;
  pulseHalo: boolean;
  angleDeg: number;
  normalizedRadius: number;
}

export type RadarNode = ProjectedFindingNode | ClusteredRadarNode;

/**
 * Returns severity visual attributes according to CR-SPEC-05 §2.E.
 */
export function getSeverityStyle(severity: Severity): {
  color: string;
  radius: number;
  pulseHalo: boolean;
  weight: number;
} {
  switch (severity) {
    case "CRITICAL":
      return {
        color: "#ef4444", // Vivid Crimson
        radius: 8,
        pulseHalo: true,
        weight: 4,
      };
    case "HIGH":
      return {
        color: "#f97316", // Amber / Deep Orange
        radius: 7,
        pulseHalo: false,
        weight: 3,
      };
    case "MEDIUM":
      return {
        color: "#eab308", // Amber Gold
        radius: 6,
        pulseHalo: false,
        weight: 2,
      };
    case "LOW":
    default:
      return {
        color: "#38bdf8", // Sky Blue
        radius: 4,
        pulseHalo: false,
        weight: 1,
      };
  }
}

/**
 * Coordinate Transformation & Radius Geometry (CR-SPEC-05 §2.A)
 *
 * r = R_max * (0.20 + 0.80 * s^0.8)
 * x = x0 + r * cos(theta * pi / 180)
 * y = y0 - r * sin(theta * pi / 180)  [Canvas Y-axis inversion]
 */
export function projectPolarToCanvas(
  riskScore: number,
  thetaDeg: number,
  centerX: number,
  centerY: number,
  maxRadius: number
): RadarPoint & { normalizedR: number } {
  // Clamp risk score to (0.0, 1.0]
  const s = Math.max(0.01, Math.min(1.0, riskScore));
  const normalizedR = 0.20 + 0.80 * Math.pow(s, 0.8);
  const r = maxRadius * normalizedR;
  const radians = (thetaDeg * Math.PI) / 180;

  return {
    x: centerX + r * Math.cos(radians),
    y: centerY - r * Math.sin(radians),
    normalizedR,
  };
}

/**
 * Project a slice of findings onto the canvas coordinates.
 * Strictly adheres to the Clean Node Omission Invariant (CR-SPEC-05 §2.B):
 * Clean files (s = 0 or severity == CLEAN) NEVER emit canvas nodes.
 */
export function projectFindings(
  findings: FindingDTO[],
  width: number,
  height: number,
  padding = 24
): ProjectedFindingNode[] {
  const centerX = width / 2;
  const centerY = height / 2;
  const maxRadius = Math.max(10, Math.min(width, height) / 2 - padding);

  const projected: ProjectedFindingNode[] = [];

  for (const finding of findings) {
    // Clean Node Omission Invariant (CR-SPEC-05 §2.B)
    if (finding.severity === "CLEAN" || finding.radar_r <= 0) {
      continue;
    }

    const { x, y, normalizedR } = projectPolarToCanvas(
      finding.radar_r,
      finding.radar_theta,
      centerX,
      centerY,
      maxRadius
    );

    const style = getSeverityStyle(finding.severity);

    projected.push({
      type: "single",
      id: finding.id,
      finding,
      x,
      y,
      radius: style.radius,
      color: style.color,
      pulseHalo: style.pulseHalo,
      angleDeg: finding.radar_theta,
      normalizedRadius: normalizedR,
    });
  }

  return projected;
}

/**
 * Aggregates nodes when visible count exceeds threshold (CR-SPEC-05 §2.D / Phase 4 Gate).
 * Threshold default is 150 nodes. Clustering aggregates nodes within proximityRadius.
 */
export function clusterRadarNodes(
  nodes: ProjectedFindingNode[],
  threshold = 150,
  proximityRadius = 22
): RadarNode[] {
  if (nodes.length <= threshold) {
    return nodes;
  }

  const result: RadarNode[] = [];
  const visited = new Set<string>();

  for (let i = 0; i < nodes.length; i++) {
    const node = nodes[i];
    if (visited.has(node.id)) continue;

    // Look for neighbors within proximityRadius
    const cluster: ProjectedFindingNode[] = [node];
    visited.add(node.id);

    for (let j = i + 1; j < nodes.length; j++) {
      const other = nodes[j];
      if (visited.has(other.id)) continue;

      const dx = node.x - other.x;
      const dy = node.y - other.y;
      const dist = Math.sqrt(dx * dx + dy * dy);

      if (dist <= proximityRadius) {
        cluster.push(other);
        visited.add(other.id);
      }
    }

    if (cluster.length === 1) {
      result.push(node);
    } else {
      // Create aggregate cluster node
      let dominantWeight = -1;
      let dominantSeverity: Severity = "LOW";
      let sumX = 0;
      let sumY = 0;
      let sumTheta = 0;
      let sumR = 0;

      for (const item of cluster) {
        const style = getSeverityStyle(item.finding.severity);
        if (style.weight > dominantWeight) {
          dominantWeight = style.weight;
          dominantSeverity = item.finding.severity;
        }
        sumX += item.x;
        sumY += item.y;
        sumTheta += item.angleDeg;
        sumR += item.normalizedRadius;
      }

      const style = getSeverityStyle(dominantSeverity);
      const count = cluster.length;
      // Slightly larger badge radius for cluster
      const clusterRadius = Math.min(16, style.radius + Math.min(6, Math.log2(count) * 2));

      result.push({
        type: "cluster",
        id: `cluster-${cluster.map((c) => c.id).join("_").slice(0, 40)}`,
        findings: cluster.map((c) => c.finding),
        count,
        dominantSeverity,
        x: sumX / count,
        y: sumY / count,
        radius: clusterRadius,
        color: style.color,
        pulseHalo: style.pulseHalo,
        angleDeg: sumTheta / count,
        normalizedRadius: sumR / count,
      });
    }
  }

  return result;
}

/**
 * Hit test a mouse coordinate against projected nodes or clusters.
 */
export function hitTestRadar(
  mouseX: number,
  mouseY: number,
  nodes: RadarNode[],
  extraTolerance = 8
): RadarNode | null {
  let closest: RadarNode | null = null;
  let minDistance = Infinity;

  for (const node of nodes) {
    const dx = mouseX - node.x;
    const dy = mouseY - node.y;
    const dist = Math.sqrt(dx * dx + dy * dy);
    const hitRadius = node.radius + extraTolerance;

    if (dist <= hitRadius && dist < minDistance) {
      minDistance = dist;
      closest = node;
    }
  }

  return closest;
}
