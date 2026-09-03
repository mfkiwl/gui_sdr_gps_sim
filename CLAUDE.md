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
| `src/app/mod.rs` | `MyApp` struct (all serde-persisted fields), `Default`, `MyApp::new`, `eframe::App` |
| `src/app/state.rs` | `AppPage` / `RouteSource` / `SimTab` / `AppStatus` enums |
| `src/app/routes.rs` | `impl MyApp` — `generate()` and the three route-generation paths (ORS, drawn, GeoJSON) |
| `src/app/library.rs` | `impl MyApp` — scanning, loading, editing, and deleting route-library entries |
| `src/app/waypoints.rs` | `impl MyApp` — waypoint load/save/edit/delete |
| `src/app/simulation.rs` | `impl MyApp` — `start_*_simulation()`, `download_rinex*()`, `parse_blocked_prns()` |
| `src/ui/mod.rs` | `update()` — the per-frame entry point; polls background tasks, then delegates |
| `src/ui/chrome.rs` | Menu bar, API-key dialog, nav sidebar, `navigate()`, central-panel dispatch. **All `include_image!` macros live here**, so asset paths are `../../assets/img/` |
| `src/ui/widgets.rs` | Helpers shared across pages: `page_heading`, `section_title`, `add_map_zoom_controls`, `sortable_header_text`, `format_duration` |
| `src/ui/home.rs` | Home page + `home_card()` |
| `src/ui/route.rs` | Create UMF Route page, its maps, and `RoutePageActions` |
| `src/ui/waypoints.rs` | Manage Waypoints page, table, add form, and `WaypointPageActions` |
| `src/ui/library.rs` | Manage UMF Routes page, library table, and `RouteLibraryActions` |
| `src/ui/sim/` | SDR GPS Simulator page: `mod.rs` (tab bar) + `dynamic`, `fixed`, `interactive`, `settings` |
| `src/waypoint.rs` | `Waypoint` / `WaypointEntry` types; free-fn `load_waypoints` / `save_waypoints` |
| `src/geo.rs` | `parse_coords`, `lla_to_ecef` (WGS-84), `write_transmit_points_to_csv` |
| `src/route/ors.rs` | Async HTTP client for the OpenRouteService directions API |
| `src/route/segment.rs` | `Segment` struct; `segmentize()` splits a route into GPS transmit points |
| `src/route/pipeline.rs` | `run_pipeline()` — orchestrates ORS fetch → segmentize → CSV write; `run_pipeline_from_geojson()` skips the ORS call |
| `src/route/geojson.rs` | Serde types for the GeoJSON API response |
| `src/simulator/mod.rs` | Public API of the simulator module; also hosts `open_file_dialog()` |
| `src/simulator/state.rs` | `SimSettings`, `SimState`, `SimStatus` — shared between worker and UI |
| `src/simulator/worker.rs` | `run()` / `run_static_loop()` — thin wrappers that delegate to `gps_sim::Simulator` |
| `src/gps_sim/` | GPS L1 C/A baseband signal simulator. Sub-modules: `types`, `coords`, `orbit`, `ionosphere`, `troposphere`, `codegen`, `navmsg`, `rinex`, `signal`, `fifo`, `hackrf`, `channel` (private), `sim` (private). Public entry point: `Simulator::builder()`. See **GPS simulator notes** below. |
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
8. Dynamic Mode shows a live-tracking map: `interpolate_route_pos()` in `ui/sim/dynamic.rs` derives the current geographic position from `current_step / total_steps` and centers the map on it each frame.

**`SdrOutput` variants** (defined in `gps_sim/mod.rs`): `HackRf { gain_db, amp }`, `IqFile { path }`, `Null`, `PlutoSdr { host, gain_db }`, `UdpStream { addr }`, `TcpServer { port }`.

**GPS simulator notes (`src/gps_sim/`):**

*Sample rate auto-selection* — `select_sample_rate(override, widest_constellation, output)` in `sim.rs` picks the IQ sample rate from the enabled constellations **and the output sink**. `SimulatorBuilder::hackrf_sample_rate` always overrides.

