//! Types shared between the simulation worker thread and the UI.

/// How the simulated IQ samples are delivered.
#[derive(Clone, Default, PartialEq, Eq)]
pub enum SimOutputType {
    /// Transmit via `HackRF` One (USB).
    #[default]
    HackRf,
    /// Write raw 8-bit IQ samples to a file.
    IqFile,
    /// Stream IQ samples over UDP.
    Udp,
    /// Stream IQ samples over TCP (server mode).
    Tcp,
    /// Discard output (testing / benchmark).
    Null,
}

/// Satellite visibility snapshot for UI display.
#[derive(Clone, Debug)]
pub struct SimSatInfo {
    /// PRN number (1–32).
    pub prn:    u8,
    /// Azimuth angle in degrees (0 = North, clockwise).
    pub az_deg: f64,
    /// Elevation angle in degrees above the horizon.
    pub el_deg: f64,
}

/// Settings passed from the UI to the simulation thread.
#[derive(Clone)]
pub struct SimSettings {
    /// Output sink selector.
    pub output_type: SimOutputType,
    /// Path to the IQ output file (used when `output_type == IqFile`).
    pub iq_file_path: String,
    /// UDP destination address (used when `output_type == Udp`), e.g. `"127.0.0.1:4567"`.
    pub udp_addr: String,
    /// TCP server port (used when `output_type == Tcp`).
    pub tcp_port: u16,
    /// Baseband sampling frequency in Hz (must be ≥ 1 000 000).
    pub frequency: usize,
    /// `HackRF` TX VGA gain in dB (0–47).
    pub txvga_gain: u16,
    /// Whether to enable the `HackRF` RF pre-amplifier.
    pub amp_enable: bool,
    /// Scenario start time: `"now"`, `"YYYY/MM/DD,hh:mm:ss"`, or `None` to
    /// use the first epoch in the RINEX file.
    pub start_time: Option<String>,
    /// When `true`, overwrite all TOC/TOE values in the ephemeris to match
    /// the scenario start time.
    pub time_override: bool,
    /// When `true`, disable the ionospheric delay model (useful for spacecraft
    /// scenarios above the ionosphere).
    pub ionospheric_disable: bool,
    /// When `Some`, disable path-loss calculations and hold all satellite
    /// signals at this constant gain level.
    pub fixed_gain: Option<i32>,
    /// RF centre frequency in Hz transmitted by the `HackRF`.
    /// Default is GPS L1 C/A (1 575 420 000 Hz).
    pub center_frequency: u64,
    /// Baseband filter bandwidth in Hz, or `None` to let `set_sample_rate_auto`
    /// choose the optimal value automatically.
    pub baseband_filter: Option<u32>,
    /// Leap second override: `Some((gps_week, day_of_week_1_to_7, delta_leap_secs))`.
    pub leap: Option<(i32, i32, i32)>,
    /// Oscillator offset in parts-per-billion.
    pub ppb: i32,
    /// Minimum satellite elevation angle in degrees (0 = no mask).
    pub elevation_mask_deg: f64,
    /// PRN numbers (1–32) to exclude from the simulation.
    pub blocked_prns: Vec<u8>,
    /// When `Some`, write a CSV position log to this file path.
    pub log_path: Option<String>,
}

/// Shared simulation progress state; updated by the worker, read by the UI.
#[derive(Default, Clone)]
pub struct SimState {
    /// Current run-state of the simulation.
    pub status: SimStatus,
    /// Simulation step index (each step ≈ 100 ms of GPS time).
    pub current_step: usize,
    /// Total number of steps in the loaded motion file.
    pub total_steps: usize,
    /// Cumulative bytes transferred to the `HackRF`.
    pub bytes_sent: u64,
    /// Human-readable error message, populated when `status == Error`.
    pub error: Option<String>,
    /// Number of completed loop passes (static looping simulator only; 0 otherwise).
    pub loop_count: usize,
    /// Most-recent receiver latitude in decimal degrees.
    pub lat_deg: f64,
    /// Most-recent receiver longitude in decimal degrees.
    pub lon_deg: f64,
    /// Most-recent receiver height above the WGS-84 ellipsoid in metres.
    pub height_m: f64,
    /// Currently-visible satellites (updated once per second).
    pub satellites: Vec<SimSatInfo>,
}

/// Run-state of the GPS signal simulation.
#[derive(Default, Clone, PartialEq, Eq)]
pub enum SimStatus {
    #[default]
    Idle,
    Running,
    Done,
    /// Simulation was halted cleanly by the user.
    Stopped,
    Error,
}
