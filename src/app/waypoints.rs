//! Waypoint persistence and the add / edit / delete operations behind the
//! Manage Waypoints page.

use std::path::PathBuf;

use crate::app::MyApp;

impl MyApp {
    /// Reloads waypoints from `waypoint/waypoint.json` into `self.waypoints`.
    pub fn load_waypoints(&mut self) {
        let path = crate::paths::waypoint_dir()
            .map(|d| d.join("waypoint.json"))
            .unwrap_or_else(|e| {
                log::warn!("Could not create waypoint directory: {e}");
                PathBuf::from("waypoint.json")
            });
        self.waypoints = crate::waypoint::load_waypoints(&path);
        self.waypoints_loaded = true;
    }

    /// Persists `self.waypoints` to `waypoint/waypoint.json`.
    pub fn save_waypoints(&self) {
        let path = crate::paths::waypoint_dir()
            .map(|d| d.join("waypoint.json"))
            .unwrap_or_else(|e| {
                log::warn!("Could not create waypoint directory: {e}");
                PathBuf::from("waypoint.json")
            });
        crate::waypoint::save_waypoints(&path, &self.waypoints);
    }

    /// Copies the waypoint at `index` into the edit form.
    /// Calling again with the same index cancels the edit.
    #[expect(
        clippy::indexing_slicing,
        reason = "index comes from .position(), always valid"
    )]
    pub fn edit_waypoint(&mut self, index: usize) {
        if self.editing_index == Some(index) {
            self.editing_index = None;
            return;
        }
        self.editing_index = Some(index);
        self.new_waypoint = self.waypoints[index].clone();
        self.new_waypoint_coords = format!(
            "{}, {}",
            self.waypoints[index].lat, self.waypoints[index].lon
        );
        self.new_waypoint_coord_error = None;
    }

    /// Removes the waypoint at `index`.
    pub fn delete_waypoint(&mut self, index: usize) {
        self.waypoints.remove(index);
    }
}
