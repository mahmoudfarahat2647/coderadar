use coderadar_domain::Finding;

/// Display-ready polar coordinate for a radar node (presentation model).
///
/// Produced by [`RadarPresenter`] and consumed by the canvas layer. The domain
/// [`Finding`] never carries these coordinates.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PolarCoordinate {
    /// `0.0` (safe baseline) to `1.0` (perimeter boundary).
    pub radius: f32,
    /// Deterministically hashed angle within the category quadrant.
    pub angle_deg: f32,
}

/// Maximum angular dispersion applied around a pillar's base angle.
pub const MAX_ANGLE_JITTER_DEG: f32 = 35.0;

const FNV_OFFSET_BASIS: u64 = 0xcbf2_9ce4_8422_2325;
const FNV_PRIME: u64 = 0x0000_0100_0000_01b3;

/// FNV-1a hash of a byte slice.
fn fnv1a(bytes: &[u8]) -> u64 {
    let mut hash = FNV_OFFSET_BASIS;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(FNV_PRIME);
    }
    hash
}

/// Map a file location to a deterministic jitter in `[-max_deg, +max_deg]`.
///
/// The same `(file_path, line_start)` always yields the same offset, so radar
/// nodes never "jump" between renders.
pub fn compute_angle_jitter(file_path: &str, line_start: u32, max_deg: f32) -> f32 {
    let mut seed = file_path.as_bytes().to_vec();
    seed.extend_from_slice(&line_start.to_le_bytes());
    let hash = fnv1a(&seed);
    // Project into [0.0, 1.0] with a denominator that is exact in f32.
    let unit = (hash % 10_001) as f32 / 10_000.0;
    (unit * 2.0 - 1.0) * max_deg
}

/// Projects domain findings onto the polar radar.
#[derive(Debug, Default, Clone, Copy)]
pub struct RadarPresenter;

impl RadarPresenter {
    /// Project a single finding to a polar coordinate.
    pub fn project_to_polar(finding: &Finding) -> PolarCoordinate {
        let base_angle = finding.category.radar_base_angle_deg();
        let jitter =
            compute_angle_jitter(&finding.file_path, finding.line_start, MAX_ANGLE_JITTER_DEG);
        PolarCoordinate {
            radius: finding.risk_score.value(),
            angle_deg: (base_angle + jitter).rem_euclid(360.0),
        }
    }

    /// Project an entire slice of findings.
    pub fn project_all(findings: &[Finding]) -> Vec<PolarCoordinate> {
        findings.iter().map(Self::project_to_polar).collect()
    }
}
