//! Coordinate conversion utilities and CSV output for GPS transmit points.

use std::{fs::File, io::Write as _, path::Path};

/// Parses a comma-separated string of `f64` values.
///
/// # Errors
/// Returns an error if any token cannot be parsed as `f64`.
pub fn parse_coords(input: &str) -> Result<Vec<f64>, Box<dyn std::error::Error>> {
    let parsed: Result<Vec<f64>, _> = input.split(',').map(|s| s.trim().parse::<f64>()).collect();
    parsed.map_err(Into::into)
}

/// Converts geodetic LLA coordinates to ECEF (Earth-Centred Earth-Fixed).
///
/// Uses the WGS-84 ellipsoid. Returns `(X, Y, Z)` in metres.
pub fn lla_to_ecef(lat: f64, lon: f64, alt: f64) -> (f64, f64, f64) {
    const A: f64 = 6_378_137.0;           // WGS-84 semi-major axis (m)
    const F: f64 = 1.0 / 298.257_223_563; // WGS-84 flattening
    const E2: f64 = 2.0 * F - F * F;      // First eccentricity squared

    let lat_rad = lat.to_radians();
    let lon_rad = lon.to_radians();
    let cos_lat = lat_rad.cos();
    let sin_lat = lat_rad.sin();
    let n = A / (1.0 - E2 * sin_lat * sin_lat).sqrt(); // Prime vertical radius

    let x = (n + alt) * cos_lat * lon_rad.cos();
    let y = (n + alt) * cos_lat * lon_rad.sin();
    let z = (n * (1.0 - E2) + alt) * sin_lat;

    (x, y, z)
}

/// Converts LLA transmit points to ECEF and writes them to a CSV file.
///
/// Each row: `time_s, X, Y, Z` where `time_s` increments by 0.1 s per point.
/// Points are `[lon, lat, alt_m]`.
///
/// # Errors
/// Returns an error if the file cannot be created or written.
pub fn write_transmit_points_to_csv(
    transmit_points: &[[f64; 3]],
    path: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut file = File::create(path)?;

    for (index, point) in transmit_points.iter().enumerate() {
        let time_s = index as f64 * 0.1;
        let (x, y, z) = lla_to_ecef(point[1], point[0], point[2]);
        writeln!(file, "{time_s:.1},{x:.6},{y:.6},{z:.6}")?;
    }

    Ok(())
}
