//! Route-library management - scanning `umf/` for CSV routes and loading,
//! editing, and deleting the entries recorded in `library.json`.

use crate::app::{MyApp, RouteSource};

impl MyApp {
    /// Loads the `GeoJSON` for library entry `idx` into `sim_lib_route_points`,
    /// sets `sim_motion_path` to the corresponding `CSV` file, and centres the
    /// Dynamic Mode map on the first route point.
    pub fn load_sim_lib_route(&mut self, idx: usize) {
        self.sim_lib_route_points.clear();
        let Some(entry) = self.library.get(idx) else {
            return;
        };
        let name = entry.name.clone();
        let Ok(umf_dir) = crate::paths::umf_dir() else {
            return;
        };

        // Set the motion CSV path.
        let csv_path = umf_dir.join(format!("{name}.csv"));
        self.sim_motion_path = Some(csv_path);

        // Load the route geometry from the companion GeoJSON.
        let geojson_path = umf_dir.join(format!("{name}.geojson"));
        let Ok(text) = std::fs::read_to_string(&geojson_path) else {
            return;
        };
        let Ok(json) = serde_json::from_str::<serde_json::Value>(&text) else {
            return;
        };
        let Some(coords) = json
            .pointer("/features/0/geometry/coordinates")
            .or_else(|| json.pointer("/geometry/coordinates"))
            .or_else(|| json.pointer("/coordinates"))
            .and_then(serde_json::Value::as_array)
        else {
            return;
        };
        for pt in coords {
            let Some(arr) = pt.as_array() else {
                continue;
            };
            let lon = arr
                .first()
                .and_then(serde_json::Value::as_f64)
                .unwrap_or(0.0);
            let lat = arr
                .get(1)
                .and_then(serde_json::Value::as_f64)
                .unwrap_or(0.0);
            self.sim_lib_route_points.push(walkers::lat_lon(lat, lon));
        }
        if let Some(first) = self.sim_lib_route_points.first() {
            self.sim_lib_map_memory.center_at(*first);
        }
    }

    /// Loads `umf/library.json` into `self.library` (once per session).
    pub fn load_library(&mut self) {
        if self.library_loaded {
            return;
        }
        if let Ok(path) = crate::library::library_path() {
            self.library = crate::library::load_library(&path);
        }
        self.library_loaded = true;
    }

    /// Loads the route `GeoJSON` for `name` and populates `lib_route_points`.
    ///
    /// Centres `lib_map_memory` on the first point of the route. Clears the
    /// point list silently if the file cannot be read or parsed.
    pub fn load_library_route(&mut self, name: &str) {
        self.lib_route_points.clear();

        let path = match crate::paths::umf_dir() {
            Ok(d) => d.join(format!("{name}.geojson")),
            Err(_) => return,
        };

        let Ok(text) = std::fs::read_to_string(&path) else {
            return;
        };
        let Ok(json) = serde_json::from_str::<serde_json::Value>(&text) else {
            return;
        };

        let Some(coords) = json
            .pointer("/features/0/geometry/coordinates")
            .or_else(|| json.pointer("/geometry/coordinates"))
            .or_else(|| json.pointer("/coordinates"))
            .and_then(serde_json::Value::as_array)
        else {
            return;
        };

        for pt in coords {
            let Some(arr) = pt.as_array() else { continue };
            let lon = arr
                .first()
                .and_then(serde_json::Value::as_f64)
                .unwrap_or(0.0);
            let lat = arr
                .get(1)
                .and_then(serde_json::Value::as_f64)
                .unwrap_or(0.0);
            self.lib_route_points.push(walkers::lat_lon(lat, lon));
        }

        if let Some(first) = self.lib_route_points.first() {
            self.lib_map_memory.center_at(*first);
        }
    }

