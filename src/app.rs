//! Application state, initialisation, and eframe integration.

use std::{
    path::PathBuf,
    sync::mpsc,
};

use crate::{
    geo::parse_coords,
    route::run_pipeline,
    ui,
    waypoint::{Waypoint, WaypointEntry},
};

/// Identifies which page is shown in the central panel.
#[derive(Debug, PartialEq, Eq, Clone, Copy, serde::Serialize, serde::Deserialize, Default)]
pub enum AppPage {
    #[default]
    Home,
    SdrGpsSimulator,
    CreateUmfRoute,
    ManageWaypoints,
    ManageUmfRoutes,
}

/// How the `GeoJSON` route geometry is obtained on the [`AppPage::CreateUmfRoute`] page.
#[derive(Debug, PartialEq, Eq, Clone, Copy, Default)]
pub enum RouteSource {
    /// Fetch the route from the `OpenRouteService` directions API.
    #[default]
    OrsApi,
    /// Load a pre-existing `GeoJSON` file from disk.
    GeoJsonFile,
}

/// Selects the active tab on the [`AppPage::SdrGpsSimulator`] page.
#[derive(Debug, PartialEq, Eq, Clone, Copy, Default)]
pub enum SimTab {
    /// Route-based simulation driven by a user-motion CSV file.
    #[default]
    Dynamic,
    /// Single fixed-position simulation (static coordinates).
    Static,
}

/// Tracks the current state of the background route-generation task.
#[derive(Default)]
pub enum AppStatus {
    #[default]
    Idle,
    Working,
    Done(usize),
    Error(String),
}

/// Top-level application state, persisted across sessions via eframe storage.
#[derive(serde::Deserialize, serde::Serialize)]
#[serde(default)]
pub struct MyApp {
    /// Currently visible page.
    #[serde(skip)]
    pub current_mode: AppPage,

    /// Waypoints loaded from / saved to `waypoint.json`.
    pub waypoints: Vec<Waypoint>,
    /// Set to `true` the first time `load_waypoints()` is called this session.
    /// Guards `on_exit` so we never overwrite `waypoint.json` with default-empty data.
    #[serde(skip)]
    pub waypoints_loaded: bool,
    /// Scratch space for the add / edit waypoint form.
    pub new_waypoint: Waypoint,
    /// Filter string applied to the waypoint table (not persisted).
    #[serde(skip)]
    pub filter_text: String,
    /// Column the waypoint table is sorted by, if any (not persisted).
    #[serde(skip)]
    pub sort_column: Option<usize>,
    /// `true` = ascending order, `false` = descending.
    pub sort_ascending: bool,
    /// Index of the waypoint currently being edited, if any (not persisted).
    #[serde(skip)]
    pub editing_index: Option<usize>,

    /// Coordinate text input (`lat, lon`) for the add/edit form (not persisted).
    #[serde(skip)]
    pub new_waypoint_coords: String,
    /// Parse error from the coordinates field, cleared on success (not persisted).
    #[serde(skip)]
    pub new_waypoint_coord_error: Option<String>,

    /// Name used for the output files (`{route_name}.csv` / `{route_name}.geojson`).
    #[serde(skip)]
    pub route_name: String,

    /// `ORS` routing profile (e.g. `"foot-walking"`, `"driving-car"`).
    pub ors_profile: String,

    /// Use contraction hierarchies for faster `ORS` routing.
    ///
    /// Disable to unlock turn restrictions and avoid-polygon support.
    pub ors_optimized: bool,

    /// How to obtain the route `GeoJSON` (not persisted).
    #[serde(skip)]
    pub route_source: RouteSource,

    /// Path to a user-supplied `GeoJSON` route file (not persisted).
    #[serde(skip)]
    pub route_geojson_path: Option<PathBuf>,

    /// Pending file-dialog receiver for the `GeoJSON` picker (not persisted).
    #[serde(skip)]
    pub route_geojson_dialog: Option<mpsc::Receiver<Option<PathBuf>>>,

