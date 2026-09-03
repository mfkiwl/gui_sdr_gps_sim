//! Route generation - turning the Create UMF Route form into a transmit-point
//! CSV, via the ORS API, a drawn polyline, or a `GeoJSON` file on disk.

use crate::app::{AppStatus, MyApp, RouteSource};
use crate::geo::parse_coords;
use crate::route::run_pipeline;

impl MyApp {
    /// Validates the route inputs and spawns the background pipeline task.
    pub fn generate(&mut self) {
        let route_name = self.route_name.trim().to_owned();
        if route_name.is_empty() {
            self.status = AppStatus::Error("Route name must not be empty.".to_owned());
            return;
        }
        let velocity: f64 = self.velocity.trim().parse().unwrap_or(3.0);

        match self.route_source {
            RouteSource::OrsApi => self.generate_ors(route_name, velocity),
            RouteSource::GeoJsonFile => self.generate_from_geojson_file(route_name, velocity),
            RouteSource::DrawImport | RouteSource::ImportKmlGpx => {
                self.generate_from_drawn_route(route_name, velocity);
            }
        }
    }

    /// ORS API branch of [`Self::generate`].
    #[expect(
        clippy::indexing_slicing,
        reason = "coords.len() >= 2 guard makes [0]/[1] safe"
    )]
    fn generate_ors(&mut self, route_name: String, velocity: f64) {
        let mut route_points: Vec<[f64; 2]> = Vec::new();

        match parse_coords(&self.start.text) {
            Ok(coords) if coords.len() >= 2 => route_points.push([coords[1], coords[0]]),
            Ok(_) => {
                self.status = AppStatus::Error("Start: enter lat, lon".to_owned());
                return;
            }
            Err(e) => {
                self.status = AppStatus::Error(format!("Start: {e}"));
                return;
            }
        }

        for (i, via) in self.viapoints.iter().enumerate() {
            match parse_coords(&via.text) {
                Ok(coords) if coords.len() >= 2 => route_points.push([coords[1], coords[0]]),
                Ok(_) => {
                    self.status = AppStatus::Error(format!("Via {}: enter lat, lon", i + 1));
                    return;
                }
                Err(e) => {
                    self.status = AppStatus::Error(format!("Via {}: {e}", i + 1));
                    return;
                }
            }
        }

        match parse_coords(&self.end.text) {
            Ok(coords) if coords.len() >= 2 => route_points.push([coords[1], coords[0]]),
            Ok(_) => {
                self.status = AppStatus::Error("End: enter lat, lon".to_owned());
                return;
            }
            Err(e) => {
                self.status = AppStatus::Error(format!("End: {e}"));
                return;
            }
        }

        let api_key = self.ors_api_key.trim().to_owned();
        if api_key.is_empty() {
            self.status = AppStatus::Error(
                "No ORS API key set. Use File → Set ORS API Key… to add your key.".to_owned(),
            );
            return;
        }
        let profile = self.ors_profile.clone();
        self.status = AppStatus::Working;
        let tx = self.result_tx.clone();
        self.rt.spawn(async move {
            let result = run_pipeline(route_points, velocity, route_name, api_key, profile).await;
            tx.send(result).ok();
        });
    }

    /// Draw/Import branch of [`Self::generate`].
    ///
    /// Serialises the current `draw_route_points` as a `GeoJSON` `FeatureCollection`,
    /// writes it to `umf/drawn_route.geojson`, then runs the segmentation pipeline.
    fn generate_from_drawn_route(&mut self, route_name: String, velocity: f64) {
        if self.draw_route_points.len() < 2 {
            self.status = AppStatus::Error("Draw at least 2 points on the map first.".to_owned());
            return;
        }

        let coords: Vec<serde_json::Value> = self
            .draw_route_points
            .iter()
            .map(|p| serde_json::json!([p.x(), p.y()]))
            .collect();

        let geojson = match serde_json::to_string_pretty(&serde_json::json!({
            "type": "FeatureCollection",
            "features": [{
                "type": "Feature",
                "geometry": { "type": "LineString", "coordinates": coords },
                "properties": {}
            }]
        })) {
            Ok(s) => s,
            Err(e) => {
                self.status = AppStatus::Error(format!("Failed to serialise route: {e}"));
                return;
            }
        };

        let path = match crate::paths::umf_dir() {
            Ok(dir) => dir.join("drawn_route.geojson"),
            Err(e) => {
                self.status = AppStatus::Error(e);
                return;
            }
        };

        if let Err(e) = std::fs::write(&path, geojson) {
            self.status = AppStatus::Error(format!("Failed to write GeoJSON: {e}"));
            return;
        }

        self.status = AppStatus::Working;
        let tx = self.result_tx.clone();
        self.rt.spawn(async move {
            let result = crate::route::run_pipeline_from_geojson(path, velocity, route_name).await;
            tx.send(result).ok();
        });
    }

    /// `GeoJSON`-file branch of [`Self::generate`].
    fn generate_from_geojson_file(&mut self, route_name: String, velocity: f64) {
        let Some(path) = self.route_geojson_path.clone() else {
            self.status = AppStatus::Error("No GeoJSON file selected.".to_owned());
            return;
        };
        self.status = AppStatus::Working;
        let tx = self.result_tx.clone();
        self.rt.spawn(async move {
            let result = crate::route::run_pipeline_from_geojson(path, velocity, route_name).await;
            tx.send(result).ok();
        });
    }
}
