//! Top-level simulator: builder, runner, and IQ generation loop.
//!
//! # Usage
//!
//! ```no_run
//! use gui_sdr_gps_sim::gps_sim::{Simulator, Location, SdrOutput};
//!
//! Simulator::builder()
//!     .rinex("brdc0010.24n")
//!     .location(Location::degrees(52.3676, 4.9041, 5.0))
//!     .duration_secs(300)
//!     .output(SdrOutput::HackRf { gain_db: 20, amp: false })
//!     .build().unwrap()
//!     .run().unwrap();
//! ```

use std::io::{BufWriter, Write as _};
use std::net::{TcpListener, UdpSocket};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};
use std::time::{Duration, Instant};

use super::channel::Channel;
use super::coords::llh_to_ecef;
use super::fifo::IqFifo;
use super::navmsg::generate_nav_msg;
use super::rinex::NavData;
use super::signal::{COS_TABLE, SIN_TABLE, ant_pattern_linear};
use super::types::{
    Constellation, GpsTime, Location, StartTime,
    consts::{
        CARR_TO_CODE, DT, HACKRF_BUF_BYTES, LAMBDA_L1, MAX_CHANNELS, SAMPLES_PER_STEP, STEP_SECS,
    },
};
use super::{SdrOutput, SimError};

// ── Status events ─────────────────────────────────────────────────────────────

/// Status events emitted from the GPS generation loop.
///
/// Pass a callback via [`SimulatorBuilder::on_event`] to receive these.
///
/// The enum is `#[non_exhaustive]`; always include a `_ => {}` catch-all arm
/// so that future variants do not break your code.
#[non_exhaustive]
#[derive(Debug, Clone)]
pub enum SimEvent {
    /// Human-readable status message (e.g., "Loaded N satellites").
    Status(String),
    /// Current simulated receiver position (emitted every second).
    Position {
        lat_deg: f64,
        lon_deg: f64,
        height_m: f64,
    },
    /// Per-satellite azimuth/elevation status update (once per second).
    Satellite {
        prn: u8,
        az_deg: f64,
        el_deg: f64,
        active: bool,
    },
    /// Step counter for progress-bar display (emitted every 100 ms step).
    ///
    /// `bytes_sent` counts every transmitted IQ buffer, including paused iterations
    /// where `current_step` stays fixed.
    Progress {
        current_step: usize,
        total_steps: usize,
        bytes_sent: u64,
    },
    /// Simulation has finished normally.
    Done,
}

// ── GUI-friendly progress snapshot ───────────────────────────────────────────

/// Satellite visibility snapshot for GUI display.
#[derive(Clone, Debug)]
pub struct SatInfo {
    /// PRN number (1–32).
    pub prn: u8,
    /// Azimuth angle in degrees (0 = North, clockwise).
    pub az_deg: f64,
    /// Elevation angle in degrees above horizon.
    pub el_deg: f64,
}

/// Snapshot of simulation progress, updated every 100 ms step.
///
/// Retrieved by cloning [`SimulatorHandle::progress`] each UI frame.
#[derive(Default, Clone, Debug)]
pub struct SimProgress {
    /// Steps completed so far (0-based step index).
    pub current_step: usize,
    /// Total number of steps (duration × 10).
    pub total_steps: usize,
    /// Most-recent receiver latitude (degrees).
    pub lat_deg: f64,
    /// Most-recent receiver longitude (degrees).
    pub lon_deg: f64,
    /// Most-recent receiver height above WGS-84 ellipsoid (metres).
    pub height_m: f64,
    /// Currently-tracked satellites (updated once per second).
    pub satellites: Vec<SatInfo>,
    /// Set to `true` when the simulation has finished (normally or via stop).
    pub finished: bool,
    /// Non-empty if the simulation ended with an error.
    pub error: Option<String>,
}

/// Handle to a simulation running on a background OS thread.
///
/// Returned by [`Simulator::run_async`].
///
/// # eframe/egui usage
/// Store this in your `App` struct.  Each frame, call [`SimulatorHandle::snapshot`]
/// to get a cheap [`SimProgress`] clone for display.  Call [`SimulatorHandle::stop`]
/// to cancel, or [`SimulatorHandle::join`] to block until the thread finishes.
///
/// ```no_run
/// # use gui_sdr_gps_sim::gps_sim::*;
/// # let handle = Simulator::builder().rinex("x.n")
/// #     .location(Location::degrees(0.,0.,0.)).output(SdrOutput::Null).build().unwrap().run_async();
/// // Inside eframe::App::update():
/// let p = handle.snapshot();
/// // ui.add(egui::ProgressBar::new(p.current_step as f32 / p.total_steps.max(1) as f32));
/// if p.finished { /* show "Done" or retrieve error */ }
/// ```
pub struct SimulatorHandle {
    /// Stop flag — set to `true` to halt the simulation early.
    pub stop: Arc<AtomicBool>,
    /// Shared progress state, updated by the simulation thread every step.
    pub progress: Arc<Mutex<SimProgress>>,
    thread: std::thread::JoinHandle<Result<(), SimError>>,
}

impl SimulatorHandle {
    /// Signal the simulation to stop as soon as possible.
    pub fn stop(&self) {
        self.stop.store(true, Ordering::Relaxed);
    }

    /// Return a cheap clone of the current progress state.
    ///
    /// Safe to call from any thread (e.g., the eframe UI thread each frame).
    pub fn snapshot(&self) -> SimProgress {
        self.progress
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    /// Return `true` if the simulation has finished (naturally or via stop).
    pub fn is_done(&self) -> bool {
        self.progress.lock().map(|p| p.finished).unwrap_or(true)
    }

    /// Block until the simulation thread finishes and return its result.
    ///
    /// Consumes the handle.
    ///
    /// # Errors
    /// Returns [`SimError::Aborted`] if the simulation thread panicked, or
    /// any error that terminated the simulation otherwise.
    pub fn join(self) -> Result<(), SimError> {
        self.thread.join().unwrap_or(Err(SimError::Aborted))
    }
}

// ── Interactive state ─────────────────────────────────────────────────────────

/// Shared state for keyboard-controlled interactive simulation.
///
/// The keyboard input thread updates this every keystroke; the IQ generation
/// loop reads it once per 100 ms step to compute the next receiver position.
#[derive(Debug, Clone)]
pub struct InteractiveState {
    /// Current heading in degrees, measured clockwise from North.
    pub bearing_deg: f64,
    /// Horizontal ground speed in m/s.
    pub speed_ms: f64,
    /// Vertical speed in m/s (positive = ascending).
    pub vert_speed_ms: f64,
}

impl Default for InteractiveState {
    fn default() -> Self {
        Self {
            bearing_deg: 0.0,
            speed_ms: 0.0,
            vert_speed_ms: 0.0,
        }
    }
}

// ── Builder ───────────────────────────────────────────────────────────────────

/// Builder for [`Simulator`].  Use [`Simulator::builder`] to obtain one.
pub struct SimulatorBuilder {
    rinex: Option<String>,
    location: Option<Location>,
    motion_file: Option<String>,
    start: StartTime,
    duration: u32,
    output: SdrOutput,
    on_event: Option<Box<dyn Fn(SimEvent) + Send + Sync + 'static>>,
    ppb: i32,
    elev_mask: f64,
    interactive: bool,
    log_path: Option<String>,
    blocked_prns: Vec<u8>,
    external_stop: Option<Arc<AtomicBool>>,
    external_istate: Option<Arc<Mutex<InteractiveState>>>,
    ionospheric_disable: bool,
    time_override: bool,
    fixed_gain: Option<i32>,
    leap: Option<(i32, i32, i32)>,
    hackrf_sample_rate: Option<f64>,
    hackrf_center_freq: Option<u64>,
    hackrf_baseband_filter: Option<u32>,
    external_pause: Option<Arc<AtomicBool>>,
    /// Enable `BeiDou` B1C signal simulation (adds SVs to the output buffer).
    use_beidou: bool,
    /// Enable Galileo E1-B signal simulation (adds SVs to the output buffer).
    use_galileo: bool,
}

impl Default for SimulatorBuilder {
    fn default() -> Self {
        Self {
            rinex: None,
            location: None,
            motion_file: None,
            start: StartTime::Now,
            duration: 300,
            output: SdrOutput::Null,
            on_event: None,
            ppb: 0,
            elev_mask: 0.0,
            interactive: false,
            log_path: None,
            blocked_prns: Vec::new(),
            external_stop: None,
            external_istate: None,
            ionospheric_disable: false,
            time_override: false,
            fixed_gain: None,
            leap: None,
            hackrf_sample_rate: None,
            hackrf_center_freq: None,
            hackrf_baseband_filter: None,
            external_pause: None,
            use_beidou: false,
            use_galileo: false,
        }
    }
}

impl SimulatorBuilder {
    /// Path to a RINEX 2 or 3 navigation file (`.n`, `.rnx`, or `.gz`).
    pub fn rinex(mut self, path: impl Into<String>) -> Self {
        self.rinex = Some(path.into());
        self
    }

