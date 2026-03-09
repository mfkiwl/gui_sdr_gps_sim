//! Waypoint data types and JSON persistence.

use std::{fs, path::Path};

/// A named GPS location used to build routes and mark points of interest.
#[derive(serde::Serialize, serde::Deserialize, Clone, Debug, Default)]
pub struct Waypoint {
    pub lat: f64,
    pub lon: f64,
    pub name: String,
    pub location: String,
    pub category: String,
}

/// A single coordinate text field used for route start / via / end inputs.
///
/// The user types `lat, lon` and the value is parsed on demand.
#[derive(Default, serde::Serialize, serde::Deserialize)]
pub struct WaypointEntry {
    pub text: String,
}

/// Loads waypoints from a JSON file.
///
/// Returns an empty `Vec` when the file does not exist.
/// Logs a warning if the file cannot be read or parsed.
pub fn load_waypoints(path: &Path) -> Vec<Waypoint> {
    if !path.exists() {
        return Vec::new();
    }

    match fs::read_to_string(path) {
        Ok(contents) => match serde_json::from_str::<Vec<Waypoint>>(&contents) {
            Ok(waypoints) => waypoints,
            Err(e) => {
                log::warn!("Failed to parse waypoints: {e}");
                Vec::new()
            }
        },
        Err(e) => {
            log::warn!("Failed to read waypoint file: {e}");
            Vec::new()
        }
    }
}

/// Saves waypoints to a JSON file with pretty formatting.
///
/// Logs a warning on failure.
pub fn save_waypoints(path: &Path, waypoints: &[Waypoint]) {
    match serde_json::to_string_pretty(waypoints) {
        Ok(json) => {
            if let Err(e) = fs::write(path, json) {
                log::warn!("Failed to write waypoint file: {e}");
            }
        }
        Err(e) => log::warn!("Failed to serialize waypoints: {e}"),
    }
}
