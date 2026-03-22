//! Per-satellite tracking state for one simulation channel.
//!
//! A [`Channel`] holds all the mutable state needed to advance one satellite's
//! signal contribution through the IQ accumulation loop:
//! - Carrier and code phase accumulators
//! - Navigation message word ring
//! - Spreading code sequence (bipolar ±1)
//! - Current data bit and code chip state
//!
//! Multi-constellation support: the spreading code and chip rate vary by
//! constellation.  GPS uses the 1023-chip C/A code at 1.023 Mcps; `BeiDou` uses
//! a 10230-chip B1C Weil code at 10.23 Mcps; Galileo uses a 4092-chip E1-B LFSR
//! code at 4.092 Mcps.  All three share the 1575.42 MHz L1/B1C/E1 carrier.

use super::codegen;
use super::navmsg;
use super::orbit::{RangeResult, compute_range};
use super::types::{
    Constellation, Ephemeris, GpsTime, IonoUtc,
    consts::{CODE_FREQ_CA, SPEED_OF_LIGHT},
};

// Chip rates for each constellation (chips/s).
const CHIP_RATE_BEIDOU: f64 = 10_230_000.0; // B1C: 10.23 Mcps
const CHIP_RATE_GALILEO: f64 = 4_092_000.0; // E1-B: 4.092 Mcps

/// Simulation state for one tracked GNSS satellite.
///
/// All phase accumulators are floating-point so that small fractional
/// increments (Doppler, sub-chip offsets) accumulate correctly over many
/// simulation steps without integer truncation error.
#[derive(Clone)]
pub struct Channel {
    /// Which constellation this channel belongs to.
    pub constellation: Constellation,

    /// PRN number (1–63 for `BeiDou`; 1–36 for Galileo; 1–32 for GPS).
    /// 0 indicates an inactive channel slot.
    pub prn: u8,

    /// Bipolar ±1 spreading code (1023 chips for GPS, 10230 for `BeiDou`, 4092 for Galileo).
    pub code: Vec<i8>,

    /// Length of the spreading code in chips (== `code.len()`).
    pub code_len: usize,

    /// Chip rate in chips/s (1.023 Mcps / 10.23 Mcps / 4.092 Mcps).
    pub chip_rate: f64,

    /// Carrier Doppler frequency offset (Hz).
    /// Negative for approaching satellites (the signal is blue-shifted, but
    /// the *offset from* the nominal carrier is negative in our convention).
    pub f_carr: f64,

    /// Effective spreading code rate (chips/s) = `chip_rate + f_carr / CARR_TO_CODE`.
    pub f_code: f64,

    /// Carrier phase accumulator in fractional cycles \[0, 1).
    /// Incremented by `f_carr × DT` each sample.
    pub carr_phase: f64,

    /// Code phase accumulator in chips \[0, `code_len`).
    /// Incremented by `f_code × DT` each sample.
    pub code_phase: f64,

    /// Index of the current navigation word in `dwrd` (0–59).
    pub iword: usize,
    /// Bit index within the current word (0–29).
    pub ibit: usize,
    /// Code epoch count within the current navigation bit (0–19).
    /// Each navigation bit spans 20 code periods (20 ms) for all three constellations.
    pub icode: usize,

    /// Current navigation data bit, bipolar ±1.
    pub data_bit: i32,
    /// Current spreading chip value at `code_phase`, bipolar ±1.
    pub code_ca: i32,

    /// Decoded navigation words (6 subframes × 10 words = 60 words total).
    /// Consumed bit-by-bit during IQ generation:
    /// `bit = (dwrd[iword] >> (29 - ibit)) & 1`.
    ///
    /// For `BeiDou` and Galileo, a GPS LNAV-style message is used as a
    /// placeholder for simulation purposes.
    pub dwrd: [u32; 60],

    /// Raw subframe words (53 rows × 10 words), produced by [`navmsg::eph_to_subframes`].
    pub sbf: [[u32; 10]; 53],

    /// Azimuth and elevation of the satellite seen from the receiver (radians).
    pub azel: [f64; 2],

    /// Which subframe 4/5 almanac page to broadcast next (0–24, cycled each 30 s).
    pub ipage: usize,
}

impl Channel {
    /// Construct a new channel for a satellite at GPS time `grx`.
    ///
    /// Returns `None` if the satellite is below the horizon at `grx`.
    pub fn new(
        constellation: Constellation,
        prn: u8,
        eph: &Ephemeris,
        iono: &IonoUtc,
        grx: GpsTime,
        rx_ecef: [f64; 3],
    ) -> Option<Self> {
        // Check visibility and compute initial pseudorange.
        let rho = compute_range(eph, iono, grx, rx_ecef)?;

        // Generate the spreading code and determine chip rate by constellation.
        let (code, code_len, chip_rate) = match constellation {
            Constellation::Gps => {
                let raw = codegen::generate(prn);
                let bipolar = codegen::to_bipolar(&raw);
                (bipolar.to_vec(), 1023usize, CODE_FREQ_CA)
            }
            Constellation::BeiDou => {
                let c = crate::gps_sim::beidou::generate_b1c_data(prn);
                let len = c.len();
                (c, len, CHIP_RATE_BEIDOU)
            }
            Constellation::Galileo => {
                let arr = crate::gps_sim::galileo::generate_e1b(prn);
                let c: Vec<i8> = arr.to_vec();
                let len = c.len();
                (c, len, CHIP_RATE_GALILEO)
            }
        };

        // Use GPS-format nav message for all constellations (simulation approximation).
        // For BeiDou and Galileo, the nav message format differs from GPS LNAV, but
        // a GPS-style placeholder works for receiver spoofing because receivers use
        // their own nav data databases for PVT.
        let sbf = navmsg::eph_to_subframes(eph, iono);

        let mut ch = Self {
            constellation,
            prn,
            code,
            code_len,
            chip_rate,
            f_carr: 0.0,
            f_code: chip_rate,
            carr_phase: 0.0,
            code_phase: 0.0,
            iword: 0,
            ibit: 0,
            icode: 0,
            data_bit: 1,
            code_ca: 1,
            dwrd: [0u32; 60],
            sbf,
            azel: rho.azel,
            ipage: 0,
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
    /// We use this to figure out which nav bit and code chip are currently
    /// being transmitted, so the simulated signal is coherent from the first sample.
    fn init_code_phase(&mut self, rho: &RangeResult, grx: GpsTime) {
        // Time offset from the channel's reference GPS time to the current time,
        // minus the signal travel time, gives the elapsed time since transmission.
        let ms = (grx.sub(rho.g) + 6.0 - rho.range / SPEED_OF_LIGHT) * 1000.0;

        // Convert elapsed milliseconds to integer indices.
        let ims = ms as usize;

        // Sub-millisecond fractional code phase (chips).
        // Scale by code_len since all three constellations have 1 ms code epochs.
        self.code_phase = (ms - ims as f64) * self.code_len as f64;

        // Navigation word/bit/code indices derived from elapsed ms.
        // 1 word = 30 bits × 20 ms = 600 ms (same timing for all constellations here).
        self.iword = (ims / 600).min(59);
        self.ibit = ((ims % 600) / 20).min(29);
        self.icode = (ims % 20).min(19);

        // Extract current data bit from the nav word ring.
        let word = self.dwrd.get(self.iword).copied().unwrap_or(0);
        self.data_bit = ((word >> (29 - self.ibit)) & 1) as i32 * 2 - 1;

        // Current chip at the initial code phase.
        let chip_idx = self.code_phase as usize % self.code_len;
        self.code_ca = self.code.get(chip_idx).copied().unwrap_or(0) as i32;
    }
}