    /// Path to a CSV motion file (`time,ecef_x,ecef_y,ecef_z`).
    ///
    /// Each row defines the receiver ECEF position (metres) at one 100 ms step.
    /// The time column is accepted but ignored — row order determines timing.
    /// When provided, overrides the fixed `-l` location and sets duration to
    /// the number of waypoints (capped by `-d` if specified).
    pub fn motion_file(mut self, path: impl Into<String>) -> Self {
        self.motion_file = Some(path.into());
        self
    }

    /// Simulated receiver location.
    pub fn location(mut self, l: Location) -> Self {
        self.location = Some(l);
        self
    }

    /// When to begin the simulation.  Defaults to the current system time.
    pub fn start_time(mut self, t: StartTime) -> Self {
        self.start = t;
        self
    }

    /// Simulation duration in seconds.  Defaults to 300 s.
    pub fn duration_secs(mut self, s: u32) -> Self {
        self.duration = s;
        self
    }

    /// Output sink (`HackRF`, IQ file, or null).  Defaults to `Null`.
    pub fn output(mut self, o: SdrOutput) -> Self {
        self.output = o;
        self
    }

    /// Callback invoked for each [`SimEvent`].
    pub fn on_event(mut self, f: impl Fn(SimEvent) + Send + Sync + 'static) -> Self {
        self.on_event = Some(Box::new(f));
        self
    }

    /// Oscillator frequency offset in parts-per-billion.
    ///
    /// Positive PPB → oscillator runs fast → signal frequency is shifted down.
    pub fn ppb(mut self, ppb: i32) -> Self {
        self.ppb = ppb;
        self
    }

    /// Minimum elevation angle (degrees) for a satellite to be included.
    pub fn elevation_mask_deg(mut self, deg: f64) -> Self {
        self.elev_mask = deg.to_radians();
        self
    }

    /// Write a CSV position log during the simulation.
    ///
    /// Each row records `time_s,lat_deg,lon_deg,height_m` once per simulation
    /// step (every 100 ms), useful for comparing the simulated trajectory
    /// against what a GPS receiver reports back.
    pub fn log_path(mut self, path: impl Into<String>) -> Self {
        self.log_path = Some(path.into());
        self
    }

    /// Block specific satellites (by PRN, 1–32) from being simulated.
    ///
    /// Blocked satellites are excluded at channel allocation and will never
    /// appear in the transmitted signal.  Useful for simulating partial sky
    /// visibility (urban canyon, trees, etc.).
    ///
    /// # Example
    /// ```no_run
    /// # use gui_sdr_gps_sim::gps_sim::Simulator;
    /// Simulator::builder().block_prns(vec![5, 12, 23]);
    /// ```
    pub fn block_prns(mut self, prns: Vec<u8>) -> Self {
        self.blocked_prns = prns;
        self
    }

    /// Enable keyboard-controlled interactive mode.
    ///
    /// When active, the receiver position is updated in real time using these keys:
    /// - `a` / `d` — bearing −0.127° / +0.127° (left/right)
    /// - `e` / `q` — speed +1 m/s / −1 m/s (faster/slower)
    /// - `w` / `s` — vertical speed +1 m/s / −1 m/s (up/down)
    /// - `x`       — stop simulation and exit
    pub fn interactive(mut self, yes: bool) -> Self {
        self.interactive = yes;
        self
    }

    /// Inject an externally-created stop flag (e.g., from a GUI "Stop" button).
    ///
    /// The built [`Simulator`] will use this flag as its stop handle instead of
    /// creating a fresh one.  This lets the caller (e.g., an eframe `App`) hold
    /// the same `Arc<AtomicBool>` and signal a stop by writing `true`.
    ///
    /// See also [`Simulator::run_async`] which returns a [`SimulatorHandle`]
    /// containing a stop flag automatically.
    pub fn with_stop(mut self, stop: Arc<AtomicBool>) -> Self {
        self.external_stop = Some(stop);
        self
    }

    /// Inject an externally-managed [`InteractiveState`] for GUI-driven control.
    ///
    /// When set, the simulator uses this shared state for receiver motion instead
    /// of spawning a stdin keyboard thread.  Update the state each UI frame (e.g.,
    /// from egui key events) to steer the simulated receiver in real time.
    ///
    /// Implicitly enables interactive mode — there is no need to also call
    /// `.interactive(true)`.
    pub fn with_interactive_state(mut self, state: Arc<Mutex<InteractiveState>>) -> Self {
        self.external_istate = Some(state);
        self.interactive = true;
        self
    }

    /// Disable the Klobuchar ionospheric correction model.
    ///
    /// When `true`, ionospheric delay is set to zero. Useful for spacecraft
    /// scenarios above the ionosphere.
    pub fn ionospheric_disable(mut self, yes: bool) -> Self {
        self.ionospheric_disable = yes;
        self
    }

    /// Shift all ephemeris TOC/TOE values to match the scenario start time.
    ///
    /// When `true`, allows any RINEX file to be used at an arbitrary time.
    pub fn time_override(mut self, yes: bool) -> Self {
        self.time_override = yes;
        self
    }

    /// Override the per-satellite gain with a fixed value, disabling path loss.
    ///
    /// When `Some(v)`, all satellite signals are transmitted at the same
    /// constant amplitude `v` regardless of elevation or distance.
    pub fn fixed_gain(mut self, gain: Option<i32>) -> Self {
        self.fixed_gain = gain;
        self
    }

    /// Override leap second parameters from the RINEX file.
    ///
    /// - `week`:  GPS week number when the leap second is effective.
    /// - `day`:   Day of week (1 = Sunday, 7 = Saturday).
    /// - `delta`: Delta leap seconds (GPS − UTC offset in whole seconds).
    pub fn leap_override(mut self, params: Option<(i32, i32, i32)>) -> Self {
        self.leap = params;
        self
    }

    /// Override the `HackRF` sample rate (Hz).  Defaults to 3 MSPS.
    pub fn hackrf_sample_rate(mut self, hz: f64) -> Self {
        self.hackrf_sample_rate = Some(hz);
        self
    }

    /// Override the `HackRF` centre frequency (Hz).  Defaults to GPS L1 C/A.
    pub fn hackrf_center_freq(mut self, hz: u64) -> Self {
        self.hackrf_center_freq = Some(hz);
        self
    }