    /// Route start coordinate (`lat, lon` as free text).
    #[serde(skip)]
    pub start: WaypointEntry,
    /// Optional intermediate waypoints.
    #[serde(skip)]
    pub viapoints: Vec<WaypointEntry>,
    /// Route end coordinate (`lat, lon` as free text).
    #[serde(skip)]
    pub end: WaypointEntry,
    /// Simulation velocity in km/h (stored as text to allow free typing).
    #[serde(skip)]
    pub velocity: String,

    /// HTTP tile fetcher for the OSM map widget (not persisted).
    #[serde(skip)]
    pub map_tiles: Option<walkers::HttpTiles>,
    /// Map pan/zoom state (not persisted).
    #[serde(skip)]
    pub map_memory: walkers::MapMemory,
    /// Most recent click on the map, pending user action (not persisted).
    #[serde(skip)]
    pub map_clicked: Option<crate::map_plugin::ClickResult>,

    /// HTTP tile fetcher for the waypoint-manager map (not persisted).
    #[serde(skip)]
    pub wp_map_tiles: Option<walkers::HttpTiles>,
    /// Map pan/zoom state for the waypoint manager (not persisted).
    #[serde(skip)]
    pub wp_map_memory: walkers::MapMemory,
    /// Most recent click on the waypoint map (not persisted).
    #[serde(skip)]
    pub wp_map_clicked: Option<crate::map_plugin::ClickResult>,
    /// Index into `waypoints` of the currently selected table row (not persisted).
    #[serde(skip)]
    pub wp_selected_row: Option<usize>,

    /// Status of the background pipeline task (not persisted).
    #[serde(skip)]
    pub status: AppStatus,
    /// Tokio runtime used to spawn the pipeline task (not persisted).
    #[serde(skip)]
    pub rt: tokio::runtime::Runtime,
    /// Receives the pipeline result from the background task (not persisted).
    #[serde(skip)]
    pub result_rx: mpsc::Receiver<Result<usize, String>>,
    /// Sender cloned into the background task to deliver its result (not persisted).
    #[serde(skip)]
    pub result_tx: mpsc::Sender<Result<usize, String>>,

    // ── GPS Simulator ─────────────────────────────────────────────────────────
    /// Active tab on the GPS Simulator page (not persisted).
    #[serde(skip)]
    pub sim_tab: SimTab,

    /// Path to the RINEX navigation file (not persisted).
    #[serde(skip)]
    pub sim_rinex_path: Option<PathBuf>,

    /// Path to the user-motion CSV file (not persisted).
    #[serde(skip)]
    pub sim_motion_path: Option<PathBuf>,

    /// Pending RINEX file-dialog receiver (not persisted).
    #[serde(skip)]
    pub sim_rinex_dialog: Option<mpsc::Receiver<Option<PathBuf>>>,

    /// Pending motion-file dialog receiver (not persisted).
    #[serde(skip)]
    pub sim_motion_dialog: Option<mpsc::Receiver<Option<PathBuf>>>,

    /// `HackRF` TX VGA gain in dB (0–47, not persisted).
    #[serde(skip)]
    pub sim_txvga_gain: u16,

    /// Whether to enable the `HackRF` RF amplifier (not persisted).
    #[serde(skip)]
    pub sim_amp_enable: bool,

    /// Baseband sample rate in Hz (not persisted).
    #[serde(skip)]
    pub sim_frequency: usize,

    /// Shared simulation state polled by the UI (not persisted).
    #[serde(skip)]
    pub sim_state: std::sync::Arc<std::sync::Mutex<crate::simulator::SimState>>,

    /// Flag set by the UI to request the simulation to stop (not persisted).
    #[serde(skip)]
    pub sim_stop_flag: std::sync::Arc<std::sync::atomic::AtomicBool>,

    /// Handle to the simulation worker thread (not persisted).
    #[serde(skip)]
    pub sim_thread: Option<std::thread::JoinHandle<()>>,

