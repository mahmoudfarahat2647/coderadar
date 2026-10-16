---
name: Supabase Dark Infrastructure
colors:
  surface: '#131313'
  surface-dim: '#131313'
  surface-bright: '#393939'
  surface-container-lowest: '#0e0e0e'
  surface-container-low: '#1c1b1b'
  surface-container: '#201f1f'
  surface-container-high: '#2a2a2a'
  surface-container-highest: '#353534'
  on-surface: '#e5e2e1'
  on-surface-variant: '#becabe'
  inverse-surface: '#e5e2e1'
  inverse-on-surface: '#313030'
  outline: '#889489'
  outline-variant: '#3e4941'
  surface-tint: '#78da9e'
  primary: '#78da9e'
  on-primary: '#00391e'
  primary-container: '#3fa26c'
  on-primary-container: '#00311a'
  inverse-primary: '#006d3f'
  secondary: '#51df9c'
  on-secondary: '#003822'
  secondary-container: '#00b173'
  on-secondary-container: '#003b24'
  tertiary: '#4cd6fb'
  on-tertiary: '#003642'
  tertiary-container: '#00829d'
  on-tertiary-container: '#000609'
  error: '#ffb4ab'
  on-error: '#690005'
  error-container: '#93000a'
  on-error-container: '#ffdad6'
  primary-fixed: '#94f7b9'
  primary-fixed-dim: '#78da9e'
  on-primary-fixed: '#002110'
  on-primary-fixed-variant: '#00522e'
  secondary-fixed: '#71fcb6'
  secondary-fixed-dim: '#51df9c'
  on-secondary-fixed: '#002112'
  on-secondary-fixed-variant: '#005233'
  tertiary-fixed: '#b3ebff'
  tertiary-fixed-dim: '#4cd6fb'
  on-tertiary-fixed: '#001f27'
  on-tertiary-fixed-variant: '#004e5f'
  background: '#131313'
  on-background: '#e5e2e1'
  surface-variant: '#353534'
  status-critical: '#EF4444'
  status-high: '#F97316'
  status-medium: '#F59E0B'
  status-low: '#06B6D4'
  surface-canvas: '#121212'
  surface-panel: '#181818'
  surface-elevated: '#222222'
  surface-overlay: '#2A2A2A'
  border-subtle: '#282828'
  border-strong: '#383838'
typography:
  headline-xl:
    fontFamily: Manrope
    fontSize: 2rem
    fontWeight: '700'
    lineHeight: 2.5rem
    letterSpacing: -0.025em
  headline-lg:
    fontFamily: Manrope
    fontSize: 1.5rem
    fontWeight: '600'
    lineHeight: 2rem
    letterSpacing: -0.02em
  headline-md:
    fontFamily: Manrope
    fontSize: 1.25rem
    fontWeight: '600'
    lineHeight: 1.75rem
    letterSpacing: -0.015em
  headline-sm:
    fontFamily: Inter
    fontSize: 1rem
    fontWeight: '600'
    lineHeight: 1.5rem
    letterSpacing: -0.01em
  body-lg:
    fontFamily: Inter
    fontSize: 1rem
    fontWeight: '400'
    lineHeight: 1.5rem
  body-md:
    fontFamily: Inter
    fontSize: 0.875rem
    fontWeight: '400'
    lineHeight: 1.375rem
  body-sm:
    fontFamily: Inter
    fontSize: 0.75rem
    fontWeight: '400'
    lineHeight: 1.125rem
  code-md:
    fontFamily: JetBrains Mono
    fontSize: 0.875rem
    fontWeight: '400'
    lineHeight: 1.375rem
  code-sm:
    fontFamily: JetBrains Mono
    fontSize: 0.75rem
    fontWeight: '500'
    lineHeight: 1.125rem
  label-md:
    fontFamily: Inter
    fontSize: 0.75rem
    fontWeight: '600'
    lineHeight: 1rem
    letterSpacing: 0.03em
  label-sm:
    fontFamily: Inter
    fontSize: 0.6875rem
    fontWeight: '600'
    lineHeight: 0.875rem
    letterSpacing: 0.04em
