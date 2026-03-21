//! Satellite orbital mechanics: Keplerian propagator and pseudorange computation.
//!
//! # References
//! - IS-GPS-200 §20.3.3.4.3.1 — User algorithm for ephemeris determination
//! - IS-GPS-200 §20.3.3.3.3.1 — SV clock correction

use super::coords::{ecef_to_llh, ecef_to_neu, ltc_matrix, neu_to_azel};
use super::ionosphere::klobuchar_delay;
use super::types::{
    Ephemeris, GpsTime, IonoUtc,
    consts::{GM_EARTH, OMEGA_EARTH, SPEED_OF_LIGHT},
};

// ── Satellite state ───────────────────────────────────────────────────────────

/// Satellite position, velocity, and clock state at a given GPS time.
#[derive(Debug, Clone, Copy)]
pub struct SatState {
    /// ECEF position (m).
    pub pos_ecef: [f64; 3],
    /// ECEF velocity (m/s), derived analytically from the Keplerian equations.
    pub vel_ecef: [f64; 3],
    /// Satellite clock bias correction (s) — includes relativistic correction
    /// and group delay TGD, ready to apply to pseudorange.
    pub clk_bias: f64,
    /// Satellite clock drift (s/s).
    pub clk_drift: f64,
}

// ── Keplerian propagator ──────────────────────────────────────────────────────

/// Propagate satellite position and clock to GPS observation time `t`.
///
/// Implements the full IS-GPS-200 §20.3.3.4.3.1 algorithm:
/// 1. Time since ephemeris reference epoch (with ±half-week wrap)
/// 2. Corrected mean motion
/// 3. Mean anomaly
/// 4. Eccentric anomaly — Newton-Raphson, tolerance 1e-14 rad
/// 5. True anomaly
/// 6. Argument of latitude with second-harmonic corrections
/// 7. ECEF longitude of ascending node (includes Earth rotation)
/// 8. ECEF position
/// 9. Analytical velocity
/// 10. Satellite clock correction (polynomial + relativistic Δtr − TGD)
pub fn sat_pos(eph: &Ephemeris, t: GpsTime) -> SatState {
    // ── 1. Time since ephemeris ───────────────────────────────────────────────
    let mut tk = t.sub(eph.toe);
    // Wrap to ±302 400 s (half-week) to handle week rollover.
    if tk > 302_400.0 {
        tk -= GpsTime::SECS_PER_WEEK;
    }
    if tk < -302_400.0 {
        tk += GpsTime::SECS_PER_WEEK;
    }

    // ── 2. Corrected mean motion ──────────────────────────────────────────────
    let a = eph.sqrta * eph.sqrta; // semi-major axis (m)
    let n0 = (GM_EARTH / (a * a * a)).sqrt(); // Keplerian mean motion (rad/s)
    let n = n0 + eph.deltan; // corrected mean motion

    // ── 3. Mean anomaly ───────────────────────────────────────────────────────
    let mk = eph.m0 + n * tk;

    // ── 4. Eccentric anomaly (Newton-Raphson) ─────────────────────────────────
    // Starting from Mk, iterate E = Mk + e·sin(E) until |ΔE| < 1e-14 rad.
    let mut ek = mk;
    for _ in 0..50 {
        let ek_old = ek;
        ek = mk + eph.ecc * ek.sin();
        if (ek - ek_old).abs() < 1e-14 {
            break;
        }
    }
    let sek = ek.sin();
    let cek = ek.cos();

    // ── 5. True anomaly ───────────────────────────────────────────────────────
    // ν = atan2(√(1-e²)·sin(E), cos(E) - e)  then add ω to get argument of lat.
    let vk_raw = ((1.0 - eph.ecc * eph.ecc).sqrt() * sek).atan2(cek - eph.ecc);
    let vk = vk_raw + eph.aop; // argument of latitude φ (before corrections)

    // ── 6. Second-harmonic corrections ───────────────────────────────────────
    let sin2vk = (2.0 * vk).sin();
    let cos2vk = (2.0 * vk).cos();

    let delta_u = eph.cus * sin2vk + eph.cuc * cos2vk; // argument of lat corr.
    let delta_r = eph.crs * sin2vk + eph.crc * cos2vk; // radius correction (m)
    let delta_i = eph.cis * sin2vk + eph.cic * cos2vk; // inclination corr.

    let u = vk + delta_u; // corrected arg. of lat.
    let r = a * (1.0 - eph.ecc * cek) + delta_r; // corrected radius (m)
    let i = eph.inc0 + eph.idot * tk + delta_i; // corrected inclination

    // ── 7. Longitude of ascending node (ECEF) ────────────────────────────────
    // Ω = Ω₀ + (Ω̇ - Ω_e)·tk - Ω_e·toe
    // Earth's rotation during signal travel is handled in compute_range (Sagnac).
    let omegadot_corr = eph.omgdot - OMEGA_EARTH;
    let ok = eph.omg0 + omegadot_corr * tk - OMEGA_EARTH * eph.toe.sec;

    // ── 8. ECEF position ──────────────────────────────────────────────────────
    let (su, cu) = (u.sin(), u.cos());
    let (si, ci) = (i.sin(), i.cos());
    let (so, co) = (ok.sin(), ok.cos());
    let xp = r * cu; // in-plane coordinates
    let yp = r * su;

    let pos_ecef = [xp * co - yp * ci * so, xp * so + yp * ci * co, yp * si];

    // ── 9. Analytical velocity ────────────────────────────────────────────────
    // Derived by differentiating position w.r.t. time (chain rule through E, ν, u, r, i, Ω).
    let ecc_sq = eph.ecc * eph.ecc;
    let dek_dt = n / (1.0 - eph.ecc * cek);

    // dν/dt — avoid division by zero when sin(ν) ≈ 0 (at apogee/perigee).
    let sin_nu = vk_raw.sin();
    let dvk_dt = if sin_nu.abs() > 1e-10 {
        sek * dek_dt * (1.0 + eph.ecc * vk_raw.cos()) / (sin_nu * (1.0 - ecc_sq).sqrt())
    } else {
        // At perigee/apogee, use the limit: dν/dt ≈ n(1+e)/(1-e)^(3/2) or (1-e)/(1+e)^(3/2)
        dek_dt * (1.0 - ecc_sq).sqrt() / (1.0 - eph.ecc * cek).powi(2)
    };

    let du_dt = dvk_dt * (1.0 + 2.0 * (eph.cus * cos2vk - eph.cuc * sin2vk));
    let dr_dt = a * eph.ecc * sek * dek_dt + 2.0 * dvk_dt * (eph.crs * cos2vk - eph.crc * sin2vk);
    let di_dt = eph.idot + 2.0 * dvk_dt * (eph.cis * cos2vk - eph.cic * sin2vk);
    let dok_dt = omegadot_corr;

    let vxp = dr_dt * cu - r * du_dt * su;
    let vyp = dr_dt * su + r * du_dt * cu;

    let vel_ecef = [
        vxp * co - vyp * ci * so - (xp * so + yp * ci * co) * dok_dt - yp * si * so * di_dt,
        vxp * so + vyp * ci * co + (xp * co - yp * ci * so) * dok_dt + yp * si * co * di_dt,
        vyp * si + yp * ci * di_dt,
    ];

    // ── 10. Satellite clock correction ────────────────────────────────────────
    // Relativistic correction: Δtr = -4.442807633e-10 · e · √A · sin(E)
    let rel = -4.442_807_633e-10 * eph.ecc * eph.sqrta * sek;

    // Clock polynomial evaluated at (t - toc).
    let dt_clk = t.sub(eph.toc);
    let clk_bias = eph.af0 + dt_clk * (eph.af1 + dt_clk * eph.af2) + rel - eph.tgd;
    let clk_drift = eph.af1 + 2.0 * dt_clk * eph.af2;

    SatState {
        pos_ecef,
        vel_ecef,
        clk_bias,
        clk_drift,
    }
}