    /// Override the `HackRF` baseband filter bandwidth (Hz).
    ///
    /// When not set, `set_sample_rate_auto` chooses the filter automatically.
    pub fn hackrf_baseband_filter(mut self, hz: u32) -> Self {
        self.hackrf_baseband_filter = Some(hz);
        self
    }

    /// Inject an externally-created pause flag.
    ///
    /// While the flag is `true`, the IQ generation loop continues transmitting
    /// at the current receiver position but does not advance along the route.
    /// Set the flag back to `false` to resume normal playback.
    pub fn with_pause(mut self, pause: Arc<AtomicBool>) -> Self {
        self.external_pause = Some(pause);
        self
    }

    /// Enable `BeiDou` B1C signal generation.
    ///
    /// When `true`, visible `BeiDou` SVs from the loaded RINEX file are added to
    /// the IQ output alongside GPS L1 C/A.  `BeiDou` B1C shares the 1575.42 MHz
    /// carrier so both signals appear in the same output buffer.
    pub fn use_beidou(mut self, yes: bool) -> Self {
        self.use_beidou = yes;
        self
    }

    /// Enable Galileo E1-B signal generation.
    ///
    /// When `true`, visible Galileo SVs from the loaded RINEX file are added to
    /// the IQ output alongside GPS L1 C/A.  Galileo E1 shares the 1575.42 MHz
    /// carrier so all signals appear in the same output buffer.
    pub fn use_galileo(mut self, yes: bool) -> Self {
        self.use_galileo = yes;
        self
    }

    /// Load the RINEX file and build the [`Simulator`].
    ///
    /// # Errors
    /// Returns an error if the RINEX file cannot be opened or parsed.
    pub fn build(self) -> Result<Simulator, SimError> {
        let path = self
            .rinex
            .ok_or_else(|| SimError::Rinex("no RINEX file specified".into()))?;
        let nav = super::rinex::load(&path)?;

        // Load motion CSV if provided; otherwise the fixed location is used.
        let waypoints = match self.motion_file {
            Some(p) => load_motion_csv(&p)?,
            None => Vec::new(),
        };

        // Location is required only when no motion file is given.
        let loc = if waypoints.is_empty() {
            self.location.ok_or_else(|| {
                SimError::Rinex("no location specified — use -l or supply a -m motion file".into())
            })?
        } else {
            // Use first waypoint converted to LLH as the nominal location.
            self.location.unwrap_or_else(|| {
                let first = waypoints.first().copied().unwrap_or([0.0, 0.0, 0.0]);
                let llh = super::coords::ecef_to_llh(first);
                Location::radians(llh.lat_rad, llh.lon_rad, llh.height_m)
            })
        };

        Ok(Simulator {
            nav,
            loc,
            waypoints,
            start: self.start,
            duration: self.duration,
            output: self.output,
            on_event: self.on_event,
            ppb: self.ppb,
            elev_mask: self.elev_mask,
            interactive: self.interactive,
            log_path: self.log_path,
            blocked_prns: self.blocked_prns,
            stop: self
                .external_stop
                .unwrap_or_else(|| Arc::new(AtomicBool::new(false))),
            external_istate: self.external_istate,
            ionospheric_disable: self.ionospheric_disable,
            time_override: self.time_override,
            fixed_gain: self.fixed_gain,
            leap: self.leap,
            hackrf_sample_rate: self.hackrf_sample_rate,
            hackrf_center_freq: self.hackrf_center_freq,
            hackrf_baseband_filter: self.hackrf_baseband_filter,
            pause: self.external_pause,
            use_beidou: self.use_beidou,
            use_galileo: self.use_galileo,
        })
    }
}

// ── Simulator ─────────────────────────────────────────────────────────────────

/// Configured GNSS signal simulator (GPS L1 C/A + optional `BeiDou` B1C / Galileo E1).
///
/// Obtain via [`Simulator::builder`] → [`SimulatorBuilder::build`].
pub struct Simulator {
    nav: NavData,
    loc: Location,
    /// ECEF waypoints loaded from a motion CSV.  Empty = fixed position.
    waypoints: Vec<[f64; 3]>,
    start: StartTime,
    duration: u32,
    output: SdrOutput,
    on_event: Option<Box<dyn Fn(SimEvent) + Send + Sync + 'static>>,
    ppb: i32,
    elev_mask: f64,
    interactive: bool,
    log_path: Option<String>,
    blocked_prns: Vec<u8>,
    stop: Arc<AtomicBool>,
    external_istate: Option<Arc<Mutex<InteractiveState>>>,
    ionospheric_disable: bool,
    time_override: bool,
    fixed_gain: Option<i32>,
    leap: Option<(i32, i32, i32)>,
    hackrf_sample_rate: Option<f64>,
    hackrf_center_freq: Option<u64>,
    hackrf_baseband_filter: Option<u32>,
    pause: Option<Arc<AtomicBool>>,
    /// Whether to include `BeiDou` B1C signals in the output.
    use_beidou: bool,
    /// Whether to include Galileo E1-B signals in the output.
    use_galileo: bool,
}

impl Simulator {
    /// Return a builder with all settings at their defaults.
    pub fn builder() -> SimulatorBuilder {
        SimulatorBuilder::default()
    }

    /// Return a handle that can be used to stop the simulation from another thread.
    ///
    /// ```no_run
    /// # use gui_sdr_gps_sim::gps_sim::*;
    /// # let sim = Simulator::builder().rinex("x.n").location(Location::degrees(0.,0.,0.)).build().unwrap();
    /// let stop = sim.stop_handle();
    /// std::thread::spawn(move || {
    ///     std::thread::sleep(std::time::Duration::from_secs(60));
    ///     stop.store(true, std::sync::atomic::Ordering::Relaxed);
    /// });
    /// sim.run().unwrap();
    /// ```
    pub fn stop_handle(&self) -> Arc<AtomicBool> {
        self.stop.clone()
    }

    /// Run the simulation on a background OS thread and return a [`SimulatorHandle`].
    ///
    /// This is the recommended entry-point for GUI applications (eframe/egui).
    /// Call [`SimulatorHandle::snapshot`] each UI frame to poll progress, and
    /// [`SimulatorHandle::stop`] to cancel early.
    ///
    /// Any `on_event` callback previously set via [`SimulatorBuilder::on_event`]
    /// is preserved and will still be called in addition to the progress update.
    ///
    /// # Panics
    /// Panics if the OS cannot spawn a new thread.
    #[must_use]
    pub fn run_async(mut self) -> SimulatorHandle {
        let stop = self.stop.clone();
        let progress = Arc::new(Mutex::new(SimProgress::default()));
        let prog2 = progress.clone();

        // Extract the original callback (if any) and replace self.on_event with
        // a wrapper that updates SimProgress AND forwards to the original callback.
        let orig_cb = self.on_event.take();
        self.on_event = Some(Box::new(move |event| {
            match &event {
                SimEvent::Progress {
                    current_step,
                    total_steps,
                    ..
                } => {
                    let mut p = prog2.lock().unwrap_or_else(|e| e.into_inner());
                    p.current_step = *current_step;
                    p.total_steps = *total_steps;
                }
                SimEvent::Position {
                    lat_deg,
                    lon_deg,
                    height_m,
                } => {
                    let mut p = prog2.lock().unwrap_or_else(|e| e.into_inner());
                    p.lat_deg = *lat_deg;
                    p.lon_deg = *lon_deg;
                    p.height_m = *height_m;
                    p.satellites.clear(); // reset satellite list at start of each second
                }
                SimEvent::Satellite {
                    prn,
                    az_deg,
                    el_deg,
                    ..
                } => {
                    let mut p = prog2.lock().unwrap_or_else(|e| e.into_inner());
                    p.satellites.push(SatInfo {
                        prn: *prn,
                        az_deg: *az_deg,
                        el_deg: *el_deg,
                    });
                }
                SimEvent::Done => {
                    let mut p = prog2.lock().unwrap_or_else(|e| e.into_inner());
                    p.finished = true;
                }
                _ => {}
            }
            if let Some(f) = &orig_cb {
                f(event);
            }
        }));

        let thread = std::thread::Builder::new()
            .name("gps-sim".into())
            .spawn(move || self.run())
            .expect("failed to spawn simulation thread");

        SimulatorHandle {
            stop,
            progress,
            thread,
        }
    }