    /// Receives the result of a background RINEX download task (not persisted).
    #[serde(skip)]
    pub sim_rinex_download: Option<mpsc::Receiver<Result<PathBuf, String>>>,

    /// Human-readable error from the last failed RINEX download (not persisted).
    #[serde(skip)]
    pub sim_rinex_dl_error: Option<String>,

    // ── Draw Route (ManageUmfRoutes page) ─────────────────────────────────────
    /// Polyline points added by clicking on the draw-route map (not persisted).
    #[serde(skip)]
    pub draw_route_points: Vec<walkers::Position>,
    /// HTTP tile fetcher for the draw-route map (not persisted).
    #[serde(skip)]
    pub draw_map_tiles: Option<walkers::HttpTiles>,
    /// Pan/zoom state for the draw-route map (not persisted).
    #[serde(skip)]
    pub draw_map_memory: walkers::MapMemory,
    /// Most recent click on the draw-route map, pending insertion (not persisted).
    #[serde(skip)]
    pub draw_map_clicked: Option<crate::map_plugin::ClickResult>,
    /// Error from the last "Use Route" save attempt (not persisted).
    #[serde(skip)]
    pub draw_route_status: Option<String>,
    /// Pending file-dialog receiver for `GPX`/`KML` import (not persisted).
    #[serde(skip)]
    pub draw_import_dialog: Option<std::sync::mpsc::Receiver<Option<std::path::PathBuf>>>,

    // ── ORS API key dialog ────────────────────────────────────────────────────
    /// Stored ORS API key — persisted by eframe in the OS app-data directory,
    /// never in the repository.
    pub ors_api_key: String,
    /// Whether the "Set ORS API Key" dialog is open (not persisted).
    #[serde(skip)]
    pub ors_key_dialog_open: bool,
    /// Current text in the API key input field (not persisted).
    #[serde(skip)]
    pub ors_key_input: String,
    /// Whether the key is shown as plain text or obscured (not persisted).
    #[serde(skip)]
    pub ors_key_show: bool,
}

impl Default for MyApp {
    fn default() -> Self {
        let (result_tx, result_rx) = mpsc::channel::<Result<usize, String>>();
        Self {
            current_mode: AppPage::Home,
            waypoints: Vec::new(),
            waypoints_loaded: false,
            new_waypoint: Waypoint::default(),
            filter_text: String::new(),
            sort_column: None,
            sort_ascending: true,
            editing_index: None,
            new_waypoint_coords: String::new(),
            new_waypoint_coord_error: None,
            route_name: String::new(),
            ors_profile: "foot-walking".to_owned(),
            ors_optimized: true,
            route_source: RouteSource::OrsApi,
            route_geojson_path: None,
            route_geojson_dialog: None,
            start: WaypointEntry::default(),
            viapoints: Vec::new(),
            end: WaypointEntry::default(),
            velocity: "3.0".to_owned(),
            map_tiles: None,
            map_memory: walkers::MapMemory::default(),
            map_clicked: None,
            wp_map_tiles: None,
            wp_map_memory: walkers::MapMemory::default(),
            wp_map_clicked: None,
            wp_selected_row: None,
            status: AppStatus::Idle,
            rt: tokio::runtime::Runtime::new().expect("Failed to create Tokio runtime"),
            result_rx,
            result_tx,
            sim_tab: SimTab::Dynamic,
            sim_rinex_path: crate::rinex::today_rinex_path().filter(|p| p.exists()),
            sim_motion_path: None,
            sim_rinex_dialog: None,
            sim_motion_dialog: None,
            sim_txvga_gain: 20,
            sim_amp_enable: false,
            sim_frequency: 2_600_000,
            sim_state: std::sync::Arc::new(std::sync::Mutex::new(
                crate::simulator::SimState::default(),
            )),
            sim_stop_flag: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
            sim_thread: None,
            sim_rinex_download: None,
            sim_rinex_dl_error: None,
            draw_route_points: Vec::new(),
            draw_map_tiles: None,
            draw_map_memory: walkers::MapMemory::default(),
            draw_map_clicked: None,
            draw_route_status: None,
            draw_import_dialog: None,
            ors_api_key: String::new(),
            ors_key_dialog_open: false,
            ors_key_input: String::new(),
            ors_key_show: false,
        }
    }
}

