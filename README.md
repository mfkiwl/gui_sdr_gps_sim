This project is a work in progress. Please test it and give your feedback!

# GUI SDR GPS Simulator

A cross-platform desktop application that generates real **GPS L1 C/A baseband signals** and transmits them through a software-defined radio. It also provides tools to create and manage the route files (UMF motion files) that drive those simulations.

Built with [egui](https://github.com/emilk/egui) / [eframe](https://github.com/emilk/egui) in Rust. Runs natively on **Windows, Linux, and macOS**.

> **Legal note:** Transmitting GPS signals without authorisation is regulated or prohibited in most jurisdictions. Use this software only in a shielded enclosure or with the appropriate licences. The authors accept no liability for misuse.

---

## Table of contents

1. [What this application does](#what-this-application-does)
2. [Hardware requirements](#hardware-requirements)
3. [Software requirements](#software-requirements)
4. [Getting started](#getting-started)
5. [Page 1 — GPS Simulator](#page-1--gps-simulator)
   - [Dynamic mode](#dynamic-mode-follow-a-route)
   - [Static mode](#static-mode-fixed-position)
   - [Interactive mode](#interactive-mode-keyboard-steered)
   - [Simulator settings](#simulator-settings)
   - [SDR output options](#sdr-output-options)
6. [Page 2 — Create UMF Route](#page-2--create-umf-route)
   - [ORS API](#route-source-1-openrouteservice-api)
   - [GeoJSON file](#route-source-2-geojson-file)
   - [Draw on map](#route-source-3-draw-on-map)
   - [GPX / KML import](#route-source-4-gpx--kml-import)
7. [Page 3 — Manage Waypoints](#page-3--manage-waypoints)
8. [Page 4 — Manage UMF Routes](#page-4--manage-umf-routes)
9. [GNU Radio flow graphs](#gnu-radio-flow-graphs)
10. [File layout](#file-layout)
11. [Building](#building)
12. [Dependencies](#dependencies)

---

## What this application does

The app has two main jobs:

1. **Simulate GPS** — generate a mathematically accurate GPS L1 C/A baseband IQ signal for a moving or stationary receiver and deliver it to an SDR transmitter (or to a file / network stream).
2. **Create routes** — produce the UMF user-motion CSV files that describe a receiver's trajectory through space. Routes can come from an online directions API, a drawn polyline, imported GPX/KML tracks, or a GeoJSON file.

The generated signal contains a full satellite constellation (up to 12 SVs), orbital mechanics, ionospheric / tropospheric delay models, and Doppler shifts — everything a real GPS receiver expects to see.

---

## Hardware requirements

| Item | Role |
|---|---|
| **[HackRF One](https://greatscottgadgets.com/hackrf/)** | SDR transmitter — sends the GPS IQ signal over RF |
| (optional) Second HackRF or SDR receiver | Verify the transmitted signal with the GNU Radio analyzer |

The HackRF driver must be installed and accessible without root/administrator privileges.
- **Windows:** replace the HackRF USB driver with WinUSB via [Zadig](https://zadig.akeo.ie/).
- **Linux:** add a udev rule so the device is accessible without `sudo`.

---

## Software requirements

| Requirement | Notes |
|---|---|
| **Rust 1.88** | Pinned via `rust-toolchain`; installed automatically by `rustup` |
| **OpenRouteService API key** | Free at [openrouteservice.org](https://openrouteservice.org/dev/#/signup) — only needed for the ORS route source |

---

## Getting started

```bash
# 1. Clone the repository
git clone https://github.com/okiedocus/gui_sdr_gps_sim
cd gui_sdr_gps_sim

# 2. Build and run (Rust 1.88 is picked up automatically from rust-toolchain)
cargo run
```

The window opens at **1100 × 750 px** and is resizable (minimum 700 × 500 px). Every page scrolls vertically when content exceeds the window height.

On first run:
- Set your OpenRouteService API key via **File → Set ORS API Key** (only needed for the ORS route source).
- Download a RINEX navigation file on the Simulator page before starting a simulation.

---

## Page 1 — GPS Simulator

Navigate to the **GPS Simulator** page using the left sidebar.

The page has four tabs: **Dynamic**, **Static**, **Interactive**, and **Settings**.

---

### Dynamic mode (follow a route)

The receiver moves along a pre-recorded UMF motion file at the speed defined when the route was created.

**Steps:**

1. **RINEX navigation file** — click *Browse* to pick a broadcast ephemeris file (`.nav` / `.n` / `.rnx`), or click *Download Today's RINEX* to fetch the current-day file automatically from [CDDIS / NASA](https://cddis.nasa.gov/). Files are stored in `./Rinex_files/`.

2. **Route** — either:
   - Click a row in the **Route Library** table to auto-fill the motion CSV path and show the route on the map, or
   - Click *Browse* next to *Motion CSV* to pick any UMF `.csv` file manually.

3. Click **Start**. A **live map** shows the current position moving along the route as the simulation progresses. A progress bar and bytes-sent counter update in real time.

4. Click **Stop** to cancel at any time.

**What you see while running:**
- Live position marker on the map, updated every frame.
- Progress bar showing how far through the route the simulation is.
- Cumulative bytes transferred to the output sink.
- Satellite sky plot (azimuth / elevation of all visible SVs, updated once per second).

---

### Static mode (fixed position)

The receiver holds a constant geographic coordinate. Useful for testing that a GPS device can get a fix at a known location.

**Steps:**

1. Select the RINEX navigation file (same as Dynamic mode).
2. Enter **Latitude** and **Longitude** in decimal degrees, and **Altitude** in metres above the WGS-84 ellipsoid.
3. Optionally enable **Loop** and set a loop duration — the simulation restarts automatically at the end of each pass, incrementing a loop counter.
4. Click **Start Loop**.

---

### Interactive mode (keyboard-steered)

Move the simulated receiver position in real time using arrow keys or WASD while the simulation is running. Useful for interactive demonstrations.

**Steps:**

1. Select the RINEX navigation file.
2. Set the starting **Latitude**, **Longitude**, and **Altitude**.
3. Set the **Step size** (metres per key press) and **Heading** (degrees).
4. Click **Start**. Use the on-screen controls or keyboard to steer the position.

---

### Simulator settings

The **Settings** tab applies to all three simulation modes. Changes take effect the next time a simulation is started.

| Setting | Description |
|---|---|
| **Output type** | Where IQ samples are sent — see [SDR output options](#sdr-output-options) below |
| **TX VGA Gain (dB)** | HackRF baseband transmit gain, 0–47 dB. Start at 20 dB. |
| **RF Amplifier** | Enables the HackRF's built-in +14 dB RF pre-amp. Use only when needed (cable losses, long runs). |
| **Sample rate (Hz)** | Baseband sampling frequency — must be ≥ 1 000 000. Default 3 000 000 (3 MSPS). |
| **Centre frequency (Hz)** | Carrier frequency transmitted by the HackRF. Default is GPS L1 C/A = **1 575 420 000 Hz**. |
| **Baseband filter (Hz)** | Override automatic filter bandwidth. Leave blank for automatic selection. |
| **Start time** | `now`, a `YYYY/MM/DD,hh:mm:ss` UTC timestamp, or blank to use the first epoch in the RINEX file. |
| **Overwrite TOC/TOE** | Force all ephemeris clock/orbit epochs to match the start time (removes epoch-validity warnings). |
| **Disable ionospheric model** | Skip ionospheric delay computation — useful for spacecraft above the ionosphere. |
| **Fixed gain** | Hold all satellite signal levels at a constant value instead of computing free-space path loss. |
| **Elevation mask (°)** | Ignore satellites below this elevation angle. 0 = no mask. |
| **Block PRNs** | Comma-separated list of satellite PRN numbers (1–32) to exclude from the simulation. |
| **Oscillator offset (ppb)** | Simulate receiver clock frequency offset in parts-per-billion. |
| **Leap seconds** | Manual override: GPS week, day of week (1–7), and delta leap seconds. |
| **Position log** | Write a CSV log of the simulated receiver position to the specified file path. |

---

### SDR output options

Select the output sink in the **Settings** tab.

| Output | Description |
|---|---|
| **HackRF** | Transmit via a connected HackRF One (default). Uses the TX VGA gain and amp settings. |
| **IQ File** | Write raw signed 8-bit interleaved IQ samples (`[I0, Q0, I1, Q1, …]`) to a binary file. |
| **UDP stream** | Stream IQ bytes to a UDP destination address (e.g. `127.0.0.1:4567`). Datagrams are 32 768 bytes each. |
| **TCP server** | Open a TCP server on the specified port. The app waits for a client to connect, then streams IQ bytes continuously. |
| **Null (discard)** | Generate the signal but discard all output. Useful for benchmarking or testing without hardware. |

> The UDP and TCP outputs are intended to be consumed by the included [GNU Radio flow graphs](#gnu-radio-flow-graphs).

---

## Page 2 — Create UMF Route

Navigate to **Create UMF Route** using the left sidebar.

A UMF (User Motion File) is a CSV with one transmit position every 100 ms. The app generates it from a geographic route and a target speed.

**Before generating:**
1. Enter a **Route name** — this becomes the filename (`{name}.csv` and `{name}.geojson`).
2. Enter a **Velocity (km/h)** — the speed at which the simulated receiver moves along the route.
3. Choose one of the four route sources below.

---

### Route source 1: OpenRouteService API

Fetches a realistic road/path route from the free [OpenRouteService](https://openrouteservice.org/) directions API.

**Steps:**
1. Enter your ORS API key (or set it once via *File → Set ORS API Key*).
2. Choose a **profile**: foot-walking, cycling-regular, driving-car, etc.
3. Enter **Start** coordinates (`lat, lon`). Click the field label to pick from your Waypoints list.
4. (Optional) Add one or more **Via** points.
5. Enter **End** coordinates.
6. The map shows green (start), orange (via), and red (end) markers as you type.
7. Click **Generate User Motion File**.

The app calls the ORS API, receives a GeoJSON LineString, converts it to transmit points at the configured velocity, and saves the CSV.

---

### Route source 2: GeoJSON file

Load a pre-existing route from a `.geojson` or `.json` file containing a `LineString` or `MultiLineString` geometry.

**Steps:**
1. Select **GeoJSON File** as the route source.
2. Click *Browse* and open the file.
3. The route is shown on the map.
4. Click **Generate User Motion File**.

---

### Route source 3: Draw on map

Draw a route interactively by clicking on the map.

**Controls:**
| Action | Result |
|---|---|
| Click on the map | Append a new endpoint |
| Click near an existing line segment | Insert a new vertex at that position |
| Drag a vertex | Move it to a new position |
| **Undo** button | Remove the last point |
| **Clear** button | Remove all points and start over |
| *Browse* (import) | Load a GPX or KML file as the starting shape, then continue editing |

Click **Generate User Motion File** when the route looks correct.

---

### Route source 4: GPX / KML import

Import a route recorded by a GPS device or exported from a mapping tool.

**Steps:**
1. Select **Import GPX/KML** as the route source.
2. Click *Browse* and open a `.gpx` or `.kml` file.
3. The track is shown on the map.
4. Click **Generate User Motion File**.

---

### After generating

The route is saved to `./umf/`:
- `{name}.csv` — the UMF motion file used by the simulator
- `{name}.geojson` — the route geometry for map display

The **Route Library** on the Simulator page is updated automatically so the new route is immediately available.

---

## Page 3 — Manage Waypoints

Navigate to **Manage Waypoints** using the left sidebar.

Waypoints are named geographic coordinates that can be quickly selected as start, via, or end points when creating routes.

**Features:**

| Action | How |
|---|---|
| **Add waypoint** | Fill in name, description, and coordinates (`lat, lon`) in the form and click *Add* |
| **Pick on map** | Click a location on the map to fill in the coordinates automatically |
| **Edit** | Click the *Edit* button on any row — the form re-opens with the existing values |
| **Delete** | Click the *Delete* button on any row |
| **Filter** | Type in the search box to filter the list by name or description |
| **Sort** | Click any column header to sort by that column; click again to reverse |

All waypoints are persisted to `./waypoint/` and survive app restarts.

---

## Page 4 — Manage UMF Routes

Navigate to **Manage UMF Routes** using the left sidebar.

Displays all route CSV files found in `./umf/` with their distance, duration, and velocity metadata.

**Features:**

| Action | How |
|---|---|
| **Preview** | Click any row to display the route geometry on the map |
| **Edit route** | Click the *Edit* button — opens a full interactive editor |
| **Delete** | Click the *Delete* button — removes the `.csv` and `.geojson` files and refreshes the list |
| **Rescan** | The library is rescanned automatically each time the page loads |

**Interactive route editor:**
- Drag any vertex to move it.
- Click near a line segment to insert a new point there.
- Click away from all segments to append a new endpoint.
- Click **Done** to save and return to the library.
- Click **Open in Draw Route** to transfer the geometry to the *Create UMF Route → Draw on map* editor (name and velocity are pre-filled).

---

## GNU Radio flow graphs

The `gnuradio/` folder contains flow graphs for receiving and re-transmitting the IQ stream with GNU Radio.

### `gps_udp_to_hackrf_simple` — minimal UDP → HackRF

The simplest possible bridge: receives the app's UDP IQ stream and retransmits it immediately via a HackRF One. No visualisation.

```
[gui_sdr_gps_sim app]
  UDP output → 127.0.0.1:4567
      │
[network_udp_source]          receive 32 768-byte datagrams
      │
[interleaved_char_to_complex  ×1/128]   i8 → complex float ±1.0
      │
[osmosdr_sink  hackrf=0]      TX @ 1575.42 MHz / 3 MSPS
```

**Usage:**
```bash
python gnuradio/gps_udp_to_hackrf_simple.py
# or open in GNU Radio Companion:
gnuradio-companion gnuradio/gps_udp_to_hackrf_simple.grc
```

Set the app output to **UDP** → `127.0.0.1:4567`. Start the GNU Radio script first, then start the simulation.

---

### `gps_network_to_hackrf` — network stream → visualize → HackRF TX

Full-featured flow graph: receives the IQ stream (UDP or TCP), shows live spectrum/waterfall/time/constellation, and simultaneously retransmits via HackRF.

```
[gui_sdr_gps_sim app]
  TCP or UDP output
      │
[network_tcp_source / network_udp_source]
      │
[interleaved_char_to_complex  ×1/128]
      │
      ├──► [qtgui_freq_sink_c]        Tab 0 — FFT spectrum
      ├──► [qtgui_waterfall_sink_c]   Tab 1 — Waterfall
      ├──► [keep_one_in_n ×100]
      │        └──► [qtgui_time_sink_c]   Tab 2 — Time domain
      ├──► [qtgui_const_sink_c]       Tab 3 — Constellation diagram
      └──► [osmosdr_sink  hackrf=0]   HackRF TX @ 1575.42 MHz / 3 MSPS
```

**Usage:**
```bash
python gnuradio/gps_network_to_hackrf.py
gnuradio-companion gnuradio/gps_network_to_hackrf.grc
```

TCP is the recommended mode (no packet loss, no datagram alignment issues). Set the app output to **TCP**, configure a port (e.g. `4568`), start the app simulation first (the app is the TCP server), then start the GNU Radio script.

---

### `gps_l1_analyzer` — HackRF RX spectrum analyzer

Standalone receiver flow graph. Uses a **second HackRF in RX mode** to verify the transmitted signal over the air or through a cable + attenuator.

```bash
python gnuradio/gps_l1_analyzer.py
gnuradio-companion gnuradio/gps_l1_analyzer.grc
```

---

### IQ wire format

| Field | Value |
|---|---|
| Encoding | Signed 8-bit integers (`i8`), interleaved `[I0, Q0, I1, Q1, …]` |
| Sample rate | 3 000 000 sps |
| Centre frequency | 1 575 420 000 Hz (GPS L1 C/A) |
| UDP datagram size | 32 768 bytes = 16 384 complex samples |
| TCP | Continuous byte stream, no framing |
| GNU Radio conversion | `interleaved_char_to_complex(scale=1/128)` → complex float ±1.0 |

### GNU Radio requirements

```bash
# Ubuntu / Debian
sudo apt install gnuradio gr-osmosdr python3-pyqt5

# Arch
sudo pacman -S gnuradio gr-osmosdr python-pyqt5

# Windows: install GNU Radio via the official installer (includes gr-osmosdr)
# HackRF driver: replace with WinUSB via Zadig  https://zadig.akeo.ie/
```

GNU Radio 3.10+ · Python 3 · PyQt5

---

## File layout

```
gui_sdr_gps_sim/
├── src/
│   ├── main.rs              Native entry point (window, fonts, icon)
│   ├── app.rs               MyApp struct, page/tab enums, application state
│   ├── ui.rs                All UI rendering (every page and panel)
│   ├── map_plugin.rs        Custom walkers map plugins (markers, polylines, editor)
│   ├── simulator/
│   │   ├── mod.rs           Public simulator API, open_file_dialog()
│   │   ├── state.rs         SimSettings, SimState, SimStatus, SimOutputType
│   │   └── worker.rs        Background thread: run() / run_static_loop()
│   ├── gps_sim/             GPS L1 C/A baseband signal engine (native only)
│   │   ├── types, coords    WGS-84 types and coordinate conversions
│   │   ├── orbit            Satellite orbit propagation (Keplerian + perturbations)
│   │   ├── ionosphere       Klobuchar ionospheric delay model
│   │   ├── troposphere      Hopfield tropospheric delay model
│   │   ├── codegen          C/A code generation (Gold codes, PRN 1–37)
│   │   ├── navmsg           GPS navigation message generation
│   │   ├── rinex            RINEX 2/3 broadcast ephemeris parser
│   │   ├── signal           IQ sample accumulation (100 ms blocks)
│   │   ├── fifo             8 × 262 KB lock-free FIFO between generator and TX
│   │   ├── hackrf           HackRF USB TX thread (libhackrf bindings)
│   │   └── sim              Top-level Simulator::builder() / run()
│   ├── route/
│   │   ├── ors.rs           Async HTTP client for OpenRouteService API
│   │   ├── segment.rs       segmentize() — route → 100 ms transmit points
│   │   ├── pipeline.rs      run_pipeline() / run_pipeline_from_geojson()
│   │   └── geojson.rs       Serde types for ORS GeoJSON response
│   ├── library.rs           Route library scan + library.json
│   ├── rinex.rs             CDDIS anonymous FTPS downloader
│   ├── waypoint.rs          Waypoint persistence (load / save)
│   ├── geo.rs               Coordinate maths + CSV writer (WGS-84 LLA → ECEF)
│   ├── import.rs            GPX / KML parser
│   └── paths.rs             umf_dir() / waypoint_dir() helpers
├── gnuradio/
│   ├── gps_udp_to_hackrf_simple.grc/.py    Minimal UDP → HackRF bridge
│   ├── gps_network_to_hackrf.grc/.py       Full network → visualize → HackRF
│   ├── gps_l1_analyzer.grc/.py             HackRF RX spectrum analyzer
│   └── README.md                           GNU Radio usage guide
├── assets/img/              Embedded UI images (compiled into the binary)
├── umf/                     Generated route CSVs, GeoJSON files, library.json
├── waypoint/                Persisted waypoint files
├── Rinex_files/             Downloaded RINEX navigation files
├── rust-toolchain           Pins Rust 1.88
└── check.sh                 Local CI script (fmt + clippy + test)
```

---

## Building

```bash
# Debug build + run
cargo run

# Release build
cargo build --release

# Run all CI checks (format, lint, tests)
bash check.sh
```

### Individual CI steps

```bash
cargo fmt --all -- --check       # formatting
cargo clippy --workspace --all-targets --all-features -- -D warnings   # lint (zero warnings)
cargo test --workspace --all-targets --all-features                     # tests
```

---

## Dependencies

| Crate | Purpose |
|---|---|
| `egui` / `eframe` | Immediate-mode GUI framework |
| `walkers` | OpenStreetMap tile map widget for egui |
| `reqwest` + `tokio` | Async HTTP for OpenRouteService API |
| `suppaftp` + `flate2` | Anonymous FTPS download + gzip decompression for RINEX files |
| `serde` / `serde_json` | App state serialisation and route library JSON |
| `roxmltree` | GPX / KML XML parsing |
| `rfd` | Native file picker dialogs |
| `chrono` | UTC date for RINEX filename construction |
| `geo` | Geodesic distance and interpolation along great-circle arcs |

---

## License

Licensed under either of [Apache License 2.0](LICENSE-APACHE) or [MIT License](LICENSE-MIT) at your option.