    /// Run the simulation until `duration` seconds have elapsed or the stop
    /// handle is set.  **Blocks** the calling thread.
    ///
    /// # Errors
    /// Returns the first error encountered (RINEX load, `HackRF` USB, etc.).
    pub fn run(mut self) -> Result<(), SimError> {
        let rx_ecef = llh_to_ecef(self.loc);

        // Resolve start time to GPS time.
        let g0 = resolve_start_time(self.start, &self.nav);

        // Choose the ephemeris set whose reference time is closest to g0.
        let ieph = best_eph_set(&self.nav, g0);

        // Apply ionospheric disable.
        if self.ionospheric_disable {
            self.nav.iono.valid = false;
        }
        // Apply leap second override.
        if let Some((week, day, delta)) = self.leap {
            self.nav.iono.dtls = delta;
            self.nav.iono.wnt = week;
            self.nav.iono.tot = day;
        }
        // Apply time override: shift all ephemeris TOC/TOE to the start time.
        if self.time_override {
            for eph_set in &mut self.nav.gps {
                for eph in eph_set.iter_mut() {
                    if eph.valid {
                        eph.toc = g0;
                        eph.toe = g0;
                    }
                }
            }
            for eph_set in &mut self.nav.beidou {
                for eph in eph_set.iter_mut() {
                    if eph.valid {
                        eph.toc = g0;
                        eph.toe = g0;
                    }
                }
            }
            for eph_set in &mut self.nav.galileo {
                for eph in eph_set.iter_mut() {
                    if eph.valid {
                        eph.toc = g0;
                        eph.toe = g0;
                    }
                }
            }
        }

        // Create shared interactive state and spawn keyboard thread if needed.
        // When an external state is injected (GUI mode), skip the stdin thread.
        let istate: Option<Arc<Mutex<InteractiveState>>> = if let Some(ext) = &self.external_istate
        {
            Some(Arc::clone(ext))
        } else if self.interactive {
            let s = Arc::new(Mutex::new(InteractiveState::default()));
            let s2 = s.clone();
            let stop2 = self.stop.clone();
            std::thread::Builder::new()
                .name("kbd-input".into())
                .spawn(move || keyboard_thread(s2, stop2))
                .expect("failed to spawn keyboard thread");
            Some(s)
        } else {
            None
        };

        // Open position log file if requested.
        let log_file: Option<BufWriter<std::fs::File>> = match self.log_path.as_deref() {
            Some(p) => {
                let f = std::fs::File::create(p)?;
                let mut w = BufWriter::new(f);
                writeln!(w, "time_s,lat_deg,lon_deg,height_m")?;
                Some(w)
            }
            None => None,
        };

        // Extract the output variant *before* matching so that `self` is not
        // partially moved — replace with a cheap sentinel so the rest of `self`
        // can still be passed by value to the dispatch methods.
        let output = std::mem::replace(&mut self.output, SdrOutput::Null);

        match output {
            SdrOutput::HackRf { gain_db, amp } => {
                self.run_hackrf(g0, rx_ecef, ieph, gain_db, amp, istate, log_file)
            }
            SdrOutput::IqFile { path } => self.run_file(g0, rx_ecef, ieph, &path, istate, log_file),
            SdrOutput::Null => self.run_null(g0, rx_ecef, ieph, istate, log_file),
            SdrOutput::PlutoSdr { host, gain_db } => {
                self.run_plutosdr(g0, rx_ecef, ieph, host, gain_db, istate, log_file)
            }
            SdrOutput::UdpStream { addr } => {
                self.run_udp(g0, rx_ecef, ieph, &addr, istate, log_file)
            }
            SdrOutput::TcpServer { port } => {
                self.run_tcp(g0, rx_ecef, ieph, port, istate, log_file)
            }
        }
    }

    // ── Output backends ───────────────────────────────────────────────────────

