# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Commands

```bash
# Build and run natively
cargo run

# Build for release
cargo build --release

# Run tests
cargo test --workspace --all-targets --all-features

# Check formatting
cargo fmt --all -- --check

# Apply formatting
cargo fmt --all

# Lint (must pass with zero warnings)
cargo clippy --workspace --all-targets --all-features -- -D warnings -W clippy::all

# Run all CI checks locally (equivalent to CI pipeline)
bash check.sh
```

## Architecture

Cross-platform desktop GUI app using [egui](https://github.com/emilk/egui) / [eframe](https://github.com/emilk/egui). Crate name: `gui_sdr_gps_sim`. Rust toolchain pinned to **1.88**, edition **2024**. Default window size is **1100×750**, minimum **700×500**.

**Module layout:**

| File / dir | Responsibility |
|---|---|
| `src/main.rs` | Native entry point — window config, icon, image loaders, `setup_fonts()` (loads system symbol font as fallback for ▲▼) |
| `src/lib.rs` | Module declarations; re-exports `MyApp` |
| `src/app.rs` | `MyApp` struct, `AppPage` / `AppStatus` enums, `Default`, `eframe::App` |
| `src/ui.rs` | All UI rendering — delegates from `eframe::App::update` |
| `src/waypoint.rs` | `Waypoint` / `WaypointEntry` types; free-fn `load_waypoints` / `save_waypoints` |
| `src/geo.rs` | `parse_coords`, `lla_to_ecef` (WGS-84), `write_transmit_points_to_csv` |
| `src/route/ors.rs` | Async HTTP client for the OpenRouteService directions API |
| `src/route/segment.rs` | `Segment` struct; `segmentize()` splits a route into GPS transmit points |
| `src/route/pipeline.rs` | `run_pipeline()` — orchestrates ORS fetch → segmentize → CSV write; `run_pipeline_from_geojson()` skips the ORS call |
| `src/route/geojson.rs` | Serde types for the GeoJSON API response |
| `src/simulator/mod.rs` | Public API of the simulator module; also hosts `open_file_dialog()` |
| `src/simulator/state.rs` | `SimSettings`, `SimState`, `SimStatus` — shared between worker and UI |
| `src/simulator/worker.rs` | `run()` / `run_static_loop()` — thin wrappers that delegate to `gps_sim::Simulator` |
| `src/gps_sim/` | GPS L1 C/A baseband signal simulator. Sub-modules: `types`, `coords`, `orbit`, `ionosphere`, `troposphere`, `codegen`, `navmsg`, `rinex`, `signal`, `fifo`, `hackrf`, `channel` (private), `sim` (private). Public entry point: `Simulator::builder()` |
| `src/rinex.rs` | Downloads today's broadcast RINEX nav file from CDDIS via anonymous FTPS |
| `src/map_plugin.rs` | walkers `Plugin` impls: `ClickCapturePlugin`, `WaypointMarkerPlugin`, `RouteLinePlugin`, `EditableRoutePlugin`, `PolylinePlugin` |
| `src/paths.rs` | `umf_dir()` / `waypoint_dir()` — create and return well-known working directories |
| `src/import.rs` | `load_route_file()` — parses GPX and KML files into `[lat, lon]` sequences |
| `src/library.rs` | `RouteEntry` type; scans `umf/` for CSV routes and persists metadata to `library.json` |

**Data flow for route generation:**

1. User fills start / via / end fields on the *CreateUmfRoute* page and clicks "Generate CSV".
2. `MyApp::generate()` parses inputs, then spawns `run_pipeline()` on the Tokio runtime (`self.rt`).
3. `run_pipeline()` calls `get_ors_route()` → `segmentize()` → `write_transmit_points_to_csv()`.
4. The result is sent back via `mpsc::channel` (`result_tx` / `result_rx`). `ui::update()` polls the channel each frame, updates `AppStatus`, and auto-rescans the route library.

**Data flow for GPS simulation:**

1. User selects a RINEX nav file (or downloads today's from CDDIS via `rinex::blocking_download()` run in a `std::thread::spawn`) and a UMF motion file, then clicks "Start".
2. The UI spawns a dedicated OS thread running `simulator::worker::run()` (dynamic route) or `run_static_loop()` (fixed position).
3. `worker` calls `gps_sim::Simulator::builder()` with RINEX path, location/motion-file, output target, stop flag, and an `on_event` callback, then calls `.run()`.
4. Inside `gps_sim`, the signal chain is: RINEX → ephemeris → channel allocator (≤12 SVs) → 100 ms IQ accumulation loop → FIFO (8 × 262 KB) → TX thread → HackRF / IQ file / UDP / TCP / Null.
5. The `on_event` callback translates `SimEvent::Progress` into `SimState` updates (`current_step`, `total_steps`, `bytes_sent`). The UI polls `Arc<Mutex<SimState>>` each frame.
6. The user can cancel at any time via `Arc<AtomicBool>` stop flag passed to the simulator.
7. Static mode loops indefinitely (each pass re-creates the `Simulator`); `SimState::loop_count` tracks iterations.
8. Dynamic Mode shows a live-tracking map: `interpolate_route_pos()` in `ui.rs` derives the current geographic position from `current_step / total_steps` and centers the map on it each frame.

**`SdrOutput` variants** (defined in `gps_sim/mod.rs`): `HackRf { gain_db, amp }`, `IqFile { path }`, `Null`, `PlutoSdr { host, gain_db }`, `UdpStream { addr }`, `TcpServer { port }`.

**UI rendering pattern:**

`eframe::App::update` delegates immediately to `ui::update(app, ctx)`, which renders:
- `TopBottomPanel` (top) — File menu + theme toggle
- `SidePanel` (left) — logo (click → Home) + four `nav_image_active_with_tooltip` buttons that set `app.current_mode`
- `CentralPanel` — wraps all page content in a `ScrollArea::vertical()` (auto-scrolls when content exceeds window height), then dispatches on `app.current_mode`

Because egui closures hold borrows, mutations triggered by button clicks are **deferred**: page functions return an actions struct (`RouteLibraryActions`, `WaypointPageActions`, `RoutePageActions`) applied after the closure completes.

**UI helpers in `ui.rs`:**
- `page_heading(ui, title)` — renders a large heading + separator used at the top of every page
- `section_title(ui, text)` — bold 13 px label used for group headers within a page
- `nav_image_active_with_tooltip(ui, src, active, tooltip)` — nav button with blue left accent when active
- `home_card(ui, title, body)` — full-width info card used on the Home page
- `sortable_header_text(ui, label, col_idx, sort_col, sort_asc)` — clickable text header with ▲/▼ arrows for tables

**map_plugin.rs plugins:**
- `ClickCapturePlugin` — captures primary-click position; skips the zoom-button exclusion zone (`ZOOM_WIDGET_EXCLUSION`)
- `WaypointMarkerPlugin` — draws filled circle markers at given positions + colours
- `RouteLinePlugin` — draws a read-only red polyline
- `EditableRoutePlugin` — interactive route editor: drag vertices, click near a segment to insert a point, click away to append; uses `nearest_segment_idx` + `point_to_segment_dist` helpers
- `PolylinePlugin` — blue numbered polyline (used for waypoint routes)

**Persistence:**

- `MyApp` serialises via serde; eframe restores it on startup via `eframe::get_value`.
- Fields tagged `#[serde(skip)]` (`status`, `rt`, `result_rx`, `result_tx`) are re-created fresh in `Default::default()`.
- Waypoints persist in `./waypoint/`; UMF motion files in `./umf/`; downloaded RINEX nav files in `./Rinex_files/`.
- Route library index is `./umf/library.json` (array of `RouteEntry` with `name`, `distance_m`, `duration_s`, `velocity_kmh`).

**Image assets** in `assets/img/` are embedded at compile time via `egui::include_image!()`. All image macros live in `src/ui.rs`, so paths use `../assets/img/`.

## Linting rules

All lints live in `[workspace.lints]` in `Cargo.toml`. Key rules:

- `unsafe_code = "deny"`
- `unwrap_used`, `get_unwrap` — use `?`, `.unwrap_or_default()`, or `if let`
- `print_stdout`, `print_stderr` — use `log::` macros
- `todo` — do not leave `todo!()` in code
- `wildcard_imports` — explicit imports only
- `allow_attributes` — use `#[expect(lint, reason = "…")]` instead of `#[allow(lint)]`

Clippy runs as `-D warnings`; any new warning is a build failure. Run `cargo clippy` before finishing any change.

## Platform targets

- Windows (x86_64-pc-windows-msvc), Linux (x86_64, ARM), macOS (aarch64, x86_64)
