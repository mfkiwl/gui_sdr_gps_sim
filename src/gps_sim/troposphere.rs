//! Simplified tropospheric delay model for GPS pseudorange correction.
//!
//! The troposphere introduces a delay of roughly 2.3 m at zenith at sea level,
//! increasing to ~25 m near the horizon.  This module implements the simplified
//! Hopfield / Saastamoinen model with a standard atmosphere and a 1/sin(el)
//! mapping function.
//!
//! # References
//! - Hofmann-Wellenhof et al., *GPS: Theory and Practice*, 5th ed., §5.6
//! - Saastamoinen, J. (1972), *Contributions to the theory of atmospheric
//!   refraction*, Bull. Géod. 105, pp. 279–298

/// Compute the tropospheric path delay for a satellite at `elevation_rad`.
///
/// Uses the simplified Saastamoinen zenith total delay (ZTD) with a standard
/// atmosphere:
/// - Surface pressure P₀ = 1013.25 hPa
/// - Surface temperature T₀ = 293.15 K (20 °C)
/// - ZTD scale height H = 8 500 m
///
/// The delay decreases exponentially with receiver altitude and is mapped to
/// the slant direction using the `1/sin(el)` mapping function, with a 5°
/// elevation floor to avoid singularities.
///
/// # Parameters
/// - `elevation_rad` — satellite elevation angle in radians.
/// - `height_m`      — receiver altitude above WGS-84 ellipsoid (m).
///
/// # Returns
/// Path delay in metres (always ≥ 0).
///
/// # Examples
/// ```
/// use gui_sdr_gps_sim::gps_sim::troposphere::tropospheric_delay;
/// let delay = tropospheric_delay(30_f64.to_radians(), 0.0);
/// assert!(delay > 4.0 && delay < 6.0, "30° elevation delay: {delay:.2} m");
/// ```
pub fn tropospheric_delay(elevation_rad: f64, height_m: f64) -> f64 {
    // Zenith total delay at the receiver's altitude.
    // ZTD(0) ≈ 2.3 m at sea level; scale height ≈ 8 500 m.
    let ztd = 2.3 * f64::exp(-height_m.max(0.0) / 8_500.0);

    // Mapping function: 1 / sin(el), floored at 5° to avoid near-zero division.
    let el_floored = elevation_rad.max(5.0_f64.to_radians());
    ztd / el_floored.sin()
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use approx::assert_relative_eq;

    #[test]
    fn zenith_delay_sea_level() {
        // At 90° elevation (zenith) and sea level the delay equals ZTD ≈ 2.3 m.
        let d = tropospheric_delay(90_f64.to_radians(), 0.0);
        assert_relative_eq!(d, 2.3, epsilon = 1e-6);
    }

    #[test]
    fn delay_increases_at_low_elevation() {
        let zen  = tropospheric_delay(90_f64.to_radians(), 0.0);
        let low  = tropospheric_delay(10_f64.to_radians(), 0.0);
        assert!(low > zen, "low elevation delay {low:.2} m should exceed zenith {zen:.2} m");
    }

    #[test]
    fn delay_decreases_with_altitude() {
        let sea   = tropospheric_delay(45_f64.to_radians(), 0.0);
        let high  = tropospheric_delay(45_f64.to_radians(), 3000.0);
        assert!(high < sea, "high-altitude delay should be smaller");
    }

    #[test]
    fn floor_at_five_degrees() {
        // Requesting 1° should give the same result as 5° (clamped).
        let d1 = tropospheric_delay(1_f64.to_radians(), 0.0);
        let d5 = tropospheric_delay(5_f64.to_radians(), 0.0);
        assert_relative_eq!(d1, d5, epsilon = 1e-9);
    }
}