impl MyApp {
    /// Called once by eframe before the first frame.
    /// Restores persisted state when available.
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        if let Some(storage) = cc.storage {
            eframe::get_value(storage, eframe::APP_KEY).unwrap_or_default()
        } else {
            Self::default()
        }
    }

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
        let optimized = self.ors_optimized;
        self.status = AppStatus::Working;
        let tx = self.result_tx.clone();
        self.rt.spawn(async move {
            let result =
                run_pipeline(route_points, velocity, route_name, api_key, profile, optimized)
                    .await;
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
            let result =
                crate::route::run_pipeline_from_geojson(path, velocity, route_name).await;
            tx.send(result).ok();
        });
    }

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
        self.new_waypoint_coords =
            format!("{}, {}", self.waypoints[index].lat, self.waypoints[index].lon);
        self.new_waypoint_coord_error = None;
    }

    /// Removes the waypoint at `index`.
    pub fn delete_waypoint(&mut self, index: usize) {
        self.waypoints.remove(index);
    }

    /// Spawns an async task that downloads today's RINEX nav file from CDDIS.
    ///
    /// The result is delivered via `sim_rinex_download`; the UI polls it each
    /// frame and updates `sim_rinex_path` on success.
    pub fn download_rinex(&mut self) {
        let (tx, rx) = mpsc::channel();
        self.sim_rinex_download = Some(rx);
        self.sim_rinex_dl_error = None;
        self.rt.spawn(async move {
            let result = crate::rinex::download_today_rinex().await;
            tx.send(result).ok();
        });
    }

    /// Spawns the simulation worker thread.
    ///
    /// Resets shared state, configures settings from current UI values, and
    /// spawns a thread that drives the GPS signal generator and `HackRF` device.
    pub fn start_simulation(&mut self) {
        use std::sync::atomic::Ordering;

        #[expect(
            clippy::unwrap_used,
            reason = "mutex poison means a prior panic; reset is best-effort"
        )]
        {
            *self.sim_state.lock().unwrap() = crate::simulator::SimState {
                status: crate::simulator::SimStatus::Running,
                ..crate::simulator::SimState::default()
            };
        }
        self.sim_stop_flag.store(false, Ordering::Relaxed);

        let rinex_path = self
            .sim_rinex_path
            .clone()
            .expect("start_simulation requires sim_rinex_path; caller must check");
        let motion_path = self
            .sim_motion_path
            .clone()
            .expect("start_simulation requires sim_motion_path; caller must check");

        let settings = crate::simulator::SimSettings {
            frequency: self.sim_frequency,
            txvga_gain: self.sim_txvga_gain,
            amp_enable: self.sim_amp_enable,
        };
        let state = std::sync::Arc::clone(&self.sim_state);
        let stop = std::sync::Arc::clone(&self.sim_stop_flag);

        self.sim_thread = Some(std::thread::spawn(move || {
            crate::simulator::run(&rinex_path, &motion_path, &settings, &state, &stop);
        }));
    }
}

impl eframe::App for MyApp {
    /// Persists app state before shutdown.
    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        eframe::set_value(storage, eframe::APP_KEY, self);
    }

    /// Auto-saves waypoints to `waypoint.json` on exit, but only if the
    /// `ManageWaypoints` page was visited this session (guarded by `waypoints_loaded`).
    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        if self.waypoints_loaded {
            self.save_waypoints();
        }
    }

    /// Called every frame to render the UI.
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        ui::update(self, ctx);
    }
}
