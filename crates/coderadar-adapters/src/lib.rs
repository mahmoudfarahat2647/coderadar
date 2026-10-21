//! CodeRadar Studio — Layer 3: Interface Adapters.
//!
//! Translates data between the format most convenient for the use cases and
//! entities and the format most convenient for the outside world (webview UI,
//! HTTP, SQLite). Depends inward on Layers 2 and 1 only.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

mod diff;
mod dto;
mod radar;
mod report_markdown;

pub use diff::{unified_diff, UnifiedDiff};
pub use dto::{
    FindingDTO, HistoryPointDTO, LineEditDTO, PatchOutcomeDTO, PatchPreviewDTO, PatchProposalDTO,
};
pub use radar::{compute_angle_jitter, PolarCoordinate, RadarPresenter, MAX_ANGLE_JITTER_DEG};
pub use report_markdown::{
    format_timestamp, render_markdown_report, MarkdownFinding, MarkdownReport, MarkdownReportInput,
};