rounded:
  sm: 0.125rem
  DEFAULT: 0.25rem
  md: 0.375rem
  lg: 0.5rem
  xl: 0.75rem
  full: 9999px
spacing:
  gutter: 1rem
  gutter-compact: 0.5rem
  margin: 1.5rem
  margin-compact: 1rem
  space-xs: 0.25rem
  space-sm: 0.5rem
  space-md: 0.75rem
  space-lg: 1.25rem
  space-xl: 2rem
---

## Brand & Style

This design system is engineered for dense developer environments, real-time database tooling, and complex cloud infrastructure management. It balances industrial technical rigor with a modern, razor-sharp digital aesthetic.

The personality is authoritative, restrained, and meticulously organized. The visual language conveys immediate speed, deep utility, and high reliability. The design movement blends **technical minimalism** with subtle **surface layering**:
- True and near-black backdrops to maximize visual contrast without eye strain during long operational sessions.
- Crisp, 1px low-contrast borders defining dense information grids rather than loud shadows.
- Vibrant, unmistakable semantic indicators for rapid triage of infrastructure health, deployment status, and database logs.
- Strict typographical hierarchy utilizing clean geometric sans for controls and monospace accents for SQL, logs, and system metrics.

## Colors

The color palette centers on functional utility within a dark workspace:

- **Primary (`#1E8854`)**: The functional grounding green used for primary triggers, connected state markers, and focused interactive states.
- **Secondary (`#3ECF8E`)**: The vivid Supabase-inspired emerald highlight for active highlights, checkmarks, success indicators, and high-visibility CTAs.
- **Tertiary (`#00B4D8`)**: A technical cyan used for networking layers, query metrics, telemetry data, and system-level metadata.
- **Neutral (`#121212`)**: Deep slate-black canvas foundation, supporting an intentional hierarchy of elevation tokens:
  - `surface-canvas` (`#121212`): Application background.
  - `surface-panel` (`#181818`): Sidebar, toolbars, and primary grid containers.
  - `surface-elevated` (`#222222`): Cards, table rows, and interactive surfaces.
  - `surface-overlay` (`#2A2A2A`): Dropdowns, modals, and tooltips.

### Semantic Status Tokens
- **Critical / Red (`#EF4444`)**: Production downtime, failed queries, deletion flows.
- **High / Orange (`#F97316`)**: Schema migration conflicts, resource thresholds above 90%.
- **Medium / Amber (`#F59E0B`)**: Deprecation alerts, high memory consumption, idle instances.
- **Low / Blue-Cyan (`#06B6D4`)**: General telemetry updates, informational background jobs, read replica syncs.

## Typography

Typography balances rapid scanning and high-density legibility:

- **Manrope**: Reserved for high-level section anchors, major infrastructure headings, and metric counters where modern geometric clarity adds distinction.
- **Inter**: The workhorse interface font for all body copy, table grids, configuration labels, and micro-copy, providing maximum legibility at compact scales.
- **JetBrains Mono**: Used for all code blocks, query statements, project IDs, endpoint paths, database connection strings, and status values.

Numerical tabular figures (`font-variant-numeric: tabular-nums`) must be enforced across all data tables, telemetry charts, and latency metrics to prevent horizontal jitter during live telemetry streams.

## Layout & Spacing

This design system uses a strict 4px grid optimized for high-density information displays.

- **Dashboard Layout**: Fixed-width collapsible sidebar (`240px` wide, collapsable to `56px`), coupled with a fluid main container that expands to `100%` viewport width up to a `1600px` max boundary.
- **Table and Data Densities**:
  - Compact: Table rows configured at `36px` height with `space-sm` (`0.5rem`) vertical padding.
  - Standard: Table rows at `44px` height with `space-md` (`0.75rem`) vertical padding.
- **Responsive Adaptations**:
  - **Desktop (â‰¥ 1200px)**: Multi-pane workflows (e.g., table schema viewer + SQL console side-by-side).
  - **Tablet (768px - 1199px)**: Split panes stack vertically; secondary inspectors collapse into overlay drawers.
  - **Mobile (< 768px)**: Navigation collapses into a bottom or top command bar; grids collapse into stacked lists with horizontally scrollable metric cards.

