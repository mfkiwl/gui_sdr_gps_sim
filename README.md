This project is a **work in progress**. It has not been thoroughly tested across all hardware combinations, operating systems, and use cases. **Your feedback is essential.** If something works, if something breaks, or if something could be better — please [open an issue](https://github.com/okiedocus/gui_sdr_gps_sim/issues) or start a [discussion](https://github.com/okiedocus/gui_sdr_gps_sim/discussions). This is meant to be a community project and every test report, bug report, and suggestion helps.

---

# GUI SDR GPS Simulator

A cross-platform desktop application for **generating and transmitting realistic GNSS signals** via a HackRF One software-defined radio. It supports **GPS L1 C/A**, **BeiDou B1C**, and **Galileo E1** — all three share the 1575.42 MHz carrier and are combined into a single IQ stream. It includes a full route creation toolset so you can simulate a receiver moving along any path in the world — a city walk, a car journey, a flight — and feed that signal to real GNSS hardware.

Built in Rust using [egui](https://github.com/emilk/egui) / [eframe](https://github.com/emilk/egui). Runs natively on **Windows, Linux, and macOS**.

> **Legal notice:** Transmitting GPS signals without authorisation is regulated or prohibited in most jurisdictions and can interfere with safety-critical systems. Only use this software in a properly shielded enclosure or with the appropriate regulatory licences. The authors accept no liability for misuse.

---

## Why this project exists

GPS simulation is useful for testing navigation hardware and software without needing to go outside, but professional GPS simulators cost thousands of euros. This project aims to provide a free, open-source alternative that runs on affordable SDR hardware (a HackRF One costs around €300) and is accessible to hobbyists, researchers, and developers.

The goal is a polished, easy-to-use desktop application — not just a command-line tool — so that people who are not RF engineers can use it too.

---

## Table of contents

1. [Features at a glance](#features-at-a-glance)
2. [Hardware requirements](#hardware-requirements)
3. [Software requirements](#software-requirements)
4. [Installation](#installation)
5. [Quick start](#quick-start)
6. [GPS Simulator](#gps-simulator)
   - [Dynamic mode](#dynamic-mode)
   - [Static mode](#static-mode)
   - [Interactive mode](#interactive-mode)
   - [Simulator settings](#simulator-settings)
   - [Constellations](#constellations)
   - [SDR output options](#sdr-output-options)
7. [Create UMF Route](#create-umf-route)
   - [OpenRouteService API](#openrouteservice-api)
   - [GeoJSON file](#geojson-file)
   - [Draw on map](#draw-on-map)
   - [GPX / KML import](#gpx--kml-import)
8. [Manage Waypoints](#manage-waypoints)
9. [Manage UMF Routes](#manage-umf-routes)
10. [GNU Radio flow graphs](#gnu-radio-flow-graphs)
11. [File layout](#file-layout)
12. [Building from source](#building-from-source)
13. [Contributing](#contributing)
14. [License](#license)

---

## Features at a glance

| Feature | Description |
|---|---|
| **Multi-constellation** | Simultaneous GPS L1 C/A, BeiDou B1C, and Galileo E1-B signals — all at 1575.42 MHz, combined in one IQ stream |
| **Dynamic simulation** | Simulate a moving receiver following a pre-recorded route at a configurable speed |
| **Static simulation** | Hold a fixed geographic position; loop indefinitely for continuous testing |
| **Interactive simulation** | Steer the receiver position in real time using keyboard controls |
| **Live map tracking** | Watch the simulated position move along the route on an OpenStreetMap map during playback |
| **Satellite sky plot** | See which satellites are visible, their azimuth and elevation, updated every second |
| **ORS route generation** | Fetch a real walking, cycling, or driving route from the OpenRouteService API |
| **Draw on map** | Click to place and drag route vertices interactively |
| **GPX / KML import** | Load tracks recorded by GPS devices or exported from mapping tools |
| **GeoJSON import** | Load any LineString GeoJSON as a route |
| **Waypoint manager** | Store, organise, filter, and reuse named geographic coordinates |
| **Route library** | Browse, preview, edit, and delete generated routes |
| **HackRF output** | Transmit directly via a connected HackRF One |
| **IQ file output** | Save raw 8-bit IQ samples to a binary file for offline use |
| **UDP / TCP streaming** | Stream IQ samples over the network for GNU Radio or other tools |
| **RINEX download** | Download today's broadcast ephemeris automatically from NASA CDDIS |
| **Advanced signal control** | Constellation selection, ionospheric model, elevation mask, PRN blocking, oscillator offset, leap seconds, fixed gain |

---

## Hardware requirements

| Item | Details |
|---|---|
| **HackRF One** | Required for RF transmission. [Great Scott Gadgets](https://greatscottgadgets.com/hackrf/) or compatible clone. |
| GPS receiver | Any device you want to test — u-blox modules, handheld GPS units, smartphones (in a shielded enclosure) |
| RF attenuator + cable | Recommended for direct connection between HackRF and GPS receiver (30–40 dB attenuation typical) |
| Shielded enclosure | Required for safe, legal use — prevents the signal leaking outside your test setup |

The HackRF driver must be accessible without administrator / root privileges:

- **Windows:** replace the HackRF USB driver with WinUSB using [Zadig](https://zadig.akeo.ie/)
- **Linux:** install a udev rule — see [HackRF documentation](https://hackrf.readthedocs.io/en/latest/getting_started_hackrf_gnuradio.html)
- **macOS:** no extra steps needed

---

## Software requirements

| Requirement | Notes |
|---|---|
| **Rust 1.88** | Installed automatically via `rustup` — the `rust-toolchain` file in the repo handles this |
| **OpenRouteService API key** | Free account at [openrouteservice.org](https://openrouteservice.org/dev/#/signup) — only needed for ORS route generation |

---

## Installation

### Pre-built binaries (recommended)

Download the latest release for your platform from the [Releases page](https://github.com/okiedocus/gui_sdr_gps_sim/releases):

| Platform | File |
|---|---|
| Windows (x86_64) | `gui_sdr_gps_sim-windows-x86_64.zip` |
| Linux (x86_64) | `gui_sdr_gps_sim-linux-x86_64.tar.gz` |
| macOS (Apple Silicon) | `gui_sdr_gps_sim-macos-aarch64.tar.gz` |
| macOS (Intel) | `gui_sdr_gps_sim-macos-x86_64.tar.gz` |

Extract and run the binary. No installer required.

**Linux:** install the required system libraries before running:
```bash
sudo apt-get install -y \
  libxcb-render0-dev libxcb-shape0-dev libxcb-xfixes0-dev \
  libxkbcommon-dev libssl-dev libudev-dev libgtk-3-dev
```

### Build from source

```bash
git clone https://github.com/okiedocus/gui_sdr_gps_sim
cd gui_sdr_gps_sim
cargo run --release
```

Rust 1.88 is selected automatically from the `rust-toolchain` file.

---

## Quick start

1. **Launch the app** and navigate to the **GPS Simulator** page using the left sidebar.
2. **Download a RINEX file** — click *Download Today's RINEX* to fetch the current broadcast ephemeris from NASA. This RINEX 3 file contains satellite orbit data for GPS, BeiDou, and Galileo.
3. **Create a route** (optional for static mode) — go to **Create UMF Route**, draw a path on the map or fetch one from the ORS API, set a speed, and click *Generate User Motion File*.
4. **Select the route** — back on the GPS Simulator page, click the route in the Route Library table.
5. **Click Start** — the simulation begins. Watch the position marker move along the route on the live map.
6. **Check your GNSS receiver** — it should start acquiring satellites and reporting a position within 30–60 seconds, depending on the device. Multi-constellation receivers may acquire faster thanks to the additional BeiDou and Galileo signals.

---

## GPS Simulator

Navigate to the **GPS Simulator** page using the left sidebar. The page has four tabs: **Dynamic**, **Static**, **Interactive**, and **Settings**.

### Dynamic mode

Simulates a receiver moving along a pre-recorded UMF motion file.

**Steps:**

1. **RINEX navigation file** — click *Browse* to select a `.nav` / `.rnx` broadcast ephemeris file, or click *Download Today's RINEX* to fetch it automatically from [NASA CDDIS](https://cddis.nasa.gov/). Files are saved to `./Rinex_files/`.

2. **Select a route** — either:
   - Click a row in the **Route Library** table to auto-fill the path and preview the route on the map, or
   - Click *Browse* next to *Motion CSV* to pick any UMF `.csv` file manually.

3. Click **Start**. The live map shows the current simulated position moving along the route. A progress bar and bytes-sent counter update in real time.

4. Click **Stop** at any time to halt the simulation cleanly.

**What you see while running:**
- Position marker on the map, updated every frame
- Progress bar (percentage through the route)
- Cumulative bytes sent to the output sink
- Satellite sky plot showing azimuth / elevation of all visible SVs, updated once per second

---

### Static mode

Holds a fixed geographic coordinate. Useful for testing that a device can get a fix at a specific known location.

**Steps:**

1. Select the RINEX navigation file.
2. Enter **Latitude** and **Longitude** in decimal degrees and **Altitude** in metres above the WGS-84 ellipsoid.
3. Optionally enable **Loop** and set a duration in seconds — the simulation restarts automatically when a pass ends, incrementing a loop counter.
4. Click **Start Loop**.

---

### Interactive mode

Move the simulated receiver position in real time while the simulation is running. Useful for live demonstrations or manual testing.

**Steps:**

1. Select the RINEX navigation file.
2. Set a starting **Latitude**, **Longitude**, and **Altitude**.
3. Set the **Step size** (metres moved per key press) and initial **Heading** (degrees).
4. Click **Start** and use the on-screen arrow controls or keyboard to steer the position.

---

### Simulator settings

The **Settings** tab applies to all three modes. Changes take effect the next time a simulation is started.

#### SDR output

| Setting | Description |
|---|---|
| **Output type** | Where to send the IQ samples — see [SDR output options](#sdr-output-options) below |
| **TX VGA Gain (dB)** | HackRF baseband TX gain, 0–47 dB. Start at 20 dB and increase if the GPS receiver doesn't acquire. |
| **RF Amplifier** | Enables the HackRF built-in +14 dB RF pre-amplifier. Use only when needed — can overdrive a directly connected receiver. |

#### Signal timing

| Setting | Description |
|---|---|
| **Start time** | `now` uses the current UTC clock. `YYYY/MM/DD,hh:mm:ss` sets a specific UTC time. Leave blank to use the first epoch in the RINEX file. |
| **Overwrite TOC/TOE** | Forces all ephemeris clock and orbit epochs to match the configured start time. Useful when using older RINEX files. |

#### RF parameters

| Setting | Description |
|---|---|
| **Sample rate (Hz)** | Baseband sampling frequency. Must be ≥ 1 000 000. Default is 3 000 000 (3 MSPS). |
| **Centre frequency (Hz)** | Carrier frequency transmitted by the HackRF. Default is GPS L1 C/A = 1 575 420 000 Hz. |
| **Baseband filter (Hz)** | Override the automatic filter bandwidth selection. Leave blank for automatic. |

#### Constellations

| Setting | Description |
|---|---|
| **GPS L1 C/A** | Always enabled. 1023-chip Gold code at 1.023 Mcps, up to 32 satellites (PRN 1–32). |
| **BeiDou B1C** | Optional. 10 230-chip Weil code at 10.23 Mcps, up to 63 satellites. Requires a RINEX 3 nav file with BeiDou (`C`) records. |
| **Galileo E1-B** | Optional. 4092-chip LFSR code at 4.092 Mcps, up to 36 satellites. Requires a RINEX 3 nav file with Galileo (`E`) records. |

All three signals share the 1575.42 MHz carrier and are summed into the same 8-bit IQ output buffer. The simulator tracks up to 24 channels in total across all enabled constellations. No changes to GNU Radio flow graphs or HackRF settings are required when enabling additional constellations.

> **Note:** BeiDou and Galileo use a GPS LNAV-style navigation message as a simulation approximation. Navigation data accuracy is sufficient for most receiver spoofing and test scenarios, but not for precision navigation research.

#### Signal modelling

| Setting | Description |
|---|---|
| **Disable ionospheric model** | Skips the Klobuchar ionospheric delay model. Useful for spacecraft scenarios above the ionosphere. |
| **Fixed gain** | Holds all satellite signals at a constant level instead of computing free-space path loss per satellite. |
| **Elevation mask (°)** | Satellites below this elevation angle are excluded. 0 = no mask (all satellites used). |
| **Block PRNs** | Comma-separated list of PRN numbers to exclude (applies to all constellations). |
| **Oscillator offset (ppb)** | Simulate a receiver clock frequency offset in parts-per-billion. |
| **Leap seconds** | Manual override for leap second correction: GPS week, day of week (1–7), delta leap seconds. |

#### Logging

| Setting | Description |
|---|---|
| **Position log** | Write a CSV log of the simulated receiver position to the specified file path. |

---

### SDR output options

| Output | Description |
|---|---|
| **HackRF** (default) | Transmit via a connected HackRF One. Uses the TX VGA gain and amp settings above. |
| **IQ File** | Write raw signed 8-bit interleaved IQ samples to a binary file (`[I0, Q0, I1, Q1, …]`). |
| **UDP stream** | Stream IQ bytes to a UDP destination (e.g. `127.0.0.1:4567`). Packets are 32 768 bytes. |
| **TCP server** | Open a TCP server on the specified port. The app waits for a client, then streams IQ bytes continuously. |
| **Null** | Generate the signal but discard all output. Useful for benchmarking or testing without hardware. |

> The UDP and TCP outputs work with the included [GNU Radio flow graphs](#gnu-radio-flow-graphs).

---

## Create UMF Route

A **UMF (User Motion File)** is a CSV file with one receiver position every 100 ms. The GPS simulator reads this file to know where the simulated receiver is at each moment.

Navigate to **Create UMF Route** using the left sidebar.

**Before generating:**
1. Enter a **Route name** — becomes the filename (`{name}.csv` and `{name}.geojson`).
2. Enter a **Velocity (km/h)** — the speed at which the receiver moves along the route.
3. Choose one of the four route sources below, then click **Generate User Motion File**.

The route is saved to `./umf/` and appears immediately in the Route Library.

---

### OpenRouteService API

Fetches a realistic road, path, or cycling route between waypoints using the free [OpenRouteService](https://openrouteservice.org/) directions API.

**Setup:** enter your API key via *File → Set ORS API Key* (free account required).

**Steps:**
1. Choose a **profile**: foot-walking, cycling-regular, driving-car, and more.
2. Enter **Start** coordinates (`lat, lon`). Click the field label to pick from your saved Waypoints.
3. Optionally add one or more **Via** points.
4. Enter **End** coordinates.
5. The map shows coloured markers as you type — green (start), orange (via), red (end).
6. Click **Generate User Motion File**.

The app calls the ORS API, receives a GeoJSON route, converts it to 100 ms transmit points at the configured velocity, and saves the CSV.

---

### GeoJSON file

Load a pre-existing route from a `.geojson` or `.json` file containing a `LineString` or `MultiLineString` geometry.

**Steps:**
1. Select **GeoJSON File** as the source.
2. Click *Browse* and open the file.
3. The route appears on the map.
4. Click **Generate User Motion File**.

---

### Draw on map

Draw a route interactively by clicking directly on the map.

| Action | Result |
|---|---|
| Click on empty map area | Append a new endpoint at the end of the route |
| Click near an existing segment | Insert a new vertex at that point along the segment |
| Drag an existing vertex | Move it to a new position |
| **Undo** button | Remove the last added point |
| **Clear** button | Remove all points and start over |
| *Browse* (import) | Load a GPX or KML file as the starting shape, then continue editing |

Click **Generate User Motion File** when the route is ready.

---

### GPX / KML import

Import a track recorded by a GPS device or exported from a mapping application.

**Steps:**
1. Select **Import GPX/KML** as the source.
2. Click *Browse* and open a `.gpx` or `.kml` file.
3. The track appears on the map.
4. Click **Generate User Motion File**.

---

## Manage Waypoints

Navigate to **Manage Waypoints** using the left sidebar.

Waypoints are named geographic coordinates that can be quickly selected as the start, via, or end point when creating routes — saving you from typing coordinates repeatedly.

| Action | How |
|---|---|
| **Add** | Fill in name, description, and coordinates (`lat, lon`) and click *Add* |
| **Pick from map** | Click a location on the map to fill coordinates automatically |
| **Edit** | Click *Edit* on any row to re-open the form with existing values |
| **Delete** | Click *Delete* on any row |
| **Filter** | Type in the search box to filter by name or description |
| **Sort** | Click any column header; click again to reverse |

Waypoints are saved to `./waypoint/` and persist across sessions.

---

## Manage UMF Routes

Navigate to **Manage UMF Routes** using the left sidebar.

Shows all route CSV files found in `./umf/` with their distance, estimated duration, and velocity.

| Action | How |
|---|---|
| **Preview** | Click any row to show the route geometry on the map |
| **Edit** | Click *Edit* to open the interactive route editor |
| **Delete** | Click *Delete* to remove the `.csv` and `.geojson` files |

**Interactive route editor:**
- Drag any vertex to move it.
- Click near a segment to insert a point there.
- Click away from all segments to append a new endpoint.
- Click **Done** to return to the library.
- Click **Open in Draw Route** to transfer the geometry to the *Create UMF Route → Draw on map* editor, with name and velocity pre-filled.

---

## GNU Radio flow graphs

The `gnuradio/` folder contains ready-to-use GNU Radio flow graphs for working with the app's network output.

### `gps_udp_to_hackrf_simple` — UDP → HackRF

The simplest bridge: receives the app's UDP IQ stream and retransmits it directly via HackRF. No visualisation.

```
App (UDP output) → network_udp_source → interleaved_char_to_complex → osmosdr_sink (HackRF)
```

Set the app output to **UDP** → `127.0.0.1:4567`. Start the GNU Radio script first, then start the simulation.

```bash
python gnuradio/gps_udp_to_hackrf_simple.py
```

---

### `gps_network_to_hackrf` — Network stream → Visualise → HackRF

Full-featured: receives the IQ stream (UDP or TCP), shows live FFT spectrum, waterfall, time domain, and constellation, and simultaneously retransmits via HackRF.

```
App (TCP or UDP)
    │
    ├──► FFT spectrum (Tab 0)
    ├──► Waterfall (Tab 1)
    ├──► Time domain (Tab 2)
    ├──► Constellation (Tab 3)
    └──► HackRF TX @ 1575.42 MHz
```

TCP is recommended — no packet loss, no datagram alignment issues. Set the app output to **TCP**, start the simulation first (the app is the server), then start the GNU Radio script.

```bash
python gnuradio/gps_network_to_hackrf.py
```

---

### `gps_l1_analyzer` — HackRF RX spectrum analyzer

Uses a **second HackRF in RX mode** to verify the transmitted signal over the air or through a cable + attenuator.

```bash
python gnuradio/gps_l1_analyzer.py
```

---

### IQ wire format

| Parameter | Value |
|---|---|
| Encoding | Signed 8-bit integers (`i8`), interleaved `[I0, Q0, I1, Q1, …]` |
| Sample rate | 3 000 000 sps |
| Centre frequency | 1 575 420 000 Hz — GPS L1 C/A, BeiDou B1C, and Galileo E1 share this carrier |
| UDP packet size | 32 768 bytes = 16 384 complex samples |
| TCP | Continuous byte stream, no framing |
| GNU Radio block | `interleaved_char_to_complex(scale=1/128)` → complex float ±1.0 |

### GNU Radio installation

```bash
# Ubuntu / Debian
sudo apt install gnuradio gr-osmosdr python3-pyqt5

# Arch
sudo pacman -S gnuradio gr-osmosdr python-pyqt5

# Windows
# Install GNU Radio via the official installer (includes gr-osmosdr)
# Replace HackRF USB driver with WinUSB via https://zadig.akeo.ie/
```

---

## File layout

```
gui_sdr_gps_sim/
├── src/
│   ├── main.rs              Entry point — window, icon, fonts
│   ├── app.rs               App state, page/tab enums
│   ├── ui.rs                All UI rendering
│   ├── map_plugin.rs        Map plugins (markers, route lines, editor)
│   ├── simulator/
│   │   ├── mod.rs           Public simulator API
│   │   ├── state.rs         SimSettings, SimState, SimStatus, SimOutputType
│   │   └── worker.rs        Background simulation thread entry-points
│   ├── gps_sim/             GNSS baseband signal engine (GPS / BeiDou / Galileo)
│   │   ├── orbit            Satellite orbit propagation (Kepler + perturbations)
│   │   ├── ionosphere       Klobuchar ionospheric delay model
│   │   ├── troposphere      Hopfield tropospheric delay model
│   │   ├── codegen          GPS C/A Gold code generation (PRN 1–32)
│   │   ├── beidou           BeiDou B1C Weil code generation (PRN 1–63)
│   │   ├── galileo          Galileo E1-B/C LFSR code generation (PRN 1–36)
│   │   ├── navmsg           GPS navigation message generation
│   │   ├── rinex            RINEX 2/3 multi-constellation ephemeris parser
│   │   ├── signal           IQ sample accumulation (100 ms blocks)
│   │   ├── fifo             8 × 262 KB lock-free FIFO
│   │   ├── hackrf           HackRF USB TX thread
│   │   └── sim              Simulator::builder() / run()
│   ├── route/
│   │   ├── ors.rs           OpenRouteService HTTP client
│   │   ├── segment.rs       Route → 100 ms transmit points
│   │   ├── pipeline.rs      End-to-end route generation pipeline
│   │   └── geojson.rs       ORS GeoJSON response types
│   ├── library.rs           Route library scanner
│   ├── rinex.rs             NASA CDDIS FTPS downloader
│   ├── waypoint.rs          Waypoint persistence
│   ├── geo.rs               Coordinate maths + CSV writer
│   ├── import.rs            GPX / KML parser
│   └── paths.rs             Working directory helpers
├── gnuradio/                GNU Radio flow graphs + README
├── assets/img/              Embedded UI images
├── umf/                     Generated route CSVs, GeoJSON, library.json
├── waypoint/                Saved waypoints
├── Rinex_files/             Downloaded RINEX navigation files
├── rust-toolchain           Pins Rust 1.88
└── check.sh                 Local CI script
```

---

## Building from source

```bash
# Clone
git clone https://github.com/okiedocus/gui_sdr_gps_sim
cd gui_sdr_gps_sim

# Run in debug mode
cargo run

# Build optimised release binary
cargo build --release

# Run all CI checks (format, lint, tests)
bash check.sh
```

---

## Contributing

Contributions of any kind are welcome — bug reports, hardware compatibility reports, feature suggestions, code, documentation, and GNU Radio flow graphs.

Please read [CONTRIBUTING.md](CONTRIBUTING.md) before opening a pull request. Key points:

- Run `bash check.sh` before submitting — all checks must pass.
- Keep pull requests focused on a single concern.
- Open an issue to discuss large changes before writing code.

If you find a security vulnerability, **do not open a public issue** — see [SECURITY.md](SECURITY.md) for the private reporting process.

---

## License

Licensed under the [GNU General Public License v3.0 or later](LICENSE) (`GPL-3.0-or-later`).
