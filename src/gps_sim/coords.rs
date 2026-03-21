//! Coordinate system transforms for GPS positioning.
//!
//! All functions are pure (no side effects) and work only on stack-allocated
//! arrays, making them safe to call from the hot signal-generation loop.
//!
//! # Coordinate systems used
//!
//! | Name | Description |
//! |------|-------------|
//! | ECEF | Earth-Centred Earth-Fixed Cartesian (x, y, z) in metres |
//! | LLH  | Geodetic (lat_rad, lon_rad, height_m) on WGS-84 ellipsoid |
//! | NEU  | Local tangent plane: North, East, Up (metres) |
//! | Az/El | Azimuth and elevation angles (radians) |

use super::types::{
    Location,
    consts::{WGS84_A, WGS84_E},
};

/// Convert geodetic coordinates to ECEF Cartesian.
///
/// Uses the WGS-84 ellipsoid definition.  Input angles must be in radians.
///
/// # Returns
/// `[x, y, z]` in metres.
pub fn llh_to_ecef(loc: Location) -> [f64; 3] {
    let (s_lat, c_lat) = (loc.lat_rad.sin(), loc.lat_rad.cos());
    let (s_lon, c_lon) = (loc.lon_rad.sin(), loc.lon_rad.cos());

    // Radius of curvature in the prime vertical.
    let n = WGS84_A / (1.0 - WGS84_E * WGS84_E * s_lat * s_lat).sqrt();

    let r_xy = (n + loc.height_m) * c_lat;
    [
        r_xy * c_lon,
        r_xy * s_lon,
        (n * (1.0 - WGS84_E * WGS84_E) + loc.height_m) * s_lat,
    ]
}

/// Convert ECEF Cartesian to geodetic coordinates.
///
/// Uses the Bowring iterative method which converges to sub-millimetre
/// accuracy in ≤ 10 iterations (usually 4–5 suffice).
///
/// # Returns
/// [`Location`] with angles in radians and height in metres.
pub fn ecef_to_llh(xyz: [f64; 3]) -> Location {
    let [x, y, z] = xyz;
    let lon = y.atan2(x);
    let p = x.hypot(y);
    let e2 = WGS84_E * WGS84_E;

    // Initial latitude estimate.
    let mut lat = z.atan2(p * (1.0 - e2));

    for _ in 0..10 {
        let n = WGS84_A / (1.0 - e2 * lat.sin().powi(2)).sqrt();
        let lat_new = (z + e2 * n * lat.sin()).atan2(p);
        if (lat_new - lat).abs() < 1e-12 {
            lat = lat_new;
            break;
        }
        lat = lat_new;
    }

    let n = WGS84_A / (1.0 - e2 * lat.sin().powi(2)).sqrt();
    // Height: handle near-polar singularity (lat → 90°).
    let h = if lat.cos().abs() > 1e-10 {
        p / lat.cos() - n
    } else {
        z.abs() / lat.sin() - n * (1.0 - e2)
    };

    Location::radians(lat, lon, h)
}

/// Build the 3×3 Local Tangent Coordinate (LTC) rotation matrix.
///
/// Rows are the unit vectors of the local frame expressed in ECEF:
/// - row 0 → North
/// - row 1 → East
/// - row 2 → Up
///
/// Multiply an ECEF Δvector by this matrix to get North/East/Up components.
pub fn ltc_matrix(loc: Location) -> [[f64; 3]; 3] {
    let (s_lat, c_lat) = (loc.lat_rad.sin(), loc.lat_rad.cos());
    let (s_lon, c_lon) = (loc.lon_rad.sin(), loc.lon_rad.cos());

    [
        [-s_lat * c_lon, -s_lat * s_lon, c_lat], // North
        [-s_lon, c_lon, 0.0],                    // East
        [c_lat * c_lon, c_lat * s_lon, s_lat],   // Up
    ]
}

/// Project an ECEF delta-vector into the local North/East/Up frame.
///
/// `d` is the ECEF difference vector (satellite − receiver).
/// `ltc` is the matrix from [`ltc_matrix`].
///
/// # Returns
/// `[north_m, east_m, up_m]`.
#[inline]
#[expect(
    clippy::indexing_slicing,
    reason = "from_fn guarantees i<3; d and ltc[i] are fixed-size [f64;3]"
)]
pub fn ecef_to_neu(d: [f64; 3], ltc: [[f64; 3]; 3]) -> [f64; 3] {
    std::array::from_fn(|i| ltc[i][0] * d[0] + ltc[i][1] * d[1] + ltc[i][2] * d[2])
}

/// Convert a North/East/Up vector to azimuth and elevation angles.
///
/// Azimuth is measured clockwise from North (0 = North, π/2 = East).
/// Elevation is measured up from the horizontal plane.
///
/// # Returns
/// `(azimuth_rad, elevation_rad)`.
#[inline]
pub fn neu_to_azel(neu: [f64; 3]) -> (f64, f64) {
    let az = neu[1].atan2(neu[0]); // East/North
    let el = neu[2].atan2(neu[0].hypot(neu[1]));
    (az, el)
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use approx::assert_relative_eq;

    #[test]
    fn llh_ecef_round_trip_amsterdam() {
        let loc = Location::degrees(52.3676, 4.9041, 5.0);
        let xyz = llh_to_ecef(loc);
        let back = ecef_to_llh(xyz);
        assert_relative_eq!(loc.lat_rad, back.lat_rad, epsilon = 1e-10);
        assert_relative_eq!(loc.lon_rad, back.lon_rad, epsilon = 1e-10);
        assert_relative_eq!(loc.height_m, back.height_m, epsilon = 1e-4);
    }

    #[test]
    fn llh_ecef_round_trip_equator() {
        let loc = Location::degrees(0.0, 0.0, 0.0);
        let xyz = llh_to_ecef(loc);
        // On the equator at the prime meridian, x ≈ WGS84_A, y = z = 0.
        assert_relative_eq!(xyz[0], WGS84_A, epsilon = 1.0);
        assert_relative_eq!(xyz[1], 0.0, epsilon = 1e-6);
        assert_relative_eq!(xyz[2], 0.0, epsilon = 1e-6);
        let back = ecef_to_llh(xyz);
        assert_relative_eq!(back.lat_rad, 0.0, epsilon = 1e-10);
        assert_relative_eq!(back.lon_rad, 0.0, epsilon = 1e-10);
    }

    #[test]
    fn ltc_north_is_unit_vector() {
        let loc = Location::degrees(45.0, 0.0, 0.0);
        let m = ltc_matrix(loc);
        // Each row should be a unit vector.
        for row in &m {
            let len = (row[0] * row[0] + row[1] * row[1] + row[2] * row[2]).sqrt();
            assert_relative_eq!(len, 1.0, epsilon = 1e-14);
        }
    }
}