## Elevation & Depth

Depth is established primarily through **low-contrast outlines** and **tonal surface separation** rather than diffuse shadows.

- **Border Technique**: All containers, inputs, and cards use a 1px border (`#282828`). Interactive hover states brighten this border to `#383838` or `var(--primary-color)`.
- **Tonal Tiers**:
  - Canvas: Base layer (`#121212`).
  - Containers / Sidebars: Tier 1 (`#181818`), isolated with a right or top 1px border.
  - Cards & Content Blocks: Tier 2 (`#222222`), framed by 1px subtle borders.
  - Overlays & Popovers: Tier 3 (`#2A2A2A`), paired with a very subtle black ambient falloff (`0 8px 24px rgba(0, 0, 0, 0.45)`).
- **Focus & Selection**: Outlines over shadows. Active elements gain a sharp 1px highlight border in `#3ECF8E` accompanied by an optional `0 0 0 1px #3ECF8E` ring.

## Shapes

The design system embraces a **Soft (Level 1)** curvature system (`0.25rem` / `4px` to `6px`). This restrained curvature emphasizes structural discipline, tool-like precision, and compact alignment.

- **Standard Elements (4px - 6px)**: Buttons, text inputs, code viewports, and table container corners.
- **Status Badges & Semantic Chips (Pill shape)**: Status pills (e.g., `RUNNING`, `CRITICAL`, `200 OK`) utilize fully rounded radii (`9999px`) to immediately visually differentiate tags from interactive buttons and structural containers.
- **Inner-to-Outer Curvature**: When nested inside a card with a `6px` radius, child elements use a `4px` or `2px` radius to maintain concentric geometry.

## Components

### Buttons
- **Primary**: Background `#1E8854`, text `#FFFFFF`, border `1px solid #3ECF8E`. On hover, background shifts to `#249b61`. Height: `32px` (standard) or `28px` (compact). Font: `Inter` semi-bold 13px.
- **Secondary / Ghost**: Background `transparent`, text `#E6E6E6`, border `1px solid #282828`. Hover: Background `#222222`, border `#383838`.
- **Destructive**: Background `transparent`, border `1px solid #EF4444`, text `#EF4444`. Hover: Background `#EF4444` with white text.

### Status Badges & Pills
- Fully rounded (`rounded-full`), `20px` height, `JetBrains Mono` 11px uppercase font with tight letter spacing.
- **Structure**: Semi-transparent background (12% opacity) tinted with the status hue, 1px border (25% opacity), accompanied by a solid `6px` colored dot indicator on the left.
- **Variants**:
  - Critical: Background `rgba(239, 68, 68, 0.12)`, border `rgba(239, 68, 68, 0.25)`, text `#F87171`.
  - High: Background `rgba(249, 115, 22, 0.12)`, border `rgba(249, 115, 22, 0.25)`, text `#FB923C`.
  - Medium: Background `rgba(245, 158, 11, 0.12)`, border `rgba(245, 158, 11, 0.25)`, text `#FBBF24`.
  - Low / Informational: Background `rgba(6, 182, 212, 0.12)`, border `rgba(6, 182, 212, 0.25)`, text `#22D3EE`.

### Input Fields & Controls
- **Text Inputs**: Height `34px`, background `#141414`, border `1px solid #282828`, radius `4px`, text `#FFFFFF`. Placeholder in `#737373`. Focus state replaces border with `#3ECF8E` without heavy glow.
- **Checkboxes & Radios**: `16px x 16px`, background `#181818`, border `1px solid #383838`. Checked state fills with `#1E8854` displaying a crisp white icon.

### Cards & Panels
- Background `#181818`, border `1px solid #282828`, padding `1rem` to `1.25rem`.
- Header areas feature a subtle bottom border (`#222222`) separating titles from configuration forms or metric displays.

### Data Tables & Log Inspectors
- Headers: `#181818` background, uppercase `11px` Inter with `0.04em` letter-spacing, text `#8A8A8A`.
- Rows: Alternating hover state highlighting `#202020`. Cell borders separated with `1px solid #222222`.
- Code cells: Inlined `JetBrains Mono` with muted syntax tokens for keys, values, and operators.