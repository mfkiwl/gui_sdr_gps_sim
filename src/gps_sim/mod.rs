//! GPS L1 C/A baseband signal simulator.
//!
//! Generates GPS L1 C/A baseband IQ samples from a RINEX navigation file and
//! transmits them via `HackRF` One (or writes to a file).  A nearby GPS receiver
//! will lock onto the simulated signal and report the spoofed position.
//!
//! # Quick start
//!
//! ```no_run
//! use gui_sdr_gps_sim::gps_sim::{Simulator, Location, SdrOutput};
//!
//! Simulator::builder()
//!     .rinex("brdc0010.24n")
//!     .location(Location::degrees(52.3676, 4.9041, 5.0)) // Amsterdam
//!     .duration_secs(300)
//!     .output(SdrOutput::HackRf { gain_db: 20, amp: false })
//!     .build().unwrap()
//!     .run().unwrap();
//! ```
//!
//! # Signal chain
//!
//! ```text
//! RINEX nav file
//!   → Ephemeris  → Channel allocator (≤12 visible SVs)
//!                       → Per-sample IQ accumulation loop (100 ms steps)
//!                             → FIFO (8 × 262 KB buffers)
//!                                   → TX thread → HackRF One @ 1575.42 MHz
//! ```
//!
//! # Feature flags
//! *(none yet — all features are compiled unconditionally)*

pub mod codegen;
pub mod coords;
pub mod error;
pub mod fifo;
pub mod hackrf;
pub mod ionosphere;
pub mod navmsg;
pub mod orbit;
pub mod rinex;
pub mod signal;
pub mod troposphere;
pub mod types;

// Internal modules — not part of the public API.
mod channel;
mod sim;

// ── Re-exports ────────────────────────────────────────────────────────────────

pub use error::SimError;
pub use rinex::NavData;
pub use sim::{
    InteractiveState, SatInfo, SimEvent, SimProgress, Simulator, SimulatorBuilder, SimulatorHandle,
};
pub use types::{GpsTime, Location, StartTime, UtcDate};

/// Selects where the simulator sends its IQ samples.
#[derive(Debug, Clone)]
pub enum SdrOutput {
    /// Stream live IQ samples to a connected `HackRF` One via USB.
    ///
    /// - `gain_db`: TX VGA gain, 0–47 dB.
    /// - `amp`:     Enable the external RF amplifier (adds ~11 dB; use carefully).
    HackRf { gain_db: i32, amp: bool },

    /// Write interleaved signed 8-bit I/Q samples to a binary file.
    ///
    /// The file can be replayed with `hackrf_transfer -t <path>` or validated
    /// with `gnss-sdr`.
    IqFile { path: String },

    /// Discard all generated samples (useful for benchmarking and testing).
    Null,

    /// Stream to an ADALM-PLUTO SDR via its IIO network daemon (iiod, port 30431).
    ///
    /// The `PlutoSDR` must be connected via USB (RNDIS/CDC-ECM network adapter,
    /// default IP 192.168.2.1) or reachable over the network.  Samples are
    /// converted from signed 8-bit to 16-bit before transmission.
    ///
    /// Requires the `plutosdr` Cargo feature: `cargo build --features plutosdr`.
    PlutoSdr { host: String, gain_db: i32 },

    /// Stream raw signed 8-bit IQ samples as UDP datagrams.
    ///
    /// Each datagram carries `32 768` interleaved I/Q bytes (16 384 complex
    /// samples).  `addr` is the destination `"host:port"` (e.g. `"127.0.0.1:1234"`).
    /// Compatible with GNU Radio's UDP Source block in 8-bit mode.
    UdpStream { addr: String },

    /// Serve raw signed 8-bit IQ samples over a TCP connection.
    ///
    /// The simulator binds `0.0.0.0:<port>` and waits for exactly one client
    /// before generating samples.  Compatible with GNU Radio's TCP Source block
    /// and `nc -l <port> | hackrf_transfer -t /dev/stdin`.
    TcpServer { port: u16 },
}
