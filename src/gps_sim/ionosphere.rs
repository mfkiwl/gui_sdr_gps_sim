//! Klobuchar single-layer ionospheric delay model.
//!
//! The model is documented in IS-GPS-200 §20.3.3.5.2.5.  Parameters α and β
//! are broadcast in subframe 4, page 18, and parsed from the RINEX header.
//!
//! The returned value is **always positive** (adding it to the geometric range
//! increases the pseudorange, as the ionosphere slows the signal).

use super::types::{IonoUtc, GpsTime, consts::{GPS_PI, SPEED_OF_LIGHT}};

/// Compute ionospheric path delay in **metres**.
///
/// Returns 0 if the ionospheric parameters are not available (`iono.valid = false`).
///
/// # Parameters
/// - `iono`: Klobuchar coefficients from the RINEX header.
/// - `t`:    GPS observation time.
/// - `llh`:  Receiver geodetic position `[lat_rad, lon_rad, height_m]`.
/// - `azel`: `[azimuth_rad, elevation_rad]` to the satellite.
pub fn klobuchar_delay(iono: &IonoUtc, t: GpsTime, llh: [f64; 3], azel: [f64; 2]) -> f64 {
    if !iono.valid {
        return 0.0;
    }

    // Elevation and user position in semi-circles (IS-GPS-200 convention).
    let e = azel[1] / GPS_PI;        // elevation in semi-circles [0, 0.5]
    let phi_u = llh[0] / GPS_PI;     // user latitude in semi-circles
    let lam_u = llh[1] / GPS_PI;     // user longitude in semi-circles

    // ── Earth-centred angle to ionospheric pierce point ───────────────────────
    // ψ (semi-circles): angle between receiver and pierce point at 350 km altitude.
    let psi = 0.0137 / (e + 0.11) - 0.022;

    // ── Geodetic latitude of pierce point ─────────────────────────────────────
    let phi_i = (phi_u + psi * azel[0].cos()).clamp(-0.416, 0.416);

    // ── Geodetic longitude of pierce point ───────────────────────────────────
    let lam_i = lam_u + psi * azel[0].sin() / (phi_i * GPS_PI).cos();

    // ── Geomagnetic latitude of pierce point ──────────────────────────────────
    let phi_m = phi_i + 0.064 * ((lam_i - 1.617) * GPS_PI).cos();

    // ── Vertical delay amplitude (polynomial in φ_m) ──────────────────────────
    let amp = {
        let a = &iono.alpha;
        a[0] + phi_m * (a[1] + phi_m * (a[2] + phi_m * a[3]))
    }.max(0.0);

    // ── Period of cosine variation ────────────────────────────────────────────
    let per = {
        let b = &iono.beta;
        b[0] + phi_m * (b[1] + phi_m * (b[2] + phi_m * b[3]))
    }.max(72_000.0);

    // ── Obliquity factor (slant/vertical ratio) ───────────────────────────────
    let f = 1.0 + 16.0 * (0.53 - e).powi(3);

    // ── Local time at the pierce point ────────────────────────────────────────
    // lam_i is in semi-circles (−1 to +1), convert to seconds then wrap to 24h.
    let t_local = (43_200.0 * lam_i + t.sec).rem_euclid(86_400.0);

    // ── Phase of cosine (x = 0 at 14:00 local solar time) ────────────────────
    let x = 2.0 * GPS_PI * (t_local - 50_400.0) / per;

    // ── Vertical ionospheric delay (seconds) then convert to metres ───────────
    // Night-time floor: 5 ns × c = 1.5 m.
    // Daytime: cosine approximated by a 4th-order Taylor series for |x| < π/2.
    let vert_sec = if x.abs() < 1.57 {
        f * (5e-9 + amp * (1.0 - x * x / 2.0 + x * x * x * x / 24.0))
    } else {
        f * 5e-9
    };

    vert_sec * SPEED_OF_LIGHT // convert seconds of delay to metres
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::types::IonoUtc;

    /// Typical mid-latitude test case: delay should be a few metres.
    #[test]
    fn klobuchar_reasonable_daytime() {
        let iono = IonoUtc {
            valid: true,
            alpha: [1.118e-8, 1.490e-8, -5.960e-8, -1.192e-7],
            beta:  [9.011e4,  6.554e4, -1.311e5,  -1.311e5],
            ..Default::default()
        };
        // ~35 °N, 5 °E, ~14:00 local solar time so it should be near the daytime peak.
        let t = GpsTime { week: 2300, sec: 388_800.0 };
        let llh = [0.6109, 0.0873, 100.0];
        let azel = [0.0, 1.0]; // north, ~57° elevation
        let delay = klobuchar_delay(&iono, t, llh, azel);
        assert!(delay > 0.0, "iono delay must be positive (got {delay})");
        assert!(delay < 50.0, "iono delay unrealistically large: {delay} m");
    }

    /// Night-time floor: delay ≈ 1.5 m regardless of amplitude coefficients.
    #[test]
    fn klobuchar_nighttime_floor() {
        let iono = IonoUtc {
            valid: true,
            alpha: [1.118e-8, 0.0, 0.0, 0.0],
            beta:  [9.011e4,  0.0, 0.0, 0.0],
            ..Default::default()
        };
        // 02:00 local solar time (far from the noon cosine peak).
        let t = GpsTime { week: 2300, sec: 7_200.0 };
        let llh = [0.0, 0.0, 0.0];
        let azel = [0.0, GPS_PI / 4.0]; // 45° elevation
        let delay = klobuchar_delay(&iono, t, llh, azel);
        // Night-time delay: F * 5e-9 * c ≈ 1.5 * 1.5 m ≈ 2.25 m at 45° elev.
        assert!(delay > 0.5 && delay < 5.0, "night delay out of range: {delay} m");
    }

    #[test]
    fn klobuchar_zero_when_invalid() {
        let iono = IonoUtc { valid: false, ..Default::default() };
        let delay = klobuchar_delay(&iono, GpsTime::default(), [0.0; 3], [0.0, 1.0]);
        assert_eq!(delay, 0.0);
    }
}