Each constellation declares `chip_rate()`, `nyquist_rate()` (= 2 × chip rate), and `preferred_sample_rate()` in `types.rs`. The preferred rate is then clamped by `SdrOutput::max_sample_rate()`, which is `Some(20 MSPS)` for HackRF and PlutoSDR and `None` for IqFile / UdpStream / TcpServer / Null:

| Constellation | Chip rate | Nyquist | Preferred | On HackRF/Pluto | Samples/chip |
|---|---|---|---|---|---|
| GPS L1 C/A only | 1.023 Mcps | 2.05 MSPS | 3 MSPS | 3 MSPS | 2.93 |
| Galileo E1-B | 4.092 Mcps | 8.18 MSPS | 10 MSPS | 10 MSPS | 2.44 |
| BeiDou B1C | 10.23 Mcps | 20.46 MSPS | 25 MSPS | 20 MSPS (clamped) | 2.44 / 1.95 |

Clamping below Nyquist logs a `log::warn!` rather than failing — an aliased B1C signal is still the best a HackRF can do. Sinks with no hardware in the path get the full 25 MSPS, so B1C IQ recordings are usable by an external receiver.

The rate flows from `effective_sample_rate()` → each `run_*` backend → `generate_iq(sample_rate)`, which computes `dt = 1/rate` and `samples_per_step = STEP_SECS * rate` at runtime. The HackRF hardware is configured at the same rate to keep TX and IQ generation in sync.

*Signal generation hot path* (`generate_iq` in `sim.rs`):
- cos/sin lookup tables (`COS_TABLE`, `SIN_TABLE`): 512 entries, amplitude ±250
- per-channel gain: `path_loss × ant_gain` (path_loss = 20200 km / range; ant_gain from `ant_pattern_linear()`)
- per-sample: `i_acc += iq_sign * cos_tab[itable] * gain`, then `buf[pos] = (i_acc >> 4) as i8`
- right-shift by 4 brings the summed ±(N×250) accumulator into the ±127 sc8 range
- `chip_idx` and `code_ca` are updated **every sample** (not only at code-period boundaries)
- carrier phase advanced by `f_carr * dt` each sample; code phase by `f_code * dt`

*Antenna pattern* (`signal.rs`): osqzss 37-bin model, 0 dB at zenith, 31.56 dB attenuation at horizon (5° steps). `ant_pattern_linear()` converts the dB table to a linear voltage gain.

*Channel initialisation* (`channel.rs`): `Channel::new()` sets the initial Doppler from the pseudorange rate: `f_carr = -rho.rate / LAMBDA_L1`, `f_code = chip_rate + f_carr / CARR_TO_CODE`. This ensures the first 100 ms step already has the correct frequency offset, not zero.

**Navigation message (`navmsg.rs` + `channel.rs`) — the data layer:**

A receiver can acquire and track a perfectly-formed signal and still never report a position. Everything that decides *position* lives in the 50 bps data layer, and none of it is visible to a spectrum plot or an acquisition search. Five invariants must hold:

1. **Parity (IS-GPS-200 Table 20-XIV).** Each of the six parity bits XORs in one of the previous word's two carry bits: D25/D27/D30 chain from D29\*, and D26/D28/D29 chain from D30\*. Parity is computed over the *uncomplemented* data, while the transmitted data bits are complemented when D30\* is set. Dropping the carry term leaves parity correct only when both carry bits are zero — about one word in four — and every subframe is then discarded by the receiver.
2. **Words 2 and 10** of every subframe carry two non-information-bearing bits (23 and 24), solved so the resulting D29 and D30 are zero. That is the `nib` flag on `compute_checksum`.
3. **Frame timing.** A frame is exactly 50 words = 30 s at 50 bps, aligned to a 30-second GPS epoch (`frame_start`). Each subframe's HOW carries the TOW of the **next** subframe boundary. `Channel::init_code_phase` positions the bit counters from the signal's *transmit* time (`grx - range/c`), not from `grx`, because that is the reference the receiver reconstructs from the TOW.
4. **Bit-field placement (IS-GPS-200 Table 20-I / Fig. 20-1).** The 24 data bits of a word sit at shifts 29..6, so a field spanning data bits *n*..*m* goes at shift `30 - m`. Two consequences the encoder gets wrong easily and no parity or spectrum check will ever notice:
   - **SV health** is data bits 17–22 of subframe 1 word 3 (shift 8), between URA (13–16, shift 14) and the IODC MSBs (23–24, shift 6). Put anything else at shift 13 and the health MSB comes up set, which Table 20-VII defines as *"some or all navigation data are bad"* — every receiver then drops the satellite after acquiring and tracking it perfectly.
   - **M0, e, √A, Ω0, i0 and ω** are 32-bit quantities split into an 8-bit MSB field in one word and a 24-bit LSB field in the next. The MSB field needs `pack_hi(.., lo_bits = 24, ..)`; packing it with `pack` silently repeats the *low* byte and the decoded orbit lands thousands of kilometres away.

   Subframe 1's clock block also lives in words 7–10 (TGD, IODC LSB + toc, af2 + af1, af0), not 4–7; words 4–6 are reserved.