// ── Range computation ─────────────────────────────────────────────────────────

/// Result of a pseudorange computation for one satellite–receiver pair.
#[derive(Debug, Clone, Copy)]
pub struct RangeResult {
    /// GPS time at which this range was computed.
    pub g: GpsTime,
    /// Pseudorange (geometric + clock + iono corrections) in metres.
    pub range: f64,
    /// Geometric distance satellite → receiver (before corrections), metres.
    pub d: f64,
    /// `[azimuth_rad, elevation_rad]` of the satellite seen from the receiver.
    pub azel: [f64; 2],
    /// Pseudorange rate (m/s) — used to derive Doppler.
    pub rate: f64,
}

/// Compute the pseudorange and Doppler for one satellite at GPS time `grx`.
///
/// Returns `None` if the satellite is below the horizon (elevation < 0).
///
/// # Algorithm
/// 1. Propagate satellite to `grx`.
/// 2. First-pass geometric range → signal travel time τ.
/// 3. Apply Sagnac correction: rotate satellite position by `Ω_e` · τ.
/// 4. Final range, range rate, azimuth, elevation.
/// 5. Add Klobuchar ionospheric delay.
#[expect(
    clippy::indexing_slicing,
    reason = "from_fn guarantees i<3; pos_ecef, rx_ecef, rot_pos, vel_ecef, d_pos are all fixed-size [f64;3]"
)]
pub fn compute_range(
    eph: &Ephemeris,
    iono: &IonoUtc,
    grx: GpsTime,
    rx_ecef: [f64; 3],
) -> Option<RangeResult> {
    let state = sat_pos(eph, grx);

    // ── First-pass geometric range ────────────────────────────────────────────
    let d_pos0: [f64; 3] = std::array::from_fn(|i| state.pos_ecef[i] - rx_ecef[i]);
    let range0 = (d_pos0[0] * d_pos0[0] + d_pos0[1] * d_pos0[1] + d_pos0[2] * d_pos0[2]).sqrt();
    let tau = range0 / SPEED_OF_LIGHT; // signal travel time (s)

    // ── Sagnac correction: Earth rotates during signal travel ─────────────────
    // Rotate the satellite's ECEF position by Ω_e · τ about the Z-axis.
    let w = OMEGA_EARTH * tau;
    let (sw, cw) = (w.sin(), w.cos());
    let rot_pos = [
        state.pos_ecef[0] * cw + state.pos_ecef[1] * sw,
        -state.pos_ecef[0] * sw + state.pos_ecef[1] * cw,
        state.pos_ecef[2],
    ];

    // ── Final range, elevation, azimuth ──────────────────────────────────────
    let d_pos: [f64; 3] = std::array::from_fn(|i| rot_pos[i] - rx_ecef[i]);
    let d = (d_pos[0] * d_pos[0] + d_pos[1] * d_pos[1] + d_pos[2] * d_pos[2]).sqrt();

    let rx_llh = ecef_to_llh(rx_ecef);
    let ltc = ltc_matrix(rx_llh);
    let neu = ecef_to_neu(d_pos, ltc);
    let (az, el) = neu_to_azel(neu);

    if el < 0.0 {
        return None;
    } // below horizon — do not track

    // ── Range rate (dot product of velocity with unit LOS vector) ─────────────
    let rate = state.vel_ecef[0] * d_pos[0] / d
        + state.vel_ecef[1] * d_pos[1] / d
        + state.vel_ecef[2] * d_pos[2] / d;

    // ── Pseudorange = geometric - clock_bias + iono + tropo ──────────────────
    let llh_arr = [rx_llh.lat_rad, rx_llh.lon_rad, rx_llh.height_m];
    let iono_m = klobuchar_delay(iono, grx, llh_arr, [az, el]);
    let trop_m = super::troposphere::tropospheric_delay(el, rx_llh.height_m);
    let range = d - SPEED_OF_LIGHT * state.clk_bias + iono_m + trop_m;

    #[expect(
        clippy::tuple_array_conversions,
        reason = "az and el are plain f64 locals, not a tuple being converted"
    )]
    Some(RangeResult {
        g: grx,
        range,
        d,
        azel: [az, el],
        rate,
    })
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use approx::assert_relative_eq;

    fn dummy_eph() -> Ephemeris {
        // Circular orbit at GPS altitude — useful for sanity checks.
        let a = 26_560_000.0_f64; // semi-major axis (m)
        Ephemeris {
            valid: true,
            sqrta: a.sqrt(),
            ecc: 0.0,
            inc0: 0.95, // ~55°
            m0: 0.0,
            omg0: 0.0,
            aop: 0.0,
            omgdot: -8.0e-9,
            idot: 0.0,
            deltan: 0.0,
            toe: GpsTime {
                week: 2300,
                sec: 0.0,
            },
            toc: GpsTime {
                week: 2300,
                sec: 0.0,
            },
            ..Default::default()
        }
    }

    #[test]
    fn circular_orbit_radius() {
        let eph = dummy_eph();
        let state = sat_pos(
            &eph,
            GpsTime {
                week: 2300,
                sec: 0.0,
            },
        );
        let r = (state.pos_ecef.iter().map(|x| x * x).sum::<f64>()).sqrt();
        let expected = eph.sqrta * eph.sqrta;
        assert_relative_eq!(r, expected, epsilon = 1.0); // within 1 m
    }

    #[test]
    #[expect(
        clippy::indexing_slicing,
        reason = "indexing fixed-size [f64;3] arrays with i<3 in test"
    )]
    fn velocity_magnitude_circular() {
        // Verify analytical velocity against numerical differentiation.
        // Note: ECEF velocity differs from the inertial orbital speed because
        // it includes Earth's rotation (Ω_e × r), so we can't compare directly
        // to sqrt(GM/a).  Instead we check the analytical vel agrees with a
        // finite-difference estimate at a 1 ms step.
        let eph = dummy_eph();
        let dt = 0.001_f64; // 1 ms
        let t0 = GpsTime {
            week: 2300,
            sec: 0.0,
        };
        let t1 = GpsTime {
            week: 2300,
            sec: dt,
        };
        let s0 = sat_pos(&eph, t0);
        let s1 = sat_pos(&eph, t1);

        let v_numerical = (0..3)
            .map(|i| ((s1.pos_ecef[i] - s0.pos_ecef[i]) / dt).powi(2))
            .sum::<f64>()
            .sqrt();
        let v_analytical = s0.vel_ecef.iter().map(|x| x * x).sum::<f64>().sqrt();

        assert_relative_eq!(v_analytical, v_numerical, epsilon = 1.0); // within 1 m/s
    }
}
