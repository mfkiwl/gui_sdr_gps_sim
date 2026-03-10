//! Async pipeline that fetches a route, segments it, and writes the output CSV.

use std::path::PathBuf;

use super::{ors::get_ors_route, segment::segmentize};
use crate::geo::write_transmit_points_to_csv;

/// Runs the full route-generation pipeline.
///
/// 1. Fetches a walking route from `OpenRouteService`.
/// 2. Splits the route into segments and generates GPS transmit points.
/// 3. Writes transmit points to `{route_name}.csv`.
/// 4. Writes the raw route geometry to `{route_name}.geojson`.
///
/// Returns the total number of transmit points written.
///
/// # Errors
/// Returns a human-readable error string on any failure.
pub async fn run_pipeline(
    route_points: Vec<[f64; 2]>,
    velocity: f64,
    route_name: String,
    api_key: String,
    profile: String,
    optimized: bool,
) -> Result<usize, String> {
    let (lon, lat, ele, geojson_text) =
        get_ors_route(route_points, api_key, &profile, optimized)
            .await
            .map_err(|e| e.to_string())?;

    let segments = segmentize(&lon, &lat, &ele, velocity);

    let all_points: Vec<[f64; 3]> = segments
        .iter()
        .flat_map(|seg| seg.transmit_points.iter().copied())
        .collect();

    let count = all_points.len();

    let out_dir = crate::paths::umf_dir()?;
    let csv_path = out_dir.join(format!("{route_name}.csv"));
    write_transmit_points_to_csv(&all_points, &csv_path).map_err(|e| e.to_string())?;

    let geojson_path = out_dir.join(format!("{route_name}.geojson"));
    std::fs::write(&geojson_path, geojson_text).map_err(|e| e.to_string())?;

    Ok(count)
}

/// Runs the route-generation pipeline using a `GeoJSON` file from disk instead of
/// calling the `OpenRouteService` API.
///
/// Supports both 2-D (`[lon, lat]`) and 3-D (`[lon, lat, elevation_m]`) coordinate
/// arrays; missing elevation values default to `0.0`.
///
/// 1. Reads and parses the `GeoJSON` file.
/// 2. Extracts coordinate vectors.
/// 3. Segments the route and generates GPS transmit points.
/// 4. Writes transmit points to `{route_name}.csv`.
/// 5. Copies the source `GeoJSON` to `{route_name}.geojson`.
///
/// Returns the total number of transmit points written.
///
/// # Errors
/// Returns a human-readable error string on any I/O, parse, or segmentation failure.
pub async fn run_pipeline_from_geojson(
    path: PathBuf,
    velocity: f64,
    route_name: String,
) -> Result<usize, String> {
    let text = std::fs::read_to_string(&path)
        .map_err(|e| format!("Read '{}': {e}", path.display()))?;

    let json: serde_json::Value =
        serde_json::from_str(&text).map_err(|e| format!("Parse GeoJSON: {e}"))?;

    let mut lon: Vec<f64> = Vec::new();
    let mut lat: Vec<f64> = Vec::new();
    let mut ele: Vec<f64> = Vec::new();

    // Locate coordinate arrays: FeatureCollection → Feature → bare Geometry.
    let coord_arrays: Vec<&Vec<serde_json::Value>> =
        if let Some(features) = json.get("features").and_then(|f| f.as_array()) {
            features
                .iter()
                .filter_map(|f| {
                    f.pointer("/geometry/coordinates")
                        .and_then(serde_json::Value::as_array)
                })
                .collect()
        } else if let Some(coords) = json
            .pointer("/geometry/coordinates")
            .and_then(serde_json::Value::as_array)
        {
            vec![coords]
        } else if let Some(coords) = json
            .pointer("/coordinates")
            .and_then(serde_json::Value::as_array)
        {
            vec![coords]
        } else {
            return Err("No coordinate data found in GeoJSON.".to_owned());
        };

    for coords in coord_arrays {
        for pt in coords {
            let arr = pt
                .as_array()
                .ok_or_else(|| "Coordinate entry is not a JSON array.".to_owned())?;
            lon.push(
                arr.first()
                    .and_then(serde_json::Value::as_f64)
                    .ok_or_else(|| "Coordinate missing longitude.".to_owned())?,
            );
            lat.push(
                arr.get(1)
                    .and_then(serde_json::Value::as_f64)
                    .ok_or_else(|| "Coordinate missing latitude.".to_owned())?,
            );
            ele.push(arr.get(2).and_then(serde_json::Value::as_f64).unwrap_or(0.0));
        }
    }

    if lon.is_empty() {
        return Err("GeoJSON contains no coordinates.".to_owned());
    }

    let segments = segmentize(&lon, &lat, &ele, velocity);

    let all_points: Vec<[f64; 3]> = segments
        .iter()
        .flat_map(|seg| seg.transmit_points.iter().copied())
        .collect();

    let count = all_points.len();

    let out_dir = crate::paths::umf_dir()?;
    let csv_path = out_dir.join(format!("{route_name}.csv"));
    write_transmit_points_to_csv(&all_points, &csv_path).map_err(|e| e.to_string())?;

    let geojson_path = out_dir.join(format!("{route_name}.geojson"));
    std::fs::write(&geojson_path, &text).map_err(|e| e.to_string())?;

    Ok(count)
}
