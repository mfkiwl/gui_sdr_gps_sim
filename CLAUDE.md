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

# Check WASM target compiles
cargo check --workspace --all-features --lib --target wasm32-unknown-unknown

# Build WASM (requires trunk: cargo install trunk)
trunk build

# Run all CI checks locally (equivalent to CI pipeline)
bash check.sh
```

## Architecture

Cross-platform desktop + WASM GUI app using [egui](https://github.com/emilk/egui) / [eframe](https://github.com/emilk/egui). Crate name: `gui_sdr_gps_sim`. Rust toolchain pinned to **1.88**, edition **2024**.

**Module layout:**

| File / dir | Responsibility |
|---|---|
| `src/main.rs` | Native entry point — window config (400×300), icon, image loaders |
| `src/lib.rs` | Module declarations; re-exports `MyApp` for the WASM build |
| `src/app.rs` | `MyApp` struct, `AppPage` / `AppStatus` enums, `Default`, `eframe::App` |
| `src/ui.rs` | All UI rendering — delegates from `eframe::App::update` |
| `src/waypoint.rs` | `Waypoint` / `WaypointEntry` types; free-fn `load_waypoints` / `save_waypoints` |
| `src/geo.rs` | `parse_coords`, `lla_to_ecef` (WGS-84), `write_transmit_points_to_csv` |
| `src/route/ors.rs` | Async HTTP client for the OpenRouteService directions API |
| `src/route/segment.rs` | `Segment` struct; `segmentize()` splits a route into GPS transmit points |
| `src/route/pipeline.rs` | `run_pipeline()` — orchestrates ORS fetch → segmentize → CSV write |
| `src/route/geojson.rs` | Serde types for the GeoJSON API response |
| `src/simulator/mod.rs` | Public API of the simulator module; also hosts `open_file_dialog()` |
| `src/simulator/state.rs` | `SimSettings`, `SimState`, `SimStatus` — shared between worker and UI |
| `src/simulator/worker.rs` | `run()` — spawns GPS signal generation loop + HackRF TX consumer thread |
| `src/rinex.rs` | Downloads today's broadcast RINEX nav file from CDDIS via anonymous FTPS |
| `src/map_plugin.rs` | `ClickCapturePlugin` / `WaypointMarkerPlugin` for the `walkers` map widget |
| `src/paths.rs` | `umf_dir()` / `waypoint_dir()` — create and return well-known working directories |
| `src/import.rs` | `load_route_file()` — parses GPX and KML files into `[lat, lon]` sequences |
| `src/library.rs` | `RouteEntry` type; scans `umf/` for CSV routes and persists metadata to `library.json` |

**Data flow for route generation:**

1. User fills start / via / end fields on the *CreateUmfRoute* page and clicks "Generate CSV".
2. `MyApp::generate()` parses inputs, then spawns `run_pipeline()` on the Tokio runtime (`self.rt`).
3. `run_pipeline()` calls `get_ors_route()` → `segmentize()` → `write_transmit_points_to_csv()`.
4. The result is sent back via `mpsc::channel` (`result_tx` / `result_rx`). `ui::update()` polls the channel each frame and updates `AppStatus`.

**Data flow for GPS simulation:**

1. User selects a RINEX nav file (or downloads today's from CDDIS via `rinex::blocking_download()` run in a `std::thread::spawn`) and a UMF motion file, then clicks "Start".
2. The UI spawns a dedicated OS thread running `simulator::run()`.
3. `worker::do_run()` builds a `gps::SignalGenerator`, opens `HackRF` on GPS L1 (1 575.42 MHz), then feeds 100 ms IQ blocks through an `mpsc::sync_channel` to a second thread that writes to HackRF via USB.
4. Progress (`current_step`, `bytes_sent`) and final status (`Done`/`Stopped`/`Error`) are communicated back via `Arc<Mutex<SimState>>`. The UI polls this each frame.
5. The user can cancel at any time via `Arc<AtomicBool>` stop flag.

**UI rendering pattern:**

`eframe::App::update` delegates immediately to `ui::update(app, ctx)`, which renders:
- `TopBottomPanel` (top) — File menu + theme toggle
- `SidePanel` (left) — logo + four clickable nav images that set `app.current_mode`
- `CentralPanel` — page content switched on `app.current_mode`

Because egui closures hold borrows, mutations triggered by button clicks are **deferred**: page functions return an actions struct (`RoutePageActions`, `WaypointPageActions`) that is applied after the closure completes.

**Persistence:**

- `MyApp` serialises via serde; eframe restores it on startup via `eframe::get_value`.
- Fields tagged `#[serde(skip)]` (`status`, `rt`, `result_rx`, `result_tx`) are re-created fresh in `Default::default()`.
- Waypoints are persisted in `./waypoint/` (created by `paths::waypoint_dir()`); UMF motion files go in `./umf/`; downloaded RINEX nav files go in `./Rinex_files/`.

**Image assets** in `assets/img/` are embedded at compile time via `egui::include_image!()`. Paths are relative to the `.rs` file containing the macro — all image macros live in `src/ui.rs`, so paths use `../assets/img/`.

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

- Native: Windows (x86_64-pc-windows-msvc), Linux (x86_64, ARM), macOS (aarch64, x86_64)
- Web: `wasm32-unknown-unknown` built with [Trunk](https://trunkrs.dev/); deployed to GitHub Pages