    /// Deletes the `CSV` and `GeoJSON` files for library entry `idx`.
    ///
    /// Missing files are silently ignored.  Returns without doing anything if
    /// `idx` is out of range or the `umf/` directory cannot be resolved.
    pub fn delete_library_route(&mut self, idx: usize) {
        let Some(entry) = self.library.get(idx) else {
            return;
        };
        let name = entry.name.clone();
        let Ok(umf_dir) = crate::paths::umf_dir() else {
            return;
        };
        for ext in ["csv", "geojson"] {
            let path = umf_dir.join(format!("{name}.{ext}"));
            if path.exists() {
                if let Err(e) = std::fs::remove_file(&path) {
                    log::warn!("Failed to delete {}: {e}", path.display());
                }
            }
        }
    }

    /// Clears `library.json` and rescans `umf/` from scratch.
    ///
    /// Useful after deleting or renaming route files — this rebuilds the
    /// entire index rather than only appending entries that are missing.
    pub fn clear_and_rescan_library(&mut self) {
        self.library.clear();
        if let Ok(path) = crate::library::library_path() {
            crate::library::save_library(&path, &[]);
        }
        self.scan_library();
    }

    /// Scans `umf/` for new `CSV` routes, appends them to `self.library`,
    /// and persists the result to `library.json`.
    pub fn scan_library(&mut self) {
        let umf_dir = match crate::paths::umf_dir() {
            Ok(d) => d,
            Err(e) => {
                log::warn!("Cannot determine umf dir: {e}");
                return;
            }
        };
        let lib_path = umf_dir.join("library.json");
        let new_entries = crate::library::scan_new_routes(&umf_dir, &self.library);
        self.library.extend(new_entries);
        crate::library::save_library(&lib_path, &self.library);
    }

    /// Loads the `GeoJSON` for library entry `idx` into `lib_edit_points` and
    /// centres `lib_edit_map_memory` on the first point.
    ///
    /// Sets `lib_edit_entry_idx` to `idx` on success.
    pub fn load_lib_edit_route(&mut self, idx: usize) {
        self.lib_edit_points.clear();
        let name = match self.library.get(idx) {
            Some(e) => e.name.clone(),
            None => return,
        };
        let path = match crate::paths::umf_dir() {
            Ok(d) => d.join(format!("{name}.geojson")),
            Err(_) => return,
        };
        let Ok(text) = std::fs::read_to_string(&path) else {
            return;
        };
        let Ok(json) = serde_json::from_str::<serde_json::Value>(&text) else {
            return;
        };
        let Some(coords) = json
            .pointer("/features/0/geometry/coordinates")
            .or_else(|| json.pointer("/geometry/coordinates"))
            .or_else(|| json.pointer("/coordinates"))
            .and_then(serde_json::Value::as_array)
        else {
            return;
        };
        for pt in coords {
            let Some(arr) = pt.as_array() else {
                continue;
            };
            let lon = arr
                .first()
                .and_then(serde_json::Value::as_f64)
                .unwrap_or(0.0);
            let lat = arr
                .get(1)
                .and_then(serde_json::Value::as_f64)
                .unwrap_or(0.0);
            self.lib_edit_points.push(walkers::lat_lon(lat, lon));
        }
        if let Some(first) = self.lib_edit_points.first() {
            self.lib_edit_map_memory.center_at(*first);
        }
        self.lib_edit_entry_idx = Some(idx);
    }

    /// Copies `lib_edit_points` into `draw_route_points`, pre-fills `route_name`
    /// and `velocity` from the library entry, and switches the route source to
    /// [`RouteSource::DrawImport`] so the user can generate a new CSV from the
    /// edited geometry.
    ///
    /// Also clears `lib_edit_entry_idx` so the editor is dismissed.
    pub fn open_lib_edit_in_draw_route(&mut self) {
        let Some(idx) = self.lib_edit_entry_idx else {
            return;
        };
        let Some(entry) = self.library.get(idx) else {
            return;
        };
        self.draw_route_points = self.lib_edit_points.clone();
        self.route_name = entry.name.clone();
        self.velocity = format!("{:.1}", entry.velocity_kmh);
        self.route_source = RouteSource::DrawImport;
        self.draw_route_status = None;
        if let Some(first) = self.draw_route_points.first() {
            self.draw_map_memory.center_at(*first);
        }
        self.lib_edit_entry_idx = None;
    }
}