    #[expect(
        clippy::too_many_arguments,
        reason = "all parameters are required for the backend dispatch; cannot bundle without polluting caller"
    )]
    fn run_hackrf(
        self,
        g0: GpsTime,
        rx_ecef: [f64; 3],
        ieph: usize,
        gain_db: i32,
        amp: bool,
        istate: Option<Arc<Mutex<InteractiveState>>>,
        log_file: Option<BufWriter<std::fs::File>>,
    ) -> Result<(), SimError> {
        let mut dev = super::hackrf::GpsHackRf::open()?;
        dev.configure(
            gain_db,
            amp,
            self.ppb,
            self.hackrf_sample_rate,
            self.hackrf_center_freq,
            self.hackrf_baseband_filter,
        )?;
        let mut ep = dev.enter_tx()?;

        let fifo = IqFifo::new(8);
        let consumer = fifo.consumer;
        let producer = fifo.producer;

        let stop = self.stop.clone();
        let pause = self.pause.clone();
        let nav = self.nav.clone();
        let elev_mask = self.elev_mask;
        let duration = self.duration;
        let waypoints = self.waypoints;
        let blocked_prns = self.blocked_prns;
        let fixed_gain = self.fixed_gain;
        let use_beidou = self.use_beidou;
        let use_galileo = self.use_galileo;
        let emit: Arc<dyn Fn(SimEvent) + Send + Sync> = if let Some(f) = self.on_event {
            Arc::new(f)
        } else {
            Arc::new(|_| {})
        };
        let emit2 = emit.clone();

        // GPS generation thread.
        let gps_thread = std::thread::Builder::new()
            .name("gps-gen".into())
            .spawn(move || {
                generate_iq(
                    g0,
                    rx_ecef,
                    &waypoints,
                    istate.as_ref(),
                    &blocked_prns,
                    &nav,
                    ieph,
                    duration,
                    elev_mask,
                    &stop,
                    pause.as_deref(),
                    &producer,
                    &*emit,
                    log_file,
                    fixed_gain,
                    use_beidou,
                    use_galileo,
                );
                producer.shutdown();
            })
            .expect("failed to spawn GPS thread");

        // TX streaming thread.
        const MAX_INFLIGHT: usize = 4;
        let tx_thread = std::thread::Builder::new()
            .name("hackrf-tx".into())
            .spawn(move || {
                let mut in_flight = 0usize;
                loop {
                    // Fill the USB pipeline up to MAX_INFLIGHT.
                    while in_flight < MAX_INFLIGHT {
                        match consumer.dequeue() {
                            None => {
                                // Drain remaining completions before stopping.
                                while in_flight > 0 {
                                    ep.wait_next_complete(Duration::from_secs(5));
                                    in_flight -= 1;
                                }
                                return;
                            }
                            Some(buf) => {
                                // Cast i8 → u8 for USB (same bit pattern).
                                let data: Vec<u8> = buf.iter().map(|&b| b as u8).collect();
                                consumer.release(buf);
                                // Submit is non-blocking; it queues the transfer.
                                // nusb 0.2 accepts Vec<u8>.into() as a TransferBuffer.
                                ep.submit(data.into());
                                in_flight += 1;
                            }
                        }
                    }
                    // Wait for one completion before submitting more.
                    ep.wait_next_complete(Duration::from_secs(5));
                    in_flight -= 1;
                }
            })
            .expect("failed to spawn TX thread");

        gps_thread.join().ok();
        tx_thread.join().ok();
        dev.stop_tx()?;
        emit2(SimEvent::Done);
        Ok(())
    }

    fn run_file(
        self,
        g0: GpsTime,
        rx_ecef: [f64; 3],
        ieph: usize,
        path: &str,
        istate: Option<Arc<Mutex<InteractiveState>>>,
        log_file: Option<BufWriter<std::fs::File>>,
    ) -> Result<(), SimError> {
        let mut file = std::fs::File::create(path)?;

        let emit: Arc<dyn Fn(SimEvent) + Send + Sync> = if let Some(f) = self.on_event {
            Arc::new(f)
        } else {
            Arc::new(|_| {})
        };
        let emit2 = emit.clone();

        // Write to a Vec<i8> sink instead of a FIFO, then flush to disk.
        // No real-time gate for file output — run as fast as possible.
        let fifo = IqFifo::new(4);
        let consumer = fifo.consumer;
        let producer = fifo.producer;
        let stop = self.stop.clone();
        let pause = self.pause.clone();
        let nav = self.nav.clone();

        let waypoints = self.waypoints;
        let blocked_prns = self.blocked_prns;
        let duration = self.duration;
        let elev_mask = self.elev_mask;
        let fixed_gain = self.fixed_gain;
        let use_beidou = self.use_beidou;
        let use_galileo = self.use_galileo;
        let gps_thread = std::thread::Builder::new()
            .name("gps-gen".into())
            .spawn(move || {
                generate_iq(
                    g0,
                    rx_ecef,
                    &waypoints,
                    istate.as_ref(),
                    &blocked_prns,
                    &nav,
                    ieph,
                    duration,
                    elev_mask,
                    &stop,
                    pause.as_deref(),
                    &producer,
                    &*emit2,
                    log_file,
                    fixed_gain,
                    use_beidou,
                    use_galileo,
                );
                producer.shutdown();
            })
            .expect("failed to spawn GPS thread");

        while let Some(buf) = consumer.dequeue() {
            // Convert i8 slice to u8 for the file writer (same bit pattern).
            let bytes: Vec<u8> = buf.iter().map(|&b| b as u8).collect();
            file.write_all(&bytes)?;
            consumer.release(buf);
        }

        gps_thread.join().ok();
        emit(SimEvent::Done);
        Ok(())
    }

    #[expect(
        clippy::unnecessary_wraps,
        reason = "signature must match the other run_* backends which can fail"
    )]
    fn run_null(
        self,
        g0: GpsTime,
        rx_ecef: [f64; 3],
        ieph: usize,
        istate: Option<Arc<Mutex<InteractiveState>>>,
        log_file: Option<BufWriter<std::fs::File>>,
    ) -> Result<(), SimError> {
        let emit: Arc<dyn Fn(SimEvent) + Send + Sync> = if let Some(f) = self.on_event {
            Arc::new(f)
        } else {
            Arc::new(|_| {})
        };
        let emit2 = emit.clone();

        let fifo = IqFifo::new(2);
        let consumer = fifo.consumer;
        let producer = fifo.producer;
        let stop = self.stop.clone();
        let pause = self.pause.clone();
        let nav = self.nav.clone();

        let waypoints = self.waypoints;
        let blocked_prns = self.blocked_prns;
        let duration = self.duration;
        let elev_mask = self.elev_mask;
        let fixed_gain = self.fixed_gain;
        let use_beidou = self.use_beidou;
        let use_galileo = self.use_galileo;
        let gps_thread = std::thread::Builder::new()
            .name("gps-gen".into())
            .spawn(move || {
                generate_iq(
                    g0,
                    rx_ecef,
                    &waypoints,
                    istate.as_ref(),
                    &blocked_prns,
                    &nav,
                    ieph,
                    duration,
                    elev_mask,
                    &stop,
                    pause.as_deref(),
                    &producer,
                    &*emit2,
                    log_file,
                    fixed_gain,
                    use_beidou,
                    use_galileo,
                );
                producer.shutdown();
            })
            .expect("failed to spawn GPS thread");

        while let Some(buf) = consumer.dequeue() {
            consumer.release(buf); // discard
        }
        gps_thread.join().ok();
        emit(SimEvent::Done);
        Ok(())
    }

    // ── PlutoSDR backend ──────────────────────────────────────────────────────

    #[expect(
        clippy::too_many_arguments,
        reason = "signature must mirror caller dispatch; all parameters required"
    )]
    #[expect(
        clippy::unused_self,
        reason = "stub for unbuilt PlutoSDR feature; self would be used in a full impl"
    )]
    fn run_plutosdr(
        self,
        g0: GpsTime,
        rx_ecef: [f64; 3],
        ieph: usize,
        host: String,
        gain_db: i32,
        istate: Option<Arc<Mutex<InteractiveState>>>,
        log_file: Option<BufWriter<std::fs::File>>,
    ) -> Result<(), SimError> {
        let _unused = (g0, rx_ecef, ieph, host, gain_db, istate, log_file);
        Err(SimError::Config(
            "PlutoSDR support is not available in this build.".into(),
        ))
    }

    // ── UDP streaming backend ─────────────────────────────────────────────────

    fn run_udp(
        self,
        g0: GpsTime,
        rx_ecef: [f64; 3],
        ieph: usize,
        addr: &str,
        istate: Option<Arc<Mutex<InteractiveState>>>,
        log_file: Option<BufWriter<std::fs::File>>,
    ) -> Result<(), SimError> {
        let sock = UdpSocket::bind("0.0.0.0:0")
            .map_err(|e| SimError::Network(format!("UDP bind: {e}")))?;
        sock.connect(addr)
            .map_err(|e| SimError::Network(format!("UDP connect to {addr}: {e}")))?;

        let emit: Arc<dyn Fn(SimEvent) + Send + Sync> = if let Some(f) = self.on_event {
            Arc::new(f)
        } else {
            Arc::new(|_| {})
        };
        let emit2 = emit.clone();

        let fifo = IqFifo::new(4);
        let consumer = fifo.consumer;
        let producer = fifo.producer;
        let stop = self.stop.clone();
        let pause = self.pause.clone();
        let nav = self.nav.clone();
        let waypoints = self.waypoints;
        let blocked_prns = self.blocked_prns;
        let duration = self.duration;
        let elev_mask = self.elev_mask;
        let fixed_gain = self.fixed_gain;
        let use_beidou = self.use_beidou;
        let use_galileo = self.use_galileo;

        let gps_thread = std::thread::Builder::new()
            .name("gps-gen".into())
            .spawn(move || {
                generate_iq(
                    g0,
                    rx_ecef,
                    &waypoints,
                    istate.as_ref(),
                    &blocked_prns,
                    &nav,
                    ieph,
                    duration,
                    elev_mask,
                    &stop,
                    pause.as_deref(),
                    &producer,
                    &*emit2,
                    log_file,
                    fixed_gain,
                    use_beidou,
                    use_galileo,
                );
                producer.shutdown();
            })
            .expect("failed to spawn GPS thread");

        // Chunk HackRF-sized buffers into 32 768-byte UDP datagrams.
        const UDP_CHUNK: usize = 32_768;
        let mut carry: Vec<u8> = Vec::new();
        while let Some(buf) = consumer.dequeue() {
            let bytes: Vec<u8> = buf.iter().map(|&b| b as u8).collect();
            consumer.release(buf);
            let combined: Vec<u8> = carry.drain(..).chain(bytes).collect();
            let mut offset = 0;
            while offset + UDP_CHUNK <= combined.len() {
                if let Some(chunk) = combined.get(offset..offset + UDP_CHUNK) {
                    sock.send(chunk)
                        .map_err(|e| SimError::Network(format!("UDP send: {e}")))?;
                }
                offset += UDP_CHUNK;
            }
            if let Some(remainder) = combined.get(offset..) {
                carry.extend_from_slice(remainder);
            }
        }
        gps_thread.join().ok();
        emit(SimEvent::Done);
        Ok(())
    }

    // ── TCP server backend ────────────────────────────────────────────────────

    fn run_tcp(
        self,
        g0: GpsTime,
        rx_ecef: [f64; 3],
        ieph: usize,
        port: u16,
        istate: Option<Arc<Mutex<InteractiveState>>>,
        log_file: Option<BufWriter<std::fs::File>>,
    ) -> Result<(), SimError> {
        let listener = TcpListener::bind(format!("0.0.0.0:{port}"))
            .map_err(|e| SimError::Network(format!("TCP bind port {port}: {e}")))?;

        log::info!("TCP: waiting for client on port {port}");
        let (mut client, peer) = listener
            .accept()
            .map_err(|e| SimError::Network(format!("TCP accept: {e}")))?;
        log::info!("TCP: client connected from {peer}");

        let emit: Arc<dyn Fn(SimEvent) + Send + Sync> = if let Some(f) = self.on_event {
            Arc::new(f)
        } else {
            Arc::new(|_| {})
        };
        let emit2 = emit.clone();

        let fifo = IqFifo::new(4);
        let consumer = fifo.consumer;
        let producer = fifo.producer;
        let stop = self.stop.clone();
        let pause = self.pause.clone();
        let nav = self.nav.clone();
        let waypoints = self.waypoints;
        let blocked_prns = self.blocked_prns;
        let duration = self.duration;
        let elev_mask = self.elev_mask;
        let fixed_gain = self.fixed_gain;
        let use_beidou = self.use_beidou;
        let use_galileo = self.use_galileo;

        let gps_thread = std::thread::Builder::new()
            .name("gps-gen".into())
            .spawn(move || {
                generate_iq(
                    g0,
                    rx_ecef,
                    &waypoints,
                    istate.as_ref(),
                    &blocked_prns,
                    &nav,
                    ieph,
                    duration,
                    elev_mask,
                    &stop,
                    pause.as_deref(),
                    &producer,
                    &*emit2,
                    log_file,
                    fixed_gain,
                    use_beidou,
                    use_galileo,
                );
                producer.shutdown();
            })
            .expect("failed to spawn GPS thread");

        while let Some(buf) = consumer.dequeue() {
            let bytes: Vec<u8> = buf.iter().map(|&b| b as u8).collect();
            consumer.release(buf);
            if client.write_all(&bytes).is_err() {
                break;
            } // client disconnected
        }
        gps_thread.join().ok();
        emit(SimEvent::Done);
        Ok(())
    }
}

