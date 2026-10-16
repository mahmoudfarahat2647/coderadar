//! CodeRadar Studio — Layer 3: Interface Adapters.
//!
//! Translates data between the format most convenient for the use cases and
//! entities and the format most convenient for the outside world (webview UI,
//! HTTP, SQLite). Depends inward on Layers 2 and 1 only.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

mod dto;
mod radar;

pub use dto::FindingDTO;
pub use radar::{
    base_angle_for, compute_angle_jitter, PolarCoordinate, RadarPresenter, MAX_ANGLE_JITTER_DEG,
};
