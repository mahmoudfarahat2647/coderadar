import React, { useCallback, useEffect, useRef, useState } from "react";
import {
  FindingDTO,
} from "../types";
import {
  clusterRadarNodes,
  hitTestRadar,
  projectFindings,
  RadarNode,
} from "../utils/radarGeometry";
import {
  Play,
  Pause,
  Maximize2,
  Minimize2,
  Shield,
  Layers,
  Activity,
  Crosshair,
} from "lucide-react";

interface PolarRadarCanvasProps {
  findings: FindingDTO[];
  hoveredFindingId: string | null;
  selectedFinding: FindingDTO | null;
  onHoverFinding: (id: string | null) => void;
  onSelectFinding: (finding: FindingDTO) => void;
  cleanPassedPercent?: number | null;
  isScanning?: boolean;
}

export const PolarRadarCanvas: React.FC<PolarRadarCanvasProps> = ({
  findings,
  hoveredFindingId,
  selectedFinding,
  onHoverFinding,
  onSelectFinding,
  cleanPassedPercent = null,
  isScanning = false,
}) => {
  const containerRef = useRef<HTMLDivElement>(null);
  const nodesCanvasRef = useRef<HTMLCanvasElement>(null);
  const sweepCanvasRef = useRef<HTMLCanvasElement>(null);

  // Animation & Decoupled State (CR-SPEC-05 §2.D: ZERO React state in animation tick)
  const needleAngleRef = useRef<number>(90);
  const lastTimestampRef = useRef<number>(0);
  const animationFrameIdRef = useRef<number | null>(null);
  const renderedNodesRef = useRef<RadarNode[]>([]);
  const dimensionsRef = useRef<{ width: number; height: number; dpr: number }>({
    width: 480,
    height: 480,
    dpr: window.devicePixelRatio || 1,
  });

  // UI Controls & Settings
  const [isPaused, setIsPaused] = useState<boolean>(false);
  const [prefersReducedMotion, setPrefersReducedMotion] = useState<boolean>(false);
  const [isExpanded, setIsExpanded] = useState<boolean>(false);
  const [fps, setFps] = useState<number>(60);
  const [hoveredNode, setHoveredNode] = useState<RadarNode | null>(null);
  const [mousePos, setMousePos] = useState<{ x: number; y: number } | null>(null);

  const fpsFramesCountRef = useRef<number>(0);
  const fpsLastTimeRef = useRef<number>(performance.now());

  // Check prefers-reduced-motion
  useEffect(() => {
    const mediaQuery = window.matchMedia("(prefers-reduced-motion: reduce)");
    setPrefersReducedMotion(mediaQuery.matches);

    const listener = (e: MediaQueryListEvent) => setPrefersReducedMotion(e.matches);
    mediaQuery.addEventListener("change", listener);
    return () => mediaQuery.removeEventListener("change", listener);
  }, []);

  // Update Canvas Backing Buffers according to layout & devicePixelRatio (CR-SPEC-05 §2.D)
  const updateCanvasDimensions = useCallback(() => {
    if (!containerRef.current || !nodesCanvasRef.current || !sweepCanvasRef.current) return;

    const rect = containerRef.current.getBoundingClientRect();
    const width = Math.max(260, Math.floor(rect.width));
    const height = Math.max(260, Math.floor(rect.height));
    const dpr = window.devicePixelRatio || 1;

    dimensionsRef.current = { width, height, dpr };

    const nodesCanvas = nodesCanvasRef.current;
    const sweepCanvas = sweepCanvasRef.current;

    nodesCanvas.width = Math.floor(width * dpr);
    nodesCanvas.height = Math.floor(height * dpr);
    nodesCanvas.style.width = `${width}px`;
    nodesCanvas.style.height = `${height}px`;

    sweepCanvas.width = Math.floor(width * dpr);
    sweepCanvas.height = Math.floor(height * dpr);
    sweepCanvas.style.width = `${width}px`;
    sweepCanvas.style.height = `${height}px`;
  }, []);

  // Draw Background Layer (Nodes, Grid Rings, Sector Dividers, Clean Aura)
  const renderBackgroundLayer = useCallback(() => {
    const canvas = nodesCanvasRef.current;
    if (!canvas) return;
    const ctx = canvas.getContext("2d");
    if (!ctx) return;

    const { width, height, dpr } = dimensionsRef.current;

    ctx.save();
    ctx.scale(dpr, dpr);
    ctx.clearRect(0, 0, width, height);

    const centerX = width / 2;
    const centerY = height / 2;
    const padding = 32;
    const maxRadius = Math.max(20, Math.min(width, height) / 2 - padding);

    // 1. Radar Disc Gradient Background
    const bgGradient = ctx.createRadialGradient(
      centerX,
      centerY,
      0,
      centerX,
      centerY,
      maxRadius
    );
    bgGradient.addColorStop(0, "#12161f");
    bgGradient.addColorStop(0.7, "#0c0f14");
    bgGradient.addColorStop(1, "#07090c");

    ctx.fillStyle = bgGradient;
    ctx.beginPath();
    ctx.arc(centerX, centerY, maxRadius, 0, Math.PI * 2);
    ctx.fill();

    // 2. Clean Health Ambient Emerald Pulse (CR-SPEC-05 §2.B)
    if (cleanPassedPercent != null && cleanPassedPercent >= 85) {
      const cleanGlow = ctx.createRadialGradient(
        centerX,
        centerY,
        maxRadius * 0.2,
        centerX,
        centerY,
        maxRadius
      );
      cleanGlow.addColorStop(0, "rgba(16, 185, 129, 0.05)");
      cleanGlow.addColorStop(0.8, "rgba(16, 185, 129, 0.02)");
      cleanGlow.addColorStop(1, "rgba(16, 185, 129, 0.0)");

      ctx.fillStyle = cleanGlow;
      ctx.beginPath();
      ctx.arc(centerX, centerY, maxRadius, 0, Math.PI * 2);
      ctx.fill();
    }

    // 3. Concentric Range Rings (20% baseline, 45% Low, 70% Med, 88% High, 100% Perimeter)
    const rangeRings = [
      { r: 0.2, label: "SAFE", dash: [2, 4], color: "rgba(255, 255, 255, 0.08)" },
      { r: 0.45, label: "LOW", dash: [1, 5], color: "rgba(56, 189, 248, 0.15)" },
      { r: 0.7, label: "MED", dash: [2, 4], color: "rgba(234, 179, 8, 0.15)" },
      { r: 0.88, label: "HIGH", dash: [2, 4], color: "rgba(249, 115, 22, 0.18)" },
      { r: 1.0, label: "CRIT", dash: [], color: "rgba(239, 68, 68, 0.35)" },
    ];

    rangeRings.forEach((ring) => {
      const radius = maxRadius * ring.r;
      ctx.beginPath();
      ctx.setLineDash(ring.dash);
      ctx.strokeStyle = ring.color;
      ctx.lineWidth = ring.r === 1.0 ? 1.5 : 1.0;
      ctx.arc(centerX, centerY, radius, 0, Math.PI * 2);
      ctx.stroke();

      // Range text watermark
      if (width > 340 && ring.label !== "SAFE") {
        ctx.setLineDash([]);
        ctx.fillStyle = ring.color;
        ctx.font = "8px JetBrains Mono, monospace";
        ctx.textAlign = "center";
        ctx.textBaseline = "bottom";
        ctx.fillText(ring.label, centerX, centerY - radius + 11);
      }
    });

    ctx.setLineDash([]);

    // 4. Quadrant Axes (0°, 90°, 180°, 270°) and Sector Rays (45°, 135°, 225°, 315°)
    const degreeAxes = [
      { deg: 0, label: "0°", primary: true },
      { deg: 45, label: "45°", primary: false },
      { deg: 90, label: "90°", primary: true },
      { deg: 135, label: "135°", primary: false },
      { deg: 180, label: "180°", primary: true },
      { deg: 225, label: "225°", primary: false },
      { deg: 270, label: "270°", primary: true },
      { deg: 315, label: "315°", primary: false },
    ];

    degreeAxes.forEach((axis) => {
      const rad = (axis.deg * Math.PI) / 180;
      const xEnd = centerX + maxRadius * Math.cos(rad);
      const yEnd = centerY - maxRadius * Math.sin(rad);

      ctx.beginPath();
      ctx.strokeStyle = axis.primary
        ? "rgba(255, 255, 255, 0.14)"
        : "rgba(255, 255, 255, 0.05)";
      ctx.lineWidth = axis.primary ? 1.0 : 0.8;
      if (!axis.primary) ctx.setLineDash([2, 4]);
      else ctx.setLineDash([]);

      ctx.moveTo(centerX, centerY);
      ctx.lineTo(xEnd, yEnd);
      ctx.stroke();

      // Outer degree ticks
      if (width > 360) {
        ctx.setLineDash([]);
        const tickDist = maxRadius + 14;
        const xTick = centerX + tickDist * Math.cos(rad);
        const yTick = centerY - tickDist * Math.sin(rad);

        ctx.font = "8px JetBrains Mono, monospace";
        ctx.fillStyle = "rgba(148, 163, 184, 0.6)";
        ctx.textAlign = "center";
        ctx.textBaseline = "middle";
        ctx.fillText(axis.label, xTick, yTick);
      }
    });

    ctx.setLineDash([]);

    // 5. Quadrant Pillar Labels (CR-SPEC-05 §2.C)
    if (width > 320) {
      const quadrantPillars = [
        {
          name: "ARCHITECTURE",
          deg: 45,
          color: "#78da9e",
          align: "left" as const,
        },
        {
          name: "SECURITY",
          deg: 135,
          color: "#ef4444",
          align: "right" as const,
        },
        {
          name: "PERFORMANCE",
          deg: 225,
          color: "#f59e0b",
          align: "right" as const,
        },
        {
          name: "ROBUSTNESS",
          deg: 315,
          color: "#38bdf8",
          align: "left" as const,
        },
      ];

      quadrantPillars.forEach((q) => {
        const rad = (q.deg * Math.PI) / 180;
        const labelDist = maxRadius * 0.65;
        const lx = centerX + labelDist * Math.cos(rad);
        const ly = centerY - labelDist * Math.sin(rad);

        ctx.save();
        ctx.font = "bold 9px JetBrains Mono, monospace";
        ctx.fillStyle = `${q.color}44`;
        ctx.textAlign = "center";
        ctx.textBaseline = "middle";
        ctx.fillText(q.name, lx, ly);
        ctx.restore();
      });
    }

    // 6. Project & Cluster Nodes (CR-SPEC-05 §2.A, §2.B, §2.D)
    // Clean nodes are automatically omitted by projectFindings (s = 0 never drawn)
    const projected = projectFindings(findings, width, height, padding);
    const nodes = clusterRadarNodes(projected, 150, 20);
    renderedNodesRef.current = nodes;

    // Draw Nodes
    nodes.forEach((node) => {
      const isTargeted =
        (node.type === "single" &&
          (node.id === hoveredFindingId || node.id === selectedFinding?.id)) ||
        (node.type === "cluster" &&
          node.findings.some(
            (f) => f.id === hoveredFindingId || f.id === selectedFinding?.id
          ));

      ctx.save();

      if (node.type === "single") {
        // Critical pulsator halo (CR-SPEC-05 §2.E)
        if (node.pulseHalo) {
          ctx.beginPath();
          ctx.arc(node.x, node.y, node.radius + 5, 0, Math.PI * 2);
          ctx.fillStyle = "rgba(239, 68, 68, 0.22)";
          ctx.fill();
        }

        // Target reticle and connecting beam on Hover or Selection
        if (isTargeted) {
          // Vector beam from center (CR-SPEC-05 §3)
          ctx.beginPath();
          ctx.setLineDash([3, 3]);
          ctx.strokeStyle = "rgba(56, 189, 248, 0.75)";
          ctx.lineWidth = 1.2;
          ctx.moveTo(centerX, centerY);
          ctx.lineTo(node.x, node.y);
          ctx.stroke();
          ctx.setLineDash([]);

          // Glowing Target Reticle
          ctx.beginPath();
          ctx.arc(node.x, node.y, node.radius + 6, 0, Math.PI * 2);
          ctx.strokeStyle = "#38bdf8";
          ctx.lineWidth = 1.8;
          ctx.stroke();

          // Coordinate readout
          ctx.font = "9px JetBrains Mono, monospace";
          ctx.fillStyle = "#38bdf8";
          ctx.textAlign = "center";
          ctx.fillText(
            `r:${node.normalizedRadius.toFixed(2)} θ:${Math.round(node.angleDeg)}°`,
            node.x,
            node.y - node.radius - 8
          );
        }

        // Main Node Body
        ctx.beginPath();
        ctx.arc(node.x, node.y, isTargeted ? node.radius + 1.5 : node.radius, 0, Math.PI * 2);
        ctx.fillStyle = node.color;
        ctx.shadowColor = node.color;
        ctx.shadowBlur = node.pulseHalo ? 14 : 8;
        ctx.fill();

        // High-contrast border
        ctx.beginPath();
        ctx.arc(node.x, node.y, isTargeted ? node.radius + 1.5 : node.radius, 0, Math.PI * 2);
        ctx.strokeStyle = "rgba(255, 255, 255, 0.85)";
        ctx.lineWidth = 1.2;
        ctx.stroke();
      } else {
        // Cluster Node (CR-SPEC-05 §2.D / Phase 4 Gate: Clustering above 150 nodes)
        ctx.beginPath();
        ctx.arc(node.x, node.y, node.radius + 4, 0, Math.PI * 2);
        ctx.fillStyle = "rgba(255, 255, 255, 0.1)";
        ctx.fill();

        ctx.beginPath();
        ctx.arc(node.x, node.y, node.radius, 0, Math.PI * 2);
        ctx.fillStyle = node.color;
        ctx.shadowColor = node.color;
        ctx.shadowBlur = 10;
        ctx.fill();

        ctx.strokeStyle = "#ffffff";
        ctx.lineWidth = 1.5;
        ctx.stroke();

        // Cluster Count Badge
        ctx.font = "bold 9px JetBrains Mono, monospace";
        ctx.fillStyle = "#ffffff";
        ctx.textAlign = "center";
        ctx.textBaseline = "middle";
        ctx.shadowBlur = 0;
        ctx.fillText(String(node.count), node.x, node.y);
      }

      ctx.restore();
    });

    ctx.restore();
  }, [
    findings,
    hoveredFindingId,
    selectedFinding,
    cleanPassedPercent,
  ]);

  // Foreground Sweeping Needle Animation Loop (CR-SPEC-05 §2.D)
  // Maintains 60 FPS by mutating canvas context directly without React reconciliation
  useEffect(() => {
    let active = true;

    const renderSweepFrame = (timestamp: number) => {
      if (!active) return;

      if (!lastTimestampRef.current) {
        lastTimestampRef.current = timestamp;
      }
      const deltaMs = timestamp - lastTimestampRef.current;
      lastTimestampRef.current = timestamp;

      // Real-time FPS Calculation
      fpsFramesCountRef.current++;
      const now = performance.now();
      if (now - fpsLastTimeRef.current >= 500) {
        const measuredFps = Math.round(
          (fpsFramesCountRef.current * 1000) / (now - fpsLastTimeRef.current)
        );
        setFps(measuredFps);
        fpsFramesCountRef.current = 0;
        fpsLastTimeRef.current = now;
      }

      const sweepCanvas = sweepCanvasRef.current;
      if (sweepCanvas) {
        const ctx = sweepCanvas.getContext("2d");
        if (ctx) {
          const { width, height, dpr } = dimensionsRef.current;
          ctx.save();
          ctx.scale(dpr, dpr);
          ctx.clearRect(0, 0, width, height);

          const centerX = width / 2;
          const centerY = height / 2;
          const padding = 32;
          const maxRadius = Math.max(20, Math.min(width, height) / 2 - padding);

          if (!isPaused && !prefersReducedMotion) {
            // omega = 72 deg/sec (full 360° in 5.0 seconds — CR-SPEC-05 §2.D)
            const speedDegPerSec = 72;
            needleAngleRef.current =
              (needleAngleRef.current + (speedDegPerSec * deltaMs) / 1000) % 360;
          }

          const currentAngle = needleAngleRef.current;
          const needleRad = (currentAngle * Math.PI) / 180;

          // 1. Phosphorescent Decay Trail (fading over 30° arc behind needle)
          const trailArcDeg = 30;
          const steps = 18;
          for (let i = 0; i < steps; i++) {
            const stepAngle = currentAngle - (i / steps) * trailArcDeg;
            const a1 = (stepAngle * Math.PI) / 180;
            const a2 = ((stepAngle - trailArcDeg / steps) * Math.PI) / 180;
            const alpha = 0.16 * Math.pow(1 - i / steps, 1.8);

            ctx.beginPath();
            ctx.moveTo(centerX, centerY);
            ctx.arc(centerX, centerY, maxRadius, -a1, -a2, false);
            ctx.closePath();
            ctx.fillStyle = `rgba(234, 179, 8, ${alpha})`;
            ctx.fill();
          }

          // 2. Crisp Needle Beam Line
          const nx = centerX + maxRadius * Math.cos(needleRad);
          const ny = centerY - maxRadius * Math.sin(needleRad);

          ctx.beginPath();
          ctx.moveTo(centerX, centerY);
          ctx.lineTo(nx, ny);
          ctx.strokeStyle = "#eab308";
          ctx.lineWidth = 1.5;
          ctx.shadowColor = "#eab308";
          ctx.shadowBlur = 10;
          ctx.stroke();

          // 3. Central Pivot Center Dot
          ctx.beginPath();
          ctx.arc(centerX, centerY, 3.5, 0, Math.PI * 2);
          ctx.fillStyle = "#eab308";
          ctx.fill();

          // 4. Node Sweep Blip Flare (Phosphorescent flare when needle sweeps past finding)
          renderedNodesRef.current.forEach((node) => {
            let diff = (currentAngle - node.angleDeg) % 360;
            if (diff < 0) diff += 360;

            if (diff <= 18) {
              const blipAlpha = (1 - diff / 18) * 0.7;
              ctx.beginPath();
              ctx.arc(node.x, node.y, node.radius + 6, 0, Math.PI * 2);
              ctx.fillStyle = `rgba(234, 179, 8, ${blipAlpha})`;
              ctx.shadowColor = "#eab308";
              ctx.shadowBlur = 12;
              ctx.fill();
            }
          });

          ctx.restore();
        }
      }

      animationFrameIdRef.current = requestAnimationFrame(renderSweepFrame);
    };

    animationFrameIdRef.current = requestAnimationFrame(renderSweepFrame);

    return () => {
      active = false;
      if (animationFrameIdRef.current !== null) {
        cancelAnimationFrame(animationFrameIdRef.current);
      }
    };
  }, [isPaused, prefersReducedMotion]);

  // Re-draw background on resize or findings change
  useEffect(() => {
    updateCanvasDimensions();
    renderBackgroundLayer();

    const handleResize = () => {
      updateCanvasDimensions();
      renderBackgroundLayer();
    };

    window.addEventListener("resize", handleResize);
    return () => window.removeEventListener("resize", handleResize);
  }, [updateCanvasDimensions, renderBackgroundLayer]);

  // Mouse Interactions (Hit testing, Tooltip, Click)
  const handleMouseMove = (e: React.MouseEvent<HTMLDivElement>) => {
    if (!containerRef.current) return;
    const rect = containerRef.current.getBoundingClientRect();
    const x = e.clientX - rect.left;
    const y = e.clientY - rect.top;
    setMousePos({ x: e.clientX, y: e.clientY });

    const hit = hitTestRadar(x, y, renderedNodesRef.current, 8);
    if (hit) {
      setHoveredNode(hit);
      if (hit.type === "single") {
        onHoverFinding(hit.id);
      } else if (hit.findings.length > 0) {
        onHoverFinding(hit.findings[0].id);
      }
    } else {
      if (hoveredNode) {
        setHoveredNode(null);
        onHoverFinding(null);
      }
    }
  };

  const handleMouseLeave = () => {
    setHoveredNode(null);
    setMousePos(null);
    onHoverFinding(null);
  };

  const handleClick = (e: React.MouseEvent<HTMLDivElement>) => {
    if (!containerRef.current) return;
    const rect = containerRef.current.getBoundingClientRect();
    const x = e.clientX - rect.left;
    const y = e.clientY - rect.top;

    const hit = hitTestRadar(x, y, renderedNodesRef.current, 8);
    if (hit) {
      if (hit.type === "single") {
        onSelectFinding(hit.finding);
      } else if (hit.findings.length > 0) {
        onSelectFinding(hit.findings[0]);
      }
    }
  };

  const visibleCount = findings.filter((f) => f.severity !== "CLEAN" && f.radar_r > 0).length;
  const isClustered = visibleCount > 150;

  return (
    <div
      ref={containerRef}
      onMouseMove={handleMouseMove}
      onMouseLeave={handleMouseLeave}
      onClick={handleClick}
      className={`relative flex items-center justify-center bg-surface-canvas overflow-hidden select-none border-r border-border-subtle group transition-all duration-300 ${
        isExpanded ? "w-full h-full" : "w-full h-full min-h-[380px]"
      } ${hoveredNode ? "cursor-pointer" : "cursor-crosshair"}`}
      title="CodeRadar 360° Polar Canvas (CR-SPEC-05)"
    >
      {/* Background Static Layer */}
      <canvas
        ref={nodesCanvasRef}
        id="radar-nodes"
        className="absolute inset-0 pointer-events-none"
      />

      {/* Foreground Animated Sweeping Needle Layer */}
      <canvas
        ref={sweepCanvasRef}
        id="radar-sweep"
        className="absolute inset-0 pointer-events-none"
      />

      {/* Top Left: Real-time Telemetry HUD Ribbon */}
      <div className="absolute top-3 left-3 flex items-center space-x-2 z-10">
        <div className="flex items-center space-x-1.5 px-2.5 py-1 rounded bg-surface-panel/90 backdrop-blur border border-border-subtle text-[11px] font-mono text-text-secondary">
          <Activity className="w-3.5 h-3.5 text-secondary" />
          <span className="font-semibold text-text-primary">{fps} FPS</span>
          <span className="text-text-muted">|</span>
          <span>72°/s</span>
        </div>

        {isClustered && (
          <div className="flex items-center space-x-1.5 px-2.5 py-1 rounded bg-tertiary/15 border border-tertiary/30 text-[10px] font-mono text-tertiary">
            <Layers className="w-3 h-3" />
            <span>Clustering Active (&gt;150)</span>
          </div>
        )}

        {isScanning && (
          <div className="flex items-center space-x-1.5 px-2 py-0.5 rounded bg-primary/20 border border-primary/40 text-[10px] font-mono text-secondary animate-pulse">
            <span>SWEEPING</span>
          </div>
        )}
      </div>

      {/* Top Right: Radar HUD Controls */}
      <div className="absolute top-3 right-3 flex items-center space-x-1.5 z-10">
        <button
          onClick={(e) => {
            e.stopPropagation();
            setIsPaused(!isPaused);
          }}
          className="p-1.5 rounded bg-surface-panel/90 hover:bg-surface-elevated border border-border-subtle text-text-muted hover:text-text-primary transition-colors"
          title={isPaused ? "Resume Sweep Animation" : "Pause Sweep"}
        >
          {isPaused ? <Play className="w-3.5 h-3.5" /> : <Pause className="w-3.5 h-3.5" />}
        </button>

        <button
          onClick={(e) => {
            e.stopPropagation();
            setIsExpanded(!isExpanded);
          }}
          className="p-1.5 rounded bg-surface-panel/90 hover:bg-surface-elevated border border-border-subtle text-text-muted hover:text-text-primary transition-colors"
          title={isExpanded ? "Collapse View" : "Expand Radar Theater"}
        >
          {isExpanded ? <Minimize2 className="w-3.5 h-3.5" /> : <Maximize2 className="w-3.5 h-3.5" />}
        </button>
      </div>

      {/* Bottom Center: Telemetry Status Badge */}
      <div className="absolute bottom-3 inset-x-0 flex justify-center items-center pointer-events-none z-10">
        <div className="flex items-center space-x-3 px-3 py-1 rounded-full bg-surface-panel/85 backdrop-blur border border-border-subtle text-[11px] font-mono">
          <div className="flex items-center space-x-1 text-status-clean">
            <Shield className="w-3 h-3" />
            <span>{cleanPassedPercent == null ? "Score unavailable" : `${cleanPassedPercent.toFixed(1)}% of analyzed files without findings`}</span>
          </div>
          <span className="text-text-muted">·</span>
          <div className="flex items-center space-x-1 text-text-primary">
            <Crosshair className="w-3 h-3 text-secondary" />
            <span>{visibleCount} Active Nodes</span>
          </div>
          <span className="text-text-muted">·</span>
          <span className="text-text-muted">360° Polar Scan</span>
        </div>
      </div>

      {/* Floating Interactive Hover Tooltip */}
      {hoveredNode && mousePos && (
        <div
          className="fixed z-50 pointer-events-none transform -translate-x-1/2 -translate-y-full mb-3 p-2.5 rounded-lg bg-surface-elevated/95 backdrop-blur-md border border-border-strong shadow-2xl text-xs max-w-xs animate-in fade-in zoom-in-95 duration-100"
          style={{
            left: `${mousePos.x}px`,
            top: `${mousePos.y - 10}px`,
          }}
        >
          {hoveredNode.type === "single" ? (
            <div className="space-y-1.5">
              <div className="flex items-center justify-between gap-2">
                <span
                  className="px-1.5 py-0.5 rounded text-[10px] font-mono font-semibold uppercase"
                  style={{
                    backgroundColor: `${hoveredNode.color}20`,
                    color: hoveredNode.color,
                  }}
                >
                  {hoveredNode.finding.severity}
                </span>
                <span className="text-[10px] font-mono text-text-muted">
                  {hoveredNode.finding.rule_id}
                </span>
              </div>
              <div className="font-mono text-[11px] font-semibold text-text-primary truncate">
                {hoveredNode.finding.file}:{hoveredNode.finding.line_start}
              </div>
              <div className="text-[11px] text-text-secondary line-clamp-2">
                {hoveredNode.finding.title}
              </div>
              <div className="text-[9px] font-mono text-secondary pt-0.5 flex justify-between">
                <span>Click to inspect code diff</span>
                <span>r:{hoveredNode.normalizedRadius.toFixed(2)}</span>
              </div>
            </div>
          ) : (
            <div className="space-y-1">
              <div className="flex items-center justify-between">
                <span className="font-semibold text-text-primary">
                  Cluster ({hoveredNode.count} findings)
                </span>
                <span
                  className="px-1.5 py-0.5 rounded text-[10px] font-mono uppercase"
                  style={{
                    backgroundColor: `${hoveredNode.color}20`,
                    color: hoveredNode.color,
                  }}
                >
                  {hoveredNode.dominantSeverity}
                </span>
              </div>
              <p className="text-[11px] text-text-muted">
                Grouped proximate threats to maintain 60 FPS clarity.
              </p>
              <div className="text-[9px] font-mono text-secondary">
                Click to inspect leading finding
              </div>
            </div>
          )}
        </div>
      )}
    </div>
  );
};
