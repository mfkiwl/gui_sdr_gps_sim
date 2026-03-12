//! Types shared between the simulation worker thread and the UI.

/// Settings passed from the UI to the simulation thread.
#[derive(Clone)]
pub struct SimSettings {
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
    /// the scenario start time (equivalent to the `-T` flag in anywhere-sdr).
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
    /// Corresponds to the `-l` flag in anywhere-sdr.
    pub leap: Option<(i32, i32, i32)>,
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