5. **The frame wrap must always advance.** `advance_nav_bit()` swaps in the frame built by `prepare_next_frame()`; if none has been built it builds one itself rather than keeping the current frame. A channel can start anywhere in its frame — `init_code_phase` positions it from the *transmit* time, so any run beginning on or just after a 30-second epoch starts in the final subframe (word ≥ 40) and reaches its first wrap within ~70 ms, long before `generate_iq` gets to its once-per-100 ms `prepare_next_frame()` call. Re-transmitting the old frame there leaves the TOW exactly 30 s behind the signal *permanently*: parity holds, bit sync holds, the ephemeris decodes, and the receiver places every satellite 30 s of orbit — a few km of range, varying per satellite — from where it is, so the least-squares residuals never close. `Channel::new` also pre-builds the successor so the common case stays a plain copy.

The frame buffer is sized to exactly one frame so the word counter wraps precisely on the frame boundary — a buffer longer than a frame transmits dead words. `Channel::prepare_next_frame()` builds the following frame once the channel reaches word 40, and `advance_nav_bit()` swaps it in at the wrap, carrying `last_word` across so parity chains unbroken. Both are driven by the channel's own word counter rather than the step index, so frames stay aligned with that satellite's Doppler-shifted bit stream.

*Known hard limit*: BeiDou B1C (10.23 Mcps) needs >20.46 MSPS, which exceeds the HackRF's 20 MSPS maximum — over-the-air B1C is aliased at 1.95 samples/chip and the simulator warns about it. This is a hardware ceiling only: file/UDP/TCP sinks generate B1C at 25 MSPS, above Nyquist. GPS and Galileo clear Nyquist on every sink.

**Scenario time and ephemeris selection (`sim.rs`) — what a receiver checks that a decoder does not:**

`resolve_start_time(StartTime::Now, ..)` returns the **current** GPS time. It must not fall back to the RINEX file's first epoch — that quietly stamps a run started at 22:00 with a timestamp from midnight. A software receiver still fixes on that signal, because it has no independent clock to disagree with; a phone holds network time and cached assistance data and rejects it. The file's own epoch is used only when the system clock reads before 1980.

`merge_eph_sets()` then builds the ephemeris a receiver would actually hold: **per satellite**, its own most recent record at or before `g0`. The parser groups records into per-epoch sets on a one-hour boundary, but satellites do not upload together, so a day's broadcast file routinely ends with a set holding a *single* satellite that refreshed minutes ago. Selecting the one set nearest `g0` picks that set and transmits a one-satellite constellation. Ranking prefers a record already in force over one from the future; a run whose freshest ephemeris is more than two hours old logs a `warn!` suggesting a fresher file.

These two are coupled: making `Now` mean now is what exposes the tail of the file, so neither is safe to change alone.

