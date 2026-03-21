//! IQ sample generation: lookup tables and antenna gain pattern.
//!
//! # Lookup tables
//! The carrier phase accumulator `φ ∈ [0, 1)` is mapped to a 512-entry
//! table index: `idx = (φ × 512) as usize & 511`.
//! Table values are `round(250 × sin/cos(2π × i / 512))`, stored as `i16`.
//! The amplitude of ±250 ensures that the sum of up to 12 channels stays
//! within the ±3000 range of `i32`, which is then right-shifted by 4 bits
//! before quantising to `i8` for `HackRF`.
//!
//! Tables are computed once at first access via [`std::sync::LazyLock`].

use std::f64::consts::PI;
use std::sync::LazyLock;

// ── Sine / cosine lookup tables ───────────────────────────────────────────────

/// 512-entry cosine lookup table, amplitude ±250.
///
/// `COS_TABLE[i] = round(250 · cos(2π · i / 512))` for i = 0..512.
pub static COS_TABLE: LazyLock<[i16; 512]> = LazyLock::new(|| {
    std::array::from_fn(|i| (250.0 * (2.0 * PI * i as f64 / 512.0).cos()).round() as i16)
});

/// 512-entry sine lookup table, amplitude ±250.
///
/// `SIN_TABLE[i] = round(250 · sin(2π · i / 512))` for i = 0..512.
pub static SIN_TABLE: LazyLock<[i16; 512]> = LazyLock::new(|| {
    std::array::from_fn(|i| (250.0 * (2.0 * PI * i as f64 / 512.0).sin()).round() as i16)
});

// ── Antenna gain pattern ──────────────────────────────────────────────────────

/// Receive antenna gain pattern (dB) versus boresight angle from zenith.
///
/// 37 entries at 5° steps: index 0 = zenith (0° from boresight = 90° elevation),
/// index 36 = horizon (90° from boresight = 0° elevation).
///
/// Source: GPS ICS-200 antenna model for a geodetic-grade patch antenna.
const ANT_PAT_DB: [f64; 37] = [
    0.00, 0.00, 0.22, 0.44, 0.67, 1.11, 1.56, 2.22, 3.10, 4.67, 6.89, 9.56, 12.78, 14.67, 15.56,
    15.56, 15.56, 15.56, 15.56, 15.56, 15.56, 15.56, 15.56, 15.56, 15.56, 15.56, 15.56, 15.56,
    15.56, 15.56, 15.56, 15.56, 15.56, 15.56, 15.56, 15.56, 15.56,
];

/// Return the linear voltage gain for a satellite at `elevation_rad`.
///
/// The boresight angle is `90° − elevation_deg`, clamped to the table range
/// 0°–90°.  Linear interpolation is used between adjacent 5° table entries for
/// a smooth gain curve.  The returned value is `10^(-dB / 20)` (voltage gain ≤ 1.0).
#[inline]
#[expect(
    clippy::indexing_slicing,
    reason = "idx_lo and idx_hi are clamped to [0, 36] — safe for the 37-element ANT_PAT_DB array"
)]
pub fn ant_gain(elevation_rad: f64) -> f64 {
    let boresight_deg = (90.0 - elevation_rad.to_degrees()).clamp(0.0, 180.0);
    let idx_f = boresight_deg / 5.0;
    let idx_lo = (idx_f as usize).min(36);
    let idx_hi = (idx_lo + 1).min(36);
    let frac = idx_f - idx_lo as f64;
    let db = ANT_PAT_DB[idx_lo] + frac * (ANT_PAT_DB[idx_hi] - ANT_PAT_DB[idx_lo]);
    f64::powf(10.0, -db / 20.0)
}

/// Pre-compute the 37-entry linear gain table for fast per-step lookup.
///
/// Returns `[f64; 37]` where `ant_linear[i] = ant_gain((90 - i*5)° in radians)`.
pub fn ant_pattern_linear() -> [f64; 37] {
    std::array::from_fn(|i| {
        let el_rad = ((90_i32 - i as i32 * 5) as f64).to_radians();
        ant_gain(el_rad)
    })
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use approx::assert_relative_eq;

    #[test]
    fn cos_table_length() {
        assert_eq!(COS_TABLE.len(), 512);
    }

    #[test]
    fn sin_table_length() {
        assert_eq!(SIN_TABLE.len(), 512);
    }

    /// `cos(0)` = 1 → `COS_TABLE[0]` should be ≈ 250.
    #[test]
    #[expect(
        clippy::indexing_slicing,
        reason = "index 0 always exists in a 512-element array"
    )]
    fn cos_table_zero_index() {
        assert_eq!(COS_TABLE[0], 250);
    }

    /// `sin(0)` = 0 → `SIN_TABLE[0]` should be 0.
    #[test]
    #[expect(
        clippy::indexing_slicing,
        reason = "index 0 always exists in a 512-element array"
    )]
    fn sin_table_zero_index() {
        assert_eq!(SIN_TABLE[0], 0);
    }

    /// sin²+cos² = 1 → sum of squares of table values ≈ 250² for every index.
    #[test]
    #[expect(
        clippy::indexing_slicing,
        reason = "loop iterates 0..512 matching the table size"
    )]
    fn pythagorean_identity() {
        for i in 0..512 {
            let s = SIN_TABLE[i] as f64;
            let c = COS_TABLE[i] as f64;
            let r = s.hypot(c);
            // Quantisation error ≤ 1 LSB.
            assert!((r - 250.0).abs() < 2.0, "index {i}: r = {r}");
        }
    }

    /// Zenith gain (index 0) should be 0 dB → linear gain = 1.0.
    #[test]
    fn ant_gain_zenith() {
        assert_relative_eq!(ant_gain(90_f64.to_radians()), 1.0, epsilon = 1e-10);
    }

    /// Horizon gain (index 36, 15.56 dB loss) → linear ≈ 0.167.
    #[test]
    fn ant_gain_horizon() {
        let g = ant_gain(0.0);
        assert!(g > 0.0 && g < 0.5, "horizon gain out of range: {g}");
    }
}
