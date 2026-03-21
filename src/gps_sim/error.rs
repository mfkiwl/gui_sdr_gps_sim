//! Unified error type for the GPS simulator.

use thiserror::Error;

/// All errors that can occur during simulation.
///
/// Errors flow upward from sub-modules and are unified here so callers
/// only need to handle a single type.
#[derive(Debug, Error)]
pub enum SimError {
    /// Malformed or unsupported RINEX navigation file.
    #[error("RINEX parse error: {0}")]
    Rinex(String),

    /// No valid ephemeris records were found in the RINEX file.
    #[error("No ephemeris data loaded — check RINEX file and start time")]
    NoEphemeris,

    /// Standard I/O error (file open, read, etc.).
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    /// `HackRF` USB driver error.
    #[error("HackRF error: {0}")]
    HackRf(#[from] super::hackrf::HackrfError),

    /// Simulation was stopped externally via the stop handle.
    #[error("Simulation aborted by stop signal")]
    Aborted,

    /// Configuration or feature error (e.g., optional feature not compiled in).
    #[error("Configuration error: {0}")]
    Config(String),

    /// Network I/O error (UDP/TCP streaming, `PlutoSDR` IIO network).
    #[error("Network error: {0}")]
    Network(String),
}