**Pacing (`generate_iq`):** generation is paced by **back-pressure, not by a clock**. `producer.acquire()` blocks once every FIFO buffer is outstanding, which is how the C reference paces its generator — it has no sleep at all. A wall-clock sleep on top of that can only make generation slower than the consumer, never faster, because the sleep always overshoots; measured at +0.35% per step, which drains the FIFO and then underruns the HackRF on every buffer for the rest of the run. None of that is visible in an IQ file, which is written from the same samples whenever they were produced — generation is deterministic in the step index, and two runs from the same `StartTime::Gps` are byte-identical. The `pace` flag is therefore `true` only for `UdpStream`, the one sink with no peer to block on. With the gate gone, 45 s of signal generates in ~6 s.

**Ephemeris staleness:** IS-GPS-200 gives a 4-hour curve fit, so an ephemeris is valid only for `toe ± 2 h`. Past that a strict receiver discards it — it acquires, tracks, decodes clean subframes and still reports no position. `run()` warns through both `log::warn!` and a `SimEvent::Status` (so it reaches the GUI, not just the log) when the **median** ephemeris age exceeds 2 h. Median, not freshest: a broadcast file routinely ends with one SV that refreshed minutes ago, and the freshest age would call that constellation current while its other thirty members sit hours past reference.

**Code phase is re-anchored, not integrated (`Channel::resync`):**

`generate_iq` calls `resync()` on every channel at the **top** of each 100 ms step, against that step's receiver position and GPS time, before generating any samples — the same thing the C reference's `computeCodePhase()` does. It sets the code phase and bit counters from the current pseudorange and derives the Doppler from it as a backward difference of that pseudorange.

Letting the code NCO free-run between anchors instead makes the phase a *rate integral*, so every error in the rate accumulates without bound. Two errors matter:

- `RangeResult::rate` is the satellite's velocity projected on the line of sight and knows nothing about the **receiver's** motion. On a moving route the code phase drifts by the receiver's own along-line-of-sight speed — metres per second — so the solution never walks the path. The backward difference of the pseudorange picks this up for free, along with the ionospheric and satellite-clock rates.
- Any residual error in the analytic rate integrates the same way. Re-anchoring bounds it to a single step.

`prev_range` is seeded in `Channel::new` as `rho.range - rho.rate * STEP_SECS`, so the *first* `resync` reproduces the analytic rate. Seeding it with `rho.range` itself makes step 0 differ by nothing and the channel opens with 100 ms of zero-Doppler signal — enough to defeat a receiver that acquires on the first few milliseconds.

**Range rate must equal the derivative of the range (`orbit.rs`):**

`dν/dt = √(1−e²)·(dE/dt)/(1−e·cos E)`. Writing it through `sin ν` and `cos ν` instead looks plausible but collapses algebraically to plain `dE/dt` — substituting `sin ν = √(1−e²)·sin E/(1−e·cos E)` and `1 + e·cos ν = (1−e²)/(1−e·cos E)` cancels the whole eccentricity factor. That understates or overstates dν/dt by up to *e* (≈1%), which reaches the line-of-sight range rate as several m/s, different per satellite.

`sat_pos` is unaffected, so the orbit stays right and only its derivative is wrong: nothing in a position, a spectrum or an acquisition search shows it. It appears once the code phase integrates the rate, as a pseudorange that drifts linearly. `range_rate_matches_the_numerical_derivative` pins it with a central difference swept over a full orbit at a realistic eccentricity.

*Measured on a 75 s static capture, before → after both fixes:* position scatter 28.7/3.2/38.4 m E/N/U → **0.7/0.4/0.8 m**; horizontal drift 98.5 m/48 s → **1.3 m/31 s**; per-satellite pseudorange drift 2.98 m/s → 0.59 m/s. On a 15 m/s route: fitted speed 15.03 m/s, along-track misfit 0.26 m rms, cross-track 0.81 m.

**Signal-chain tests:**

`tests/signal_chain.rs` runs the whole pipeline (synthetic RINEX → IQ file) and asserts on the generated baseband — main lobe vs sidelobes, C/A nulls at ±1.023 MHz, sc8 amplitude range, I/Q balance, and DC concentration. It is the Rust counterpart of `gnuradio/plot_iq_file.py` and guards the fixed bugs listed below.

The RINEX fixture is generated in-test (`synth_rinex()`), so no nav file is needed on disk. Spectral bands deliberately **exclude DC**: an unmodulated carrier puts all its power in the DC bin and would otherwise pass a naive main-lobe check.