// ── IQ generation loop ────────────────────────────────────────────────────────

/// Core GPS signal generation loop.
///
/// Runs in the GPS thread.  Generates 300 000 IQ samples every 100 ms,
/// writing full 262 144-byte buffers to the FIFO for the TX thread to consume.
///
/// # Parameters
/// - `grx`:        Starting GPS time.
/// - `rx_ecef`:    Fixed receiver ECEF position (m) — used when `waypoints` is empty.
/// - `waypoints`:  Per-step ECEF positions from a motion CSV.  Each entry covers
///   one 100 ms step.  If non-empty, overrides `rx_ecef` and caps
///   the simulation to `waypoints.len()` steps.
/// - `interactive`:   Shared state for keyboard-controlled position update.
/// - `blocked_prns`:  PRN numbers (1–32) to exclude from simulation.
/// - `nav`:           Parsed RINEX navigation data.
/// - `ieph`:          Which ephemeris set to use (index into `nav.eph`).
/// - `duration`:      Total simulation time (seconds).
/// - `elev_mask`:     Minimum satellite elevation (radians).
/// - `stop`:          Atomic flag; set to `true` to stop early.
/// - `pause`:         Optional atomic flag; while `true`, position is frozen but signal continues.
/// - `producer`:      FIFO producer endpoint.
/// - `emit`:          Event callback.
/// - `log_file`:      Optional CSV position log file.
/// - `fixed_gain`:    When `Some(v)`, override per-satellite gain with constant `v` (disables antenna pattern / path loss).
/// - `use_beidou`:    When `true`, include `BeiDou` B1C channels.
/// - `use_galileo`:   When `true`, include Galileo E1-B channels.
#[expect(
    clippy::too_many_arguments,
    reason = "core IQ generation loop; all parameters required"
)]
#[expect(
    clippy::too_many_lines,
    reason = "hot path inner loop; extracting sub-functions would add overhead"
)]
#[expect(
    clippy::indexing_slicing,
    reason = "hot-path indexing is bounds-safe by construction: itable&511<512, buf_pos<HACKRF_BUF_BYTES, iword%60<60, chip_idx<code_len"
)]
fn generate_iq(
    mut grx: GpsTime,
    rx_ecef: [f64; 3],
    waypoints: &[[f64; 3]],
    interactive: Option<&Arc<Mutex<InteractiveState>>>,
    blocked_prns: &[u8],
    nav: &NavData,
    ieph: usize,
    duration: u32,
    elev_mask: f64,
    stop: &AtomicBool,
    pause: Option<&AtomicBool>,
    producer: &super::fifo::IqProducer,
    emit: &dyn Fn(SimEvent),
    mut log_file: Option<BufWriter<std::fs::File>>,
    fixed_gain: Option<i32>,
    use_beidou: bool,
    use_galileo: bool,
) {
    // When a motion file is provided, run for exactly the number of waypoints.
    // The `duration` cap only applies to fixed-position (no motion file) runs.
    let total_steps = if waypoints.is_empty() {
        duration as usize * 10
    } else {
        waypoints.len()
    };
    let gps_eph_set = nav.gps.get(ieph).map(|s| s.as_slice()).unwrap_or(&[]);
    let bds_eph_set = nav.beidou.get(ieph).map(|s| s.as_slice()).unwrap_or(&[]);
    let gal_eph_set = nav.galileo.get(ieph).map(|s| s.as_slice()).unwrap_or(&[]);

    // Starting ECEF position: first waypoint, interactive start, or fixed location.
    let rx_ecef0 = waypoints.first().copied().unwrap_or(rx_ecef);

    // Mutable current position for interactive/waypoint modes.
    let mut cur_ecef = rx_ecef0;

    // ── Initial channel allocation ────────────────────────────────────────────
    // GPS channels (always enabled).
    let mut channels: Vec<Channel> = (1u8..=32)
        .filter(|&prn| gps_eph_set.get(prn as usize - 1).is_some_and(|e| e.valid))
        .filter(|prn| !blocked_prns.contains(prn))
        .filter_map(|prn| {
            let eph = gps_eph_set.get(prn as usize - 1)?;
            Channel::new(Constellation::Gps, prn, eph, &nav.iono, grx, rx_ecef0)
        })
        .filter(|ch| ch.azel[1] >= elev_mask)
        .collect();

    // BeiDou B1C channels (optional).
    if use_beidou {
        let bds_channels = (1u8..=63)
            .filter(|&prn| bds_eph_set.get(prn as usize - 1).is_some_and(|e| e.valid))
            .filter(|prn| !blocked_prns.contains(prn))
            .filter_map(|prn| {
                let eph = bds_eph_set.get(prn as usize - 1)?;
                Channel::new(Constellation::BeiDou, prn, eph, &nav.iono, grx, rx_ecef0)
            })
            .filter(|ch| ch.azel[1] >= elev_mask);
        channels.extend(bds_channels);
    }

    // Galileo E1-B channels (optional).
    if use_galileo {
        let gal_channels = (1u8..=36)
            .filter(|&prn| gal_eph_set.get(prn as usize - 1).is_some_and(|e| e.valid))
            .filter(|prn| !blocked_prns.contains(prn))
            .filter_map(|prn| {
                let eph = gal_eph_set.get(prn as usize - 1)?;
                Channel::new(Constellation::Galileo, prn, eph, &nav.iono, grx, rx_ecef0)
            })
            .filter(|ch| ch.azel[1] >= elev_mask);
        channels.extend(gal_channels);
    }

    // Cap total channels.
    channels.truncate(MAX_CHANNELS);

    emit(SimEvent::Status(format!(
        "Tracking {} satellites at GPS week {} sec {:.1}\n",
        channels.len(),
        grx.week,
        grx.sec
    )));

    // Pre-compute linear antenna gain table (37 elevations at 5° steps).
    let ant = ant_pattern_linear();

    // Acquire the first output buffer from the FIFO free pool.
    let mut buf = producer.acquire();
    let mut buf_pos = 0usize;

    // ── Main simulation loop ──────────────────────────────────────────────────
    let mut step = 0usize;
    let mut wall_step = 0u64; // counts every iteration, including paused ones
    while step < total_steps {
        if stop.load(Ordering::Relaxed) {
            break;
        }

        let paused = pause.is_some_and(|p| p.load(Ordering::Relaxed));

        // Receiver position this step.
        let pos = if let Some(ist) = &interactive {
            // Interactive mode: accumulate position from bearing + speed.
            #[expect(
                clippy::unwrap_used,
                reason = "mutex poison means another thread panicked; no recovery possible"
            )]
            let state = ist.lock().unwrap();
            let bearing_rad = state.bearing_deg.to_radians();
            let dx_n = state.speed_ms * bearing_rad.cos() * STEP_SECS;
            let dx_e = state.speed_ms * bearing_rad.sin() * STEP_SECS;
            let dx_u = state.vert_speed_ms * STEP_SECS;
            drop(state);
            // NEU → ECEF via transpose of the LTC rotation matrix.
            let loc = super::coords::ecef_to_llh(cur_ecef);
            let ltc = super::coords::ltc_matrix(loc);
            cur_ecef[0] += ltc[0][0] * dx_n + ltc[1][0] * dx_e + ltc[2][0] * dx_u;
            cur_ecef[1] += ltc[0][1] * dx_n + ltc[1][1] * dx_e + ltc[2][1] * dx_u;
            cur_ecef[2] += ltc[0][2] * dx_n + ltc[1][2] * dx_e + ltc[2][2] * dx_u;
            cur_ecef
        } else if !waypoints.is_empty() {
            waypoints
                .get(step.min(waypoints.len() - 1))
                .copied()
                .unwrap_or(rx_ecef)
        } else {
            rx_ecef
        };

        let step_start = Instant::now();

        // ── Per-channel gain for this step ────────────────────────────────────
        // Amplitude = path-loss × antenna gain, scaled to an integer multiplier.
        // Path loss is normalised to GPS nominal altitude (20 200 km).
        let gains: Vec<i32> = channels
            .iter()
            .map(|ch| {
                if let Some(fg) = fixed_gain {
                    fg
                } else {
                    let el_deg = ch.azel[1].to_degrees();
                    let boresight_idx = ((90.0 - el_deg) / 5.0) as usize;
                    let ant_g = ant.get(boresight_idx.min(36)).copied().unwrap_or(1.0);
                    // LUT amplitude is ±250.  With ≤12 channels and gain=1 the
                    // accumulator stays within ±3000, then >>4 gives ±187 which
                    // clips harmlessly to i8.  Scale by ant_g (∈ [0.167,1.0]) via
                    // a Q4 fixed-point multiplier so low-elevation SVs are quieter.
                    // gain range: [2..16] → max sum ≤ 12×250×16 = 48 000, >>4 = 3000.
                    (ant_g * 16.0) as i32
                }
            })
            .collect();

        // ── Inner sample loop (hot path) ──────────────────────────────────────
        // Force-initialise the lookup tables before entering the loop.
        let cos_tab = &*COS_TABLE;
        let sin_tab = &*SIN_TABLE;

        for _ in 0..SAMPLES_PER_STEP {
            let mut i_acc = 0i32;
            let mut q_acc = 0i32;

            for (ch, &gain) in channels.iter_mut().zip(gains.iter()) {
                // Look up carrier phase.
                let itable = (ch.carr_phase * 512.0) as usize & 511;
                let iq_sign = ch.data_bit * ch.code_ca;
                i_acc += iq_sign * cos_tab[itable] as i32 * gain;
                q_acc += iq_sign * sin_tab[itable] as i32 * gain;

                // ── Advance code phase ────────────────────────────────────────
                ch.code_phase += ch.f_code * DT;
                let code_len_f = ch.code_len as f64;
                if ch.code_phase >= code_len_f {
                    ch.code_phase -= code_len_f;
                    ch.icode += 1;
                    if ch.icode >= 20 {
                        // Start of a new navigation bit.
                        ch.icode = 0;
                        ch.ibit += 1;
                        if ch.ibit >= 30 {
                            // Start of a new navigation word.
                            ch.ibit = 0;
                            ch.iword = (ch.iword + 1) % 60;
                        }
                        ch.data_bit = ((ch.dwrd[ch.iword] >> (29 - ch.ibit)) & 1) as i32 * 2 - 1;
                    }
                    let chip_idx = ch.code_phase as usize % ch.code_len;
                    ch.code_ca = ch.code[chip_idx] as i32;
                }

                // ── Advance carrier phase ─────────────────────────────────────
                ch.carr_phase += ch.f_carr * DT;
                if ch.carr_phase >= 1.0 {
                    ch.carr_phase -= 1.0;
                } else if ch.carr_phase < 0.0 {
                    ch.carr_phase += 1.0;
                }
            }

            // ── Quantise to 8-bit signed and write interleaved I/Q ────────────
            // Right-shift by 4 to bring the summed value into the ±128 range.
            buf[buf_pos] = (i_acc >> 4) as i8;
            buf[buf_pos + 1] = (q_acc >> 4) as i8;
            buf_pos += 2;

            if buf_pos >= HACKRF_BUF_BYTES {
                // Buffer is full — hand it to the TX thread and get a fresh one.
                producer.enqueue(std::mem::replace(&mut buf, producer.acquire()));
                buf_pos = 0;
            }
        }

        // ── Real-time gate ────────────────────────────────────────────────────
        // Sleep for the remainder of the 100 ms step so that the GPS thread
        // generates samples at the correct wall-clock rate when writing to
        // HackRF.  For IQ-file output this gate is not necessary, but keeping
        // it here avoids overflowing the FIFO.
        let elapsed = step_start.elapsed();
        let target = Duration::from_millis(100);
        if elapsed < target {
            std::thread::sleep(target - elapsed);
        }

        // ── Advance GPS time ──────────────────────────────────────────────────
        grx = grx.add_secs(STEP_SECS);

        // ── Update satellite positions every step ─────────────────────────────
        for ch in &mut channels {
            let eph_slice = match ch.constellation {
                Constellation::Gps => gps_eph_set,
                Constellation::BeiDou => bds_eph_set,
                Constellation::Galileo => gal_eph_set,
            };
            if let Some(eph) = eph_slice.get(ch.prn as usize - 1) {
                if let Some(rho) = super::orbit::compute_range(eph, &nav.iono, grx, pos) {
                    // Update Doppler from the range rate.
                    let rho_rate = rho.rate;
                    ch.f_carr = -rho_rate / LAMBDA_L1;
                    // Code rate = nominal chip rate + Doppler-scaled code correction.
                    // The carrier-to-code ratio (1540 for GPS L1) maps carrier Doppler
                    // to code Doppler.  For BeiDou and Galileo the ratio differs slightly
                    // (B1C: 1540.0, E1: 1540.0 at 1575.42 MHz) but is unity at L1 for
                    // all three; reuse CARR_TO_CODE as a good approximation.
                    ch.f_code = ch.chip_rate + ch.f_carr / CARR_TO_CODE;
                    ch.azel = rho.azel;
                }
            }
        }

        // ── Regenerate navigation message every 30 s ──────────────────────────
        // One navigation message cycle = 300 steps × 0.1 s = 30 s.
        if step % 300 == 299 {
            for ch in &mut channels {
                ch.ipage = (ch.ipage + 1) % 25;
                ch.dwrd = generate_nav_msg(&ch.sbf, grx, ch.ipage);
            }
        }

        // ── Progress event (every step) ───────────────────────────────────────
        emit(SimEvent::Progress {
            current_step: step,
            total_steps,
            bytes_sent: wall_step.saturating_mul(600_000),
        });

        // ── Emit position and satellite events ────────────────────────────────
        // Use step % 10 == 0 normally; when paused, always emit so the map stays live.
        if step % 10 == 0 || paused {
            let rx_llh = super::coords::ecef_to_llh(pos);
            let lat_deg = rx_llh.lat_rad.to_degrees();
            let lon_deg = rx_llh.lon_rad.to_degrees();
            let height_m = rx_llh.height_m;
            emit(SimEvent::Position {
                lat_deg,
                lon_deg,
                height_m,
            });
            for ch in &channels {
                emit(SimEvent::Satellite {
                    prn: ch.prn,
                    az_deg: ch.azel[0].to_degrees(),
                    el_deg: ch.azel[1].to_degrees(),
                    active: true,
                });
            }
            // Feature 6: write to position log (every second = every 10 steps).
            if let Some(ref mut log) = log_file {
                let time_s = step as f64 * STEP_SECS;
                writeln!(log, "{time_s:.1},{lat_deg:.8},{lon_deg:.8},{height_m:.3}").ok();
            }
        }

        // Advance wall clock every iteration; only advance route position when not paused.
        wall_step += 1;
        if !paused {
            step += 1;
        }
    }

    // ── Flush the partial final buffer ────────────────────────────────────────
    if buf_pos > 0 {
        // Zero-pad the remainder to a full HackRF buffer.
        for b in &mut buf[buf_pos..] {
            *b = 0;
        }
        producer.enqueue(buf);
    }
}

