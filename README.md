# Gui SDR GPS Simulator

A cross-platform desktop application for generating and transmitting **GPS L1 C/A signals** via a [HackRF One](https://greatscottgadgets.com/hackrf/) software-defined radio, and for creating the UMF user-motion files those simulations require.

Built with [egui](https://github.com/emilk/egui) / [eframe](https://github.com/emilk/egui) in Rust. Runs natively on Windows, Linux, and macOS, and compiles to WebAssembly for use in a browser.

---

## Features

| Feature | Description |
|---|---|
| **GPS Simulator** | Transmit a GPS L1 C/A signal from a static position or follow a pre-recorded UMF motion file through space |
| **Dynamic Mode** | Feed a UMF route CSV to the simulator; watch the current position move along the route on a live map |
| **Static Mode** | Hold a fixed geographic coordinate; optionally loop the signal for a configurable duration |
| **Create UMF Route** | Generate a UMF user-motion CSV from four different sources (see below) |
| **Manage Waypoints** | Store, filter, and organise named geographic coordinates for reuse as route endpoints |
| **Manage UMF Routes** | Browse, preview, edit, and delete the route library; open any route in the interactive editor |

### Route sources

- **OpenRouteService API** — fetch a real walking/cycling/driving route between waypoints
- **GeoJSON file** — import any LineString GeoJSON from disk
- **Draw on map** — click to place vertices interactively; drag to reposition; click near a segment to insert a point
- **GPX / KML import** — load a track from a GPS device or mapping tool

---

## Requirements

### Hardware

- **[HackRF One](https://greatscottgadgets.com/hackrf/)** — required for actual RF transmission
- The HackRF drivers must be installed and the device must be accessible without root / administrator privileges

### Software

- **Rust toolchain 1.88** — pinned via `rust-toolchain` (installed automatically by `rustup`)
- For route generation via API: a free [OpenRouteService API key](https://openrouteservice.org/dev/#/signup)
- For WASM builds: [Trunk](https://trunkrs.dev/) (`cargo install trunk`)

> **Legal note:** Transmitting GPS signals without authorisation is regulated or prohibited in most jurisdictions. Only use this software in a shielded environment or with the appropriate licences. The authors accept no liability for misuse.

---

## Getting started

```bash
# Clone the repository
git clone https://github.com/okiedocus/gui_sdr_gps_sim
cd gui_sdr_gps_sim

# Build and run (Rust 1.88 is selected automatically via rust-toolchain)
cargo run
```

The window opens at **1100 × 750 px**. All pages scroll vertically if the content is taller than the window.

---

## Usage guide

### 1 — GPS Simulator

Navigate to **GPS Simulator** in the left sidebar.

#### Dynamic Mode (motion file)

1. **RINEX nav file** — click *Browse* to select a `.nav` / `.n` broadcast ephemeris file, or click *Download Today's RINEX* to fetch the current-day file automatically from [CDDIS](https://cddis.nasa.gov/) (requires internet access). Files are stored in `./Rinex_files/`.
2. **Route Library** — select a row from the table to auto-fill the UMF motion CSV path and preview the route on the map.
3. **Motion CSV** — alternatively, click *Browse* to pick any UMF `.csv` file manually.
4. Click **Start** to begin transmission. A live map shows the current position as the signal progresses along the route. Click **Stop** to halt.

#### Static Mode (fixed position)

1. Select the RINEX nav file (same as above).
2. Enter the target **Latitude**, **Longitude**, and **Altitude (m)**.
3. Optionally enable *Loop* and set a duration.
4. Click **Start Loop**.

#### Settings (shared by both modes)

| Setting | Description |
|---|---|
| Start time | `now`, a specific `YYYY/MM/DD,hh:mm:ss` UTC timestamp, or empty to use the first RINEX epoch |
| Overwrite TOC/TOE | Force all ephemeris clock/orbit epochs to match the start time (equivalent to `-T` in anywhere-sdr) |
| Disable ionospheric model | Useful for spacecraft scenarios above the ionosphere |
| Fixed gain | Hold all satellite signals at a constant level instead of computing path loss |
| TX VGA Gain | HackRF baseband TX gain, 0–47 dB |
| Sample rate | Baseband sampling frequency (≥ 1 000 000 Hz) |
| Centre frequency | Carrier frequency in Hz; default is GPS L1 C/A = 1 575 420 000 Hz |
| RF amplifier | Enables the HackRF RF pre-amplifier — use with caution |
| Baseband filter | Override automatic filter bandwidth |
| Leap seconds | Manual leap-second override (`-l` flag) |

---

### 2 — Create UMF Route

Navigate to **Create UMF Route**. Give the route a **name** and set a **velocity (km/h)**, then choose a source:

#### ORS API route
1. Enter your OpenRouteService API key (saved via *File → Set ORS API Key*).
2. Choose a profile (foot, cycling, car, etc.).
3. Enter **Start** and **End** coordinates (`lat, lon`). Optionally add one or more **Via** points.
4. The selected points are shown as coloured markers on the map (green = start, orange = via, red = end).
5. Click **Generate User Motion File**.

#### GeoJSON file
Click *Browse* and select a `.geojson` / `.json` file containing a `LineString` geometry.

#### Draw on map
- **Click** on the map to append a new waypoint.
- **Click near an existing segment** to insert a waypoint along that segment.
- **Drag** a vertex to move it.
- Use **Undo** to remove the last point, or **Clear** to start over.
- Click **Generate User Motion File** when done.

#### GPX / KML import
Click *Browse* and select a `.gpx` or `.kml` track file.

---

After generation, the route CSV and a companion GeoJSON are saved to `./umf/` and the **Route Library** is updated automatically.

---

### 3 — Manage Waypoints

Navigate to **Manage Waypoints**.

- **Add** named geographic coordinates with a description.
- **Filter** the list by typing in the search box.
- **Sort** by any column by clicking the column header (click again to reverse).
- **Edit** or **Delete** any waypoint with the row buttons.
- Waypoints can be selected as route start / end / via points on the *Create UMF Route* page.
- All waypoints are persisted to `./waypoint/`.

---

### 4 — Manage UMF Routes

Navigate to **Manage UMF Routes**. The library is scanned automatically each time the page loads.

- **Select** a row to preview the route geometry on the map.
- **Edit** — opens an interactive route editor:
  - Drag any vertex to a new position.
  - Click near a segment to insert a point there.
  - Click away from all segments to append a new endpoint.
  - Click **Done** to return to the library, or **Open in Draw Route** to transfer the edited geometry to *Create UMF Route → Draw Route* (name and velocity are pre-filled).
- **Delete** — removes the `.csv` and `.geojson` files and rescans the library.

---

## File layout

```
gui_sdr_gps_sim/
├── src/                  Rust source
│   ├── main.rs           Native entry point (window, fonts)
│   ├── app.rs            MyApp struct + AppPage enum
│   ├── ui.rs             All UI rendering
│   ├── map_plugin.rs     Custom walkers map plugins
│   ├── simulator/        GPS signal generation + HackRF I/O
│   ├── route/            ORS API client, segmentizer, pipeline
│   ├── library.rs        Route library scan + library.json
│   ├── rinex.rs          CDDIS FTPS downloader
│   ├── waypoint.rs       Waypoint persistence
│   ├── geo.rs            Coordinate maths + CSV writer
│   ├── import.rs         GPX / KML parser
│   └── paths.rs          Well-known directory helpers
├── assets/img/           Embedded UI images
├── umf/                  Generated route CSVs + GeoJSON + library.json
├── waypoint/             Persisted waypoints
├── Rinex_files/          Downloaded RINEX navigation files
├── rust-toolchain        Pins Rust 1.88
└── check.sh              Local CI script
```

---

## Building

```bash
# Debug build + run
cargo run

# Release build
cargo build --release

# WASM (requires trunk)
trunk build
```

### Running CI checks locally

```bash
bash check.sh
```

This runs `cargo check`, `cargo fmt --check`, `cargo clippy` (zero warnings), and `cargo test`.

---

## Development notes

- **Linting** is strict: `cargo clippy -- -D warnings`. Any warning is a build failure.
- Use `#[expect(lint, reason = "…")]` instead of `#[allow(lint)]`.
- Do **not** use `unwrap()` — use `?`, `if let`, or `.unwrap_or_default()`.
- Use `log::info!` / `log::warn!` instead of `println!` / `eprintln!`.
- The UI uses an **immediate-mode deferred-action pattern**: page functions return an actions struct that is applied after the egui closure, to avoid borrow conflicts.
- App state is persisted by eframe (serde). Fields that must not be restored (channels, runtime) are tagged `#[serde(skip)]`.

---

## Dependencies

| Crate | Purpose |
|---|---|
| `egui` / `eframe` | Immediate-mode GUI framework |
| `walkers` | OpenStreetMap tile widget for egui |
| `gps` | GPS L1 C/A signal generation (private fork of anywhere-sdr) |
| `libhackrf` | HackRF USB control (private fork of anywhere-sdr) |
| `reqwest` + `tokio` | Async HTTP for OpenRouteService API |
| `suppaftp` + `flate2` | Anonymous FTPS download + gzip decompress for RINEX files |
| `serde` / `serde_json` | Serialisation of app state and route library |
| `roxmltree` | GPX / KML parsing |
| `rfd` | Native file picker dialogs |
| `chrono` | UTC date for RINEX filename construction |
| `coord_transforms` | WGS-84 LLA ↔ ECEF conversion |

---

## License

Licensed under either of [Apache License 2.0](LICENSE-APACHE) or [MIT License](LICENSE-MIT) at your option.
