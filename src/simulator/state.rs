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