// ── Keyboard input thread ─────────────────────────────────────────────────────

/// Keyboard input thread for interactive mode.
///
/// Reads raw key events and updates the shared [`InteractiveState`].
/// Terminates when `stop` is set or the user presses `x`.
///
/// Key bindings (matching multi-sdr-gps-sim):
/// - `a` / `d` — bearing −0.127° / +0.127°
/// - `e` / `q` — speed +1 m/s / −1 m/s
/// - `w` / `s` — vertical speed +1 m/s / −1 m/s
/// - `x`       — stop simulation
#[expect(
    clippy::needless_pass_by_value,
    reason = "Arc ownership is required for the spawned thread to keep the values alive"
)]
fn keyboard_thread(state: Arc<Mutex<InteractiveState>>, stop: Arc<AtomicBool>) {
    use crossterm::event::{self, Event, KeyCode, KeyEventKind};
    if crossterm::terminal::enable_raw_mode().is_err() {
        return;
    }
    loop {
        if stop.load(Ordering::Relaxed) {
            break;
        }
        if matches!(event::poll(Duration::from_millis(50)), Ok(true)) {
            if let Ok(Event::Key(key)) = event::read() {
                // Only process key-press events (ignore repeats on some terminals).
                if key.kind == KeyEventKind::Release {
                    continue;
                }
                #[expect(
                    clippy::unwrap_used,
                    reason = "mutex poison means another thread panicked; no recovery"
                )]
                let mut s = state.lock().unwrap();
                match key.code {
                    KeyCode::Char('a') => s.bearing_deg = (s.bearing_deg - 0.127 + 360.0) % 360.0,
                    KeyCode::Char('d') => s.bearing_deg = (s.bearing_deg + 0.127) % 360.0,
                    KeyCode::Char('e') => s.speed_ms += 1.0,
                    KeyCode::Char('q') => s.speed_ms = (s.speed_ms - 1.0).max(0.0),
                    KeyCode::Char('w') => s.vert_speed_ms += 1.0,
                    KeyCode::Char('s') => s.vert_speed_ms -= 1.0,
                    KeyCode::Char('x') => {
                        drop(s);
                        stop.store(true, Ordering::Relaxed);
                        break;
                    }
                    _ => {}
                }
            }
        }
    }
    crossterm::terminal::disable_raw_mode().ok();
}

