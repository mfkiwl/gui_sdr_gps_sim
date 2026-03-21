//! Per-satellite tracking state for one simulation channel.
//!
//! A [`Channel`] holds all the mutable state needed to advance one satellite's
//! signal contribution through the IQ accumulation loop:
//! - Carrier and code phase accumulators
//! - Navigation message word ring
//! - C/A code sequence (bipolar ±1)
//! - Current data bit and code chip state

use super::types::{Ephemeris, IonoUtc, GpsTime, consts::{CODE_FREQ_CA, SPEED_OF_LIGHT}};
use super::codegen;
use super::navmsg;
use super::orbit::{compute_range, RangeResult};

/// Simulation state for one tracked GPS satellite.
///
/// All phase accumulators are floating-point so that small fractional
/// increments (Doppler, sub-chip offsets) accumulate correctly over many
/// simulation steps without integer truncation error.
#[derive(Clone)]
pub struct Channel {
    /// PRN number (1–32).  0 indicates an inactive channel slot.
    pub prn: u8,

    /// Bipolar ±1 C/A code, one chip per element (1023 elements).
    pub ca: [i8; 1023],

    /// Carrier Doppler frequency offset (Hz).
    /// Negative for approaching satellites (the signal is blue-shifted, but
    /// the *offset from* the nominal carrier is negative in our convention).
    pub f_carr: f64,

    /// Effective C/A code rate (chips/s) = `CODE_FREQ_CA + f_carr / CARR_TO_CODE`.
    pub f_code: f64,

    /// Carrier phase accumulator in fractional cycles \[0, 1).
    /// Incremented by `f_carr × DT` each sample.
    pub carr_phase: f64,

    /// Code phase accumulator in chips \[0, 1023).
    /// Incremented by `f_code × DT` each sample.
    pub code_phase: f64,

    /// Index of the current navigation word in `dwrd` (0–59).
    pub iword: usize,
    /// Bit index within the current word (0–29).
    pub ibit: usize,
    /// C/A code epoch count within the current navigation bit (0–19).
    /// Each navigation bit spans 20 code periods (20 ms).
    pub icode: usize,

    /// Current navigation data bit, bipolar ±1.
    pub data_bit: i32,
    /// Current C/A chip value at `code_phase`, bipolar ±1.
    pub code_ca: i32,

    /// Decoded navigation words (6 subframes × 10 words = 60 words total).
    /// Consumed bit-by-bit during IQ generation:
    /// `bit = (dwrd[iword] >> (29 - ibit)) & 1`.
    pub dwrd: [u32; 60],

    /// Raw subframe words (53 rows × 10 words), produced by [`navmsg::eph_to_subframes`].
    pub sbf: [[u32; 10]; 53],

    /// Azimuth and elevation of the satellite seen from the receiver (radians).
    pub azel: [f64; 2],

    /// Which subframe 4/5 almanac page to broadcast next (0–24, cycled each 30 s).
    pub ipage: usize,
}

impl Channel {
    /// Construct a new channel for PRN `prn` at GPS time `grx`.
    ///
    /// Returns `None` if the satellite is below the horizon at `grx`.
    pub fn new(
        prn:      u8,
        eph:      &Ephemeris,
        iono:     &IonoUtc,
        grx:      GpsTime,
        rx_ecef:  [f64; 3],
    ) -> Option<Self> {
        // Check visibility and compute initial pseudorange.
        let rho = compute_range(eph, iono, grx, rx_ecef)?;

        let ca  = codegen::to_bipolar(&codegen::generate(prn));
        let sbf = navmsg::eph_to_subframes(eph, iono);

        let mut ch = Self {
            prn,
            ca,
            sbf,
            f_carr:     0.0,
            f_code:     CODE_FREQ_CA,
            carr_phase: 0.0,
            code_phase: 0.0,
            iword:      0,
            ibit:       0,
            icode:      0,
            data_bit:   1,
            code_ca:    1,
            dwrd:       [0u32; 60],
            azel:       rho.azel,
            ipage:      0,
        };

        // Initialise navigation message and code phase alignment.
        ch.dwrd = navmsg::generate_nav_msg(&ch.sbf, grx, ch.ipage);
        ch.init_code_phase(&rho, grx);

        Some(ch)
    }

    // ── Private helpers ───────────────────────────────────────────────────────

    /// Align code phase and navigation bit counter to the initial pseudorange.
    ///
    /// The pseudorange gives the signal travel time from satellite to receiver.
    /// We use this to figure out which nav bit and C/A chip are currently
    /// being transmitted, so the simulated signal is coherent from the first sample.
    fn init_code_phase(&mut self, rho: &RangeResult, grx: GpsTime) {
        // Time offset from the channel's reference GPS time to the current time,
        // minus the signal travel time, gives the elapsed time since transmission.
        let ms = (grx.sub(rho.g) + 6.0 - rho.range / SPEED_OF_LIGHT) * 1000.0;

        // Convert elapsed milliseconds to integer indices.
        let ims = ms as usize;

        // Sub-millisecond fractional code phase (chips).
        self.code_phase = (ms - ims as f64) * 1023.0;

        // Navigation word/bit/code indices derived from elapsed ms.
        // 1 word = 30 bits × 20 ms = 600 ms.
        self.iword  = (ims / 600).min(59);
        self.ibit   = ((ims % 600) / 20).min(29);
        self.icode  = (ims % 20).min(19);

        // Extract current data bit from the nav word ring.
        let word = self.dwrd.get(self.iword).copied().unwrap_or(0);
        self.data_bit = ((word >> (29 - self.ibit)) & 1) as i32 * 2 - 1;

        // Current C/A chip at the initial code phase.
        let chip_idx = self.code_phase as usize % 1023;
        self.code_ca = self.ca.get(chip_idx).copied().unwrap_or(0) as i32;
    }
}