When changing the signal chain, verify a test still fails with the bug reintroduced — several of these checks are only load-bearing because their bands are chosen carefully.

**Decoding a capture end to end:** `gnuradio/gps_nav_decode.py` is a complete software receiver over an IQ file — acquire, track, decode the navigation message, form pseudoranges, solve for position:

```bash
python gnuradio/gps_nav_decode.py --file capture.iq --truth 52.3791 4.9003 5.0
```

It needs ≥35 s of capture. Its tracking loop reports **fractional** sample positions for each code epoch: rounding them to whole samples costs ±0.5 sample, and at 3 MSPS one sample is 100 m of range, which buries every error worth measuring under ~40 m of quantisation noise. Three lines carry most of the diagnostic value: the satellite count in the simulator's own `Tracking N satellites` line (a low count means the ephemeris merge picked up a nearly-empty tail set), `subframe t_tx` — which must equal the GPS time the simulator printed at start-up, a shortfall of a multiple of 30 s meaning the frame is running behind the signal — and the residual rms, which should be tens of metres — that is the iono/tropo delay the simulator adds and this tool does not correct. Kilometre residuals mean the pseudoranges and the broadcast ephemeris disagree. Prefer this over an acquisition search when a receiver tracks but will not fix; acquisition passes in every one of those cases.

**Navigation-message tests** live in `channel.rs` and decode the bit stream the way a receiver does: pull bits via `advance_nav_bit()`, check parity on every word, locate the preamble, and compare the decoded TOW against where the bits actually sit in time.

`navmsg.rs` adds the complementary layer: `subframe_1_clock_fields_round_trip`, `subframe_2_and_3_orbit_fields_round_trip`, `satellites_are_broadcast_healthy` and `iode_matches_iodc_lsbs` decode each field back out by its IS-GPS-200 **data-bit number** and compare against the ephemeris that went in. They deliberately do not reuse the encoder's shift constants, so a field written into the wrong word shows up as a wrong *value* rather than as a matching mistake on both sides. Every parity and framing test passes with those fields misplaced — that is why the round-trip tests exist.

A test also has to drive the channel the way `generate_iq` does. `capture_bits` calls `prepare_next_frame()` before *every* bit, so the wrap is always a plain copy and invariant 5 above can never fail inside it; `tow_survives_the_first_wrap_when_started_in_the_final_subframe` instead calls it once per five bits (one 100 ms step) and picks an epoch whose transmit time lands in a frame's final subframe. Every other test in the suite passes with that bug reintroduced.

These tests anchor their expectations to `grx` and the pseudorange — **never to the channel's own `g0` or word counters**. A test that reads its expectation out of the state it is checking will agree with a uniformly-shifted timeline, which is exactly the bug class that lets a receiver track happily and never fix. If you add tests here, derive ground truth independently.

**UI rendering pattern:**

`eframe::App::update` delegates immediately to `ui::update(app, ctx)`, which renders:
- `TopBottomPanel` (top) — File menu + theme toggle
- `SidePanel` (left) — logo (click → Home) + four `nav_image_active_with_tooltip` buttons that set `app.current_mode`
- `CentralPanel` — wraps all page content in a `ScrollArea::vertical()` (auto-scrolls when content exceeds window height), then dispatches on `app.current_mode`

Because egui closures hold borrows, mutations triggered by button clicks are **deferred**: page functions return an actions struct (`RouteLibraryActions`, `WaypointPageActions`, `RoutePageActions`) applied after the closure completes.

Page functions live in one module per page and are `pub(crate)`; the actions structs' fields are `pub(crate)` too, because `ui::chrome::show_central_panel` applies them after the page closure returns.

**UI helpers in `ui/widgets.rs`:**
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

**Image assets** in `assets/img/` are embedded at compile time via `egui::include_image!()`. All image macros live in `src/ui/chrome.rs`, so paths use `../../assets/img/`. Adding an `include_image!` elsewhere in `src/ui/` needs the same two-level prefix — the macro resolves relative to its own source file.

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
