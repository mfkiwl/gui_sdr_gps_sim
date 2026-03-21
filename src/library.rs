//! Route library — scans `umf/` for `CSV` route files and persists metadata
//! to `library.json`.

use std::path::{Path, PathBuf};

/// A single entry in the route library.
#[derive(serde::Serialize, serde::Deserialize, Clone, Debug, Default)]
pub struct RouteEntry {
    /// File stem of the route (`{name}.csv` / `{name}.geojson`).
    pub name: String,
    /// Route length in metres, extracted from the companion `GeoJSON` file.
    pub distance_m: f64,
    /// Simulation duration in seconds (`csv_lines × 0.1 s`).
    pub duration_s: f64,
    /// Average velocity in km/h (`distance_m / duration_s × 3.6`).
    pub velocity_kmh: f64,
}

/// Returns the path to `library.json` inside the `umf/` directory.
///
/// # Errors
/// Propagates any error from [`crate::paths::umf_dir`].
pub fn library_path() -> Result<PathBuf, String> {
    crate::paths::umf_dir().map(|d| d.join("library.json"))
}

/// Reads and deserialises `library.json`.
/// Returns an empty `Vec` if the file does not exist or cannot be parsed.
pub fn load_library(path: &Path) -> Vec<RouteEntry> {
    let Ok(text) = std::fs::read_to_string(path) else {
        return Vec::new();
    };
    serde_json::from_str(&text).unwrap_or_default()
}

/// Serialises `entries` and writes them to `path`.
pub fn save_library(path: &Path, entries: &[RouteEntry]) {
    match serde_json::to_string_pretty(entries) {
        Ok(text) => {
            if let Err(e) = std::fs::write(path, text) {
                log::warn!("Failed to write library.json: {e}");
            }
        }
        Err(e) => log::warn!("Failed to serialise library: {e}"),
    }
}

/// Scans `umf_dir` for `.csv` files whose stem is absent from `existing`,
/// extracts metadata from the companion `.geojson`, and returns the new entries.
pub fn scan_new_routes(umf_dir: &Path, existing: &[RouteEntry]) -> Vec<RouteEntry> {
    let existing_names: std::collections::HashSet<&str> =
        existing.iter().map(|e| e.name.as_str()).collect();

    let dir_iter = match std::fs::read_dir(umf_dir) {
        Ok(it) => it,
        Err(e) => {
            log::warn!("Cannot read umf dir: {e}");
            return Vec::new();
        }
    };

    let mut new_entries: Vec<RouteEntry> = Vec::new();

    for entry in dir_iter.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("csv") {
            continue;
        }
        let name = match path.file_stem().and_then(|s| s.to_str()) {
            Some(n) => n.to_owned(),
            None => continue,
        };
        if existing_names.contains(name.as_str()) {
            continue;
        }

        let csv_text = match std::fs::read_to_string(&path) {
            Ok(t) => t,
            Err(e) => {
                log::warn!("Cannot read {}: {e}", path.display());
                continue;
            }
        };
        #[expect(
            clippy::cast_precision_loss,
            reason = "line counts are small enough that f64 precision is sufficient"
        )]
        let duration_s = csv_text.lines().filter(|l| !l.trim().is_empty()).count() as f64 * 0.1;

        let geojson_path = umf_dir.join(format!("{name}.geojson"));
        let distance_m = extract_distance(&geojson_path);

        let velocity_kmh = if duration_s > 0.0 {
            (distance_m / duration_s) * 3.6
        } else {
            0.0
        };

        new_entries.push(RouteEntry {
            name,
            distance_m,
            duration_s,
            velocity_kmh,
        });
    }

    new_entries
}

/// Attempts to read the total route length in metres from a `GeoJSON` file.
///
/// First tries the `ORS` summary field
/// (`features[0].properties.summary.distance`); falls back to a haversine
/// sum over the `LineString` coordinates.
fn extract_distance(path: &Path) -> f64 {
    let Ok(text) = std::fs::read_to_string(path) else {
        return 0.0;
    };
    let Ok(json) = serde_json::from_str::<serde_json::Value>(&text) else {
        return 0.0;
    };

    // ORS response: features[0].properties.summary.distance (metres)
    if let Some(dist) = json
        .pointer("/features/0/properties/summary/distance")
        .and_then(serde_json::Value::as_f64)
    {
        return dist;
    }

    // Fall back: compute haversine sum over the coordinate array.
    let Some(coords) = json
        .pointer("/features/0/geometry/coordinates")
        .or_else(|| json.pointer("/geometry/coordinates"))
        .or_else(|| json.pointer("/coordinates"))
        .and_then(serde_json::Value::as_array)
    else {
        return 0.0;
    };

    let mut total = 0.0;
    for pair in coords.windows(2) {
        if let [a, b] = pair {
            let a_lon = a.get(0).and_then(serde_json::Value::as_f64).unwrap_or(0.0);
            let a_lat = a.get(1).and_then(serde_json::Value::as_f64).unwrap_or(0.0);
            let b_lon = b.get(0).and_then(serde_json::Value::as_f64).unwrap_or(0.0);
            let b_lat = b.get(1).and_then(serde_json::Value::as_f64).unwrap_or(0.0);
            total += haversine_m(a_lat, a_lon, b_lat, b_lon);
        }
    }
    total
}

/// Haversine distance between two WGS-84 points, in metres.
fn haversine_m(lat1: f64, lon1: f64, lat2: f64, lon2: f64) -> f64 {
    const R: f64 = 6_371_000.0;
    let d_lat = (lat2 - lat1).to_radians();
    let d_lon = (lon2 - lon1).to_radians();
    let a = (d_lat / 2.0).sin().powi(2)
        + lat1.to_radians().cos() * lat2.to_radians().cos() * (d_lon / 2.0).sin().powi(2);
    R * 2.0 * a.sqrt().atan2((1.0 - a).sqrt())
}