// ── Motion CSV loader ─────────────────────────────────────────────────────────

/// Load a motion CSV file into a vector of ECEF waypoints.
///
/// Expected format (one row per 100 ms step):
/// ```text
/// time, ecef_x, ecef_y, ecef_z
/// 0.0, 3877216.643, 327184.585, 5036843.585
/// 0.1, 3877216.511, 327184.574, 5036843.686
/// …
/// ```
/// The time column is accepted but ignored; row order determines timing.
/// Blank lines and lines that cannot be parsed are silently skipped.
fn load_motion_csv(path: &str) -> Result<Vec<[f64; 3]>, SimError> {
    let content = std::fs::read_to_string(path).map_err(SimError::Io)?;
    let mut waypoints = Vec::new();
    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let mut parts = line.splitn(4, ',');
        let _time: Option<&str> = parts.next(); // skip time column
        let Some(x) = parts.next().and_then(|s| s.trim().parse::<f64>().ok()) else {
            continue;
        };
        let Some(y) = parts.next().and_then(|s| s.trim().parse::<f64>().ok()) else {
            continue;
        };
        let Some(z) = parts.next().and_then(|s| s.trim().parse::<f64>().ok()) else {
            continue;
        };
        waypoints.push([x, y, z]);
    }
    if waypoints.is_empty() {
        return Err(SimError::Rinex(format!(
            "motion file '{path}' contains no valid waypoints"
        )));
    }
    Ok(waypoints)
}

// ── Helpers ───────────────────────────────────────────────────────────────────

/// Convert `StartTime` to an absolute `GpsTime`.
fn resolve_start_time(start: StartTime, nav: &NavData) -> GpsTime {
    match start {
        StartTime::Gps(g) => g,
        StartTime::DateTime(d) => d.to_gps(),
        StartTime::Now => {
            // Convert system time → GPS time.
            // GPS epoch = Unix epoch + 315 964 800 s; add 18 s for current leap seconds.
            let unix_secs = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs_f64();
            let gps_secs = unix_secs - 315_964_800.0 + 18.0;
            let week = (gps_secs / GpsTime::SECS_PER_WEEK) as i32;
            let sec = gps_secs % GpsTime::SECS_PER_WEEK;
            // If the RINEX file has a specific epoch, use it instead of now.
            let rinex_time = best_rinex_time(nav);
            if rinex_time.week > 0 {
                rinex_time
            } else {
                GpsTime { week, sec }
            }
        }
    }
}

/// Return the GPS time of the first valid satellite in the first GPS ephemeris set.
fn best_rinex_time(nav: &NavData) -> GpsTime {
    nav.gps
        .first()
        .and_then(|set| set.iter().find(|e| e.valid).map(|e| e.toe))
        .unwrap_or_default()
}

/// Select the GPS ephemeris set whose reference time is closest to `g0`.
fn best_eph_set(nav: &NavData, g0: GpsTime) -> usize {
    nav.gps
        .iter()
        .enumerate()
        .min_by_key(|(_, set)| {
            set.iter()
                .filter(|e| e.valid)
                .map(|e| e.toe.sub(g0).abs() as i64)
                .min()
                .unwrap_or(i64::MAX)
        })
        .map(|(i, _)| i)
        .unwrap_or(0)
}
