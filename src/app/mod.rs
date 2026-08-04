//! Application state, initialisation, and eframe integration.
//!
//! [`MyApp`] holds every field the UI reads and writes; it is serde-persisted by
//! eframe, so the field set is deliberately kept in one place.  The behaviour is
//! split across sibling modules by area - [`routes`], [`library`], [`waypoints`],
//! and [`simulation`] each add their own `impl MyApp` block.

mod library;
mod routes;
mod simulation;
mod state;
mod waypoints;

use std::{path::PathBuf, sync::mpsc};

use crate::{
    ui,
    waypoint::{Waypoint, WaypointEntry},
};

pub use state::{AppPage, AppStatus, RouteSource, SimTab};

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

    /// Scenario start time entered by the user (not persisted).
    /// Empty string means "use ephemeris start"; "now" means current UTC time.
    #[serde(skip)]
    pub sim_start_time: String,

    /// Whether to overwrite TOC/TOE in the ephemeris to the scenario start time (not persisted).
    #[serde(skip)]
    pub sim_time_override: bool,

    /// Whether to disable ionospheric delay correction (not persisted).
    #[serde(skip)]
    pub sim_ionospheric_disable: bool,

    /// Whether to use a fixed gain instead of distance-based path loss (not persisted).
    #[serde(skip)]
    pub sim_fixed_gain_enable: bool,

    /// Fixed gain value used when `sim_fixed_gain_enable` is true (not persisted).
    /// `1.0` matches the C reference signal level; higher values overdrive the DAC.
    #[serde(skip)]
    pub sim_fixed_gain: f64,

    /// RF centre frequency in Hz (not persisted).
    #[serde(skip)]
    pub sim_center_freq: u64,

    /// Whether to override the baseband filter bandwidth instead of using auto (not persisted).
    #[serde(skip)]
    pub sim_baseband_filter_enable: bool,

    /// Manual baseband filter bandwidth in Hz (not persisted).
    #[serde(skip)]
    pub sim_baseband_filter: u32,

    /// Whether to override leap second parameters (not persisted).
    #[serde(skip)]
    pub sim_leap_enable: bool,

    /// Leap second GPS week number (not persisted).
    #[serde(skip)]
    pub sim_leap_week: i32,

    /// Leap second day of week, 1–7 (not persisted).
    #[serde(skip)]
    pub sim_leap_day: i32,

    /// Delta leap seconds, ±128 (not persisted).
    #[serde(skip)]
    pub sim_leap_delta: i32,

    /// Shared simulation state polled by the UI (not persisted).
    #[serde(skip)]
    pub sim_state: std::sync::Arc<std::sync::Mutex<crate::simulator::SimState>>,

    /// Flag set by the UI to request the simulation to stop (not persisted).
    #[serde(skip)]
    pub sim_stop_flag: std::sync::Arc<std::sync::atomic::AtomicBool>,

    /// Flag set by the UI to pause the dynamic simulation at the current route position (not persisted).
    #[serde(skip)]
    pub sim_pause_flag: std::sync::Arc<std::sync::atomic::AtomicBool>,

    /// Handle to the simulation worker thread (not persisted).
    #[serde(skip)]
    pub sim_thread: Option<std::thread::JoinHandle<()>>,

    /// Receives the result of a background RINEX download task (not persisted).
    #[serde(skip)]
    pub sim_rinex_download: Option<mpsc::Receiver<Result<PathBuf, String>>>,

    /// Human-readable error from the last failed RINEX download (not persisted).
    #[serde(skip)]
    pub sim_rinex_dl_error: Option<String>,

    // ── Static GPS Simulator ───────────────────────────────────────────────────
    /// Path to the RINEX navigation file for the static looping simulator (not persisted).
    #[serde(skip)]
    pub sim_static_rinex_path: Option<PathBuf>,

    /// Pending RINEX file-dialog receiver for the static simulator (not persisted).
    #[serde(skip)]
    pub sim_static_rinex_dialog: Option<mpsc::Receiver<Option<PathBuf>>>,

    /// WGS-84 latitude in decimal degrees (not persisted).
    #[serde(skip)]
    pub sim_static_lat: String,

    /// WGS-84 longitude in decimal degrees (not persisted).
    #[serde(skip)]
    pub sim_static_lon: String,

    /// Height above WGS-84 ellipsoid in metres (not persisted).
    #[serde(skip)]
    pub sim_static_alt: String,

    /// Duration of each loop pass in seconds (not persisted).
    #[serde(skip)]
    pub sim_static_loop_duration: f64,

    /// Shared simulation state polled by the UI for the static simulator (not persisted).
    #[serde(skip)]
    pub sim_static_state: std::sync::Arc<std::sync::Mutex<crate::simulator::SimState>>,

    /// Flag set by the UI to request the static simulation to stop (not persisted).
    #[serde(skip)]
    pub sim_static_stop_flag: std::sync::Arc<std::sync::atomic::AtomicBool>,

    /// Handle to the static simulation worker thread (not persisted).
    #[serde(skip)]
    pub sim_static_thread: Option<std::thread::JoinHandle<()>>,

    /// Receives the result of a RINEX download for the static simulator (not persisted).
    #[serde(skip)]
    pub sim_static_rinex_download: Option<mpsc::Receiver<Result<PathBuf, String>>>,

    /// Human-readable error from the last failed RINEX download for the static simulator (not persisted).
    #[serde(skip)]
    pub sim_static_rinex_dl_error: Option<String>,

    // ── Interactive GPS Simulator ──────────────────────────────────────────────
    /// Path to the RINEX navigation file for the interactive simulator (not persisted).
    #[serde(skip)]
    pub sim_interactive_rinex_path: Option<std::path::PathBuf>,

    /// Pending RINEX file-dialog receiver for the interactive simulator (not persisted).
    #[serde(skip)]
    pub sim_interactive_rinex_dialog: Option<std::sync::mpsc::Receiver<Option<std::path::PathBuf>>>,

    /// Receives the result of a RINEX download for the interactive simulator (not persisted).
    #[serde(skip)]
    pub sim_interactive_rinex_download:
        Option<std::sync::mpsc::Receiver<Result<std::path::PathBuf, String>>>,

    /// Human-readable error from the last failed RINEX download for the interactive simulator (not persisted).
    #[serde(skip)]
    pub sim_interactive_rinex_dl_error: Option<String>,

    /// WGS-84 starting latitude in decimal degrees (not persisted).
    #[serde(skip)]
    pub sim_interactive_lat: String,

    /// WGS-84 starting longitude in decimal degrees (not persisted).
    #[serde(skip)]
    pub sim_interactive_lon: String,

    /// Starting height above WGS-84 ellipsoid in metres (not persisted).
    #[serde(skip)]
    pub sim_interactive_alt: String,

    /// Shared simulation state polled by the UI for the interactive simulator (not persisted).
    #[serde(skip)]
    pub sim_interactive_state: std::sync::Arc<std::sync::Mutex<crate::simulator::SimState>>,

    /// Flag set by the UI to request the interactive simulation to stop (not persisted).
    #[serde(skip)]
    pub sim_interactive_stop_flag: std::sync::Arc<std::sync::atomic::AtomicBool>,

    /// Handle to the interactive simulation worker thread (not persisted).
    #[serde(skip)]
    pub sim_interactive_thread: Option<std::thread::JoinHandle<()>>,

    /// Shared interactive state updated by egui key events and consumed by the simulator (not persisted).
    #[serde(skip)]
    pub sim_interactive_istate: std::sync::Arc<std::sync::Mutex<crate::gps_sim::InteractiveState>>,

    /// HTTP tile fetcher for the interactive-tab map (not persisted).
    #[serde(skip)]
    pub sim_interactive_map_tiles: Option<walkers::HttpTiles>,

    /// Pan/zoom state for the interactive-tab map (not persisted).
    #[serde(skip)]
    pub sim_interactive_map_memory: walkers::MapMemory,

    /// Most recent click on the interactive-tab map (not persisted).
    #[serde(skip)]
    pub sim_interactive_map_clicked: Option<crate::map_plugin::ClickResult>,

    /// Oscillator PPB offset (not persisted).
    #[serde(skip)]
    pub sim_ppb: i32,

    /// Minimum elevation mask angle in degrees (not persisted).
    #[serde(skip)]
    pub sim_elevation_mask: f64,

    /// Space-separated or comma-separated PRNs to block, e.g. "5,12,23" (not persisted).
    #[serde(skip)]
    pub sim_blocked_prns: String,

    /// Whether to write a position log file (not persisted).
    #[serde(skip)]
    pub sim_log_enable: bool,

    /// Path for the position log CSV file (not persisted).
    #[serde(skip)]
    pub sim_log_path: String,

    /// Output sink type (not persisted).
    #[serde(skip)]
    pub sim_output_type: crate::simulator::SimOutputType,

    /// Path for IQ file output (not persisted).
    #[serde(skip)]
    pub sim_iq_file_path: String,

    /// UDP destination address for UDP output (not persisted).
    #[serde(skip)]
    pub sim_udp_addr: String,

    /// TCP server port for TCP output (not persisted).
    #[serde(skip)]
    pub sim_tcp_port: u16,

    /// Whether to include `BeiDou` B1C signals in the simulation (not persisted).
    /// GPS L1 C/A is always included.
    #[serde(skip)]
    pub sim_use_beidou: bool,

    /// Whether to include Galileo E1-B signals in the simulation (not persisted).
    /// GPS L1 C/A is always included.
    #[serde(skip)]
    pub sim_use_galileo: bool,

    // ── Static tab waypoint picker ─────────────────────────────────────────────
    /// Index of the currently selected waypoint row on the static-tab picker (not persisted).
    #[serde(skip)]
    pub sim_static_wp_selected_row: Option<usize>,

    /// HTTP tile fetcher for the static-tab waypoint map (not persisted).
    #[serde(skip)]
    pub sim_static_map_tiles: Option<walkers::HttpTiles>,

    /// Pan/zoom state for the static-tab waypoint map (not persisted).
    #[serde(skip)]
    pub sim_static_map_memory: walkers::MapMemory,

    /// Most recent click on the static-tab waypoint map (not persisted).
    #[serde(skip)]
    pub sim_static_map_clicked: Option<crate::map_plugin::ClickResult>,

    // ── Dynamic simulator route picker ────────────────────────────────────────
    /// Index of the route selected in the Dynamic Mode library table (not persisted).
    #[serde(skip)]
    pub sim_lib_selected_row: Option<usize>,
    /// Route points loaded from the selected entry's `GeoJSON` file (not persisted).
    #[serde(skip)]
    pub sim_lib_route_points: Vec<walkers::Position>,
    /// HTTP tile fetcher for the Dynamic Mode route-preview map (not persisted).
    #[serde(skip)]
    pub sim_lib_map_tiles: Option<walkers::HttpTiles>,
    /// Pan/zoom state for the Dynamic Mode route-preview map (not persisted).
    #[serde(skip)]
    pub sim_lib_map_memory: walkers::MapMemory,

    // ── Route Library (ManageUmfRoutes page) ──────────────────────────────────
    /// Routes loaded from `umf/library.json` (not persisted).
    #[serde(skip)]
    pub library: Vec<crate::library::RouteEntry>,
    /// Whether `library` has been loaded from disk this session (not persisted).
    #[serde(skip)]
    pub library_loaded: bool,
    /// Index of the selected row in the library table (not persisted).
    #[serde(skip)]
    pub library_selected_row: Option<usize>,
    /// Route points of the currently selected library entry (not persisted).
    #[serde(skip)]
    pub lib_route_points: Vec<walkers::Position>,
    /// HTTP tile fetcher for the library map (not persisted).
    #[serde(skip)]
    pub lib_map_tiles: Option<walkers::HttpTiles>,
    /// Pan/zoom state for the library map (not persisted).
    #[serde(skip)]
    pub lib_map_memory: walkers::MapMemory,

    // ── Library route editor ───────────────────────────────────────────────────
    /// Index into `library` of the route currently being edited (not persisted).
    #[serde(skip)]
    pub lib_edit_entry_idx: Option<usize>,
    /// Editable copy of the selected route's waypoints (not persisted).
    #[serde(skip)]
    pub lib_edit_points: Vec<walkers::Position>,
    /// HTTP tile fetcher for the route editor map (not persisted).
    #[serde(skip)]
    pub lib_edit_map_tiles: Option<walkers::HttpTiles>,
    /// Pan/zoom state for the route editor map (not persisted).
    #[serde(skip)]
    pub lib_edit_map_memory: walkers::MapMemory,

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
    /// Path of the last successfully imported `GPX`/`KML` file (not persisted).
    #[serde(skip)]
    pub draw_import_path: Option<std::path::PathBuf>,

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
    #[expect(
        clippy::too_many_lines,
        reason = "MyApp has many independent fields; splitting into sub-structs would obscure the flat serde layout"
    )]
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
            sim_frequency: crate::simulator::GPS_SAMPLE_RATE_HZ,
            sim_start_time: String::new(),
            sim_time_override: false,
            sim_ionospheric_disable: false,
            sim_fixed_gain_enable: false,
            sim_fixed_gain: 1.0,
            sim_center_freq: 1_575_420_000,
            sim_baseband_filter_enable: false,
            sim_baseband_filter: 1_750_000,
            sim_leap_enable: false,
            sim_leap_week: 0,
            sim_leap_day: 1,
            sim_leap_delta: 18,
            sim_state: std::sync::Arc::new(std::sync::Mutex::new(
                crate::simulator::SimState::default(),
            )),
            sim_stop_flag: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
            sim_pause_flag: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
            sim_thread: None,
            sim_rinex_download: None,
            sim_rinex_dl_error: None,
            sim_static_rinex_path: crate::rinex::today_rinex_path().filter(|p| p.exists()),
            sim_static_rinex_dialog: None,
            sim_static_lat: String::new(),
            sim_static_lon: String::new(),
            sim_static_alt: "10.0".to_owned(),
            sim_static_loop_duration: 300.0,
            sim_static_state: std::sync::Arc::new(std::sync::Mutex::new(
                crate::simulator::SimState::default(),
            )),
            sim_static_stop_flag: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
            sim_static_thread: None,
            sim_static_rinex_download: None,
            sim_static_rinex_dl_error: None,
            sim_interactive_rinex_path: crate::rinex::today_rinex_path().filter(|p| p.exists()),
            sim_interactive_rinex_dialog: None,
            sim_interactive_rinex_download: None,
            sim_interactive_rinex_dl_error: None,
            sim_interactive_lat: String::new(),
            sim_interactive_lon: String::new(),
            sim_interactive_alt: "10.0".to_owned(),
            sim_interactive_state: std::sync::Arc::new(std::sync::Mutex::new(
                crate::simulator::SimState::default(),
            )),
            sim_interactive_stop_flag: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(
                false,
            )),
            sim_interactive_thread: None,
            sim_interactive_istate: std::sync::Arc::new(std::sync::Mutex::new(
                crate::gps_sim::InteractiveState::default(),
            )),
            sim_interactive_map_tiles: None,
            sim_interactive_map_memory: walkers::MapMemory::default(),
            sim_interactive_map_clicked: None,
            sim_ppb: 0,
            sim_elevation_mask: 0.0,
            sim_blocked_prns: String::new(),
            sim_log_enable: false,
            sim_log_path: String::new(),
            sim_output_type: crate::simulator::SimOutputType::HackRf,
            sim_iq_file_path: "output.iq".to_owned(),
            sim_udp_addr: "127.0.0.1:4567".to_owned(),
            sim_tcp_port: 4567,
            sim_use_beidou: false,
            sim_use_galileo: false,
            sim_static_wp_selected_row: None,
            sim_static_map_tiles: None,
            sim_static_map_memory: walkers::MapMemory::default(),
            sim_static_map_clicked: None,
            sim_lib_selected_row: None,
            sim_lib_route_points: Vec::new(),
            sim_lib_map_tiles: None,
            sim_lib_map_memory: walkers::MapMemory::default(),
            library: Vec::new(),
            library_loaded: false,
            library_selected_row: None,
            lib_route_points: Vec::new(),
            lib_map_tiles: None,
            lib_map_memory: walkers::MapMemory::default(),
            lib_edit_entry_idx: None,
            lib_edit_points: Vec::new(),
            lib_edit_map_tiles: None,
            lib_edit_map_memory: walkers::MapMemory::default(),
            draw_route_points: Vec::new(),
            draw_map_tiles: None,
            draw_map_memory: walkers::MapMemory::default(),
            draw_map_clicked: None,
            draw_route_status: None,
            draw_import_dialog: None,
            draw_import_path: None,
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
