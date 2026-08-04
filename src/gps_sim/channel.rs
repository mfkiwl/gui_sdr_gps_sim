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
use super::navmsg::{self, WORDS_PER_FRAME};
use super::orbit::{RangeResult, compute_range};
use super::types::{
    Constellation, Ephemeris, GpsTime, IonoUtc,
    consts::{CARR_TO_CODE, LAMBDA_L1, SPEED_OF_LIGHT},
};

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

    /// Index of the current navigation word in `dwrd` (0–49).
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

    /// Navigation words of the frame currently being transmitted (30 s worth).
    /// Consumed bit-by-bit during IQ generation:
    /// `bit = (dwrd[iword] >> (29 - ibit)) & 1`.
    ///
    /// For `BeiDou` and Galileo, a GPS LNAV-style message is used as a
    /// placeholder for simulation purposes.
    pub dwrd: [u32; WORDS_PER_FRAME],

    /// GPS time at which the frame in `dwrd` starts — always 30-second aligned.
    pub g0: GpsTime,

    /// Last word of `dwrd`, carried into the next frame so parity chains unbroken.
    pub last_word: u32,

    /// Next frame, built one subframe ahead of the wrap so that the swap at the
    /// frame boundary is instant and no stale words are ever transmitted.
    pub dwrd_next: [u32; WORDS_PER_FRAME],
    /// Final word of `dwrd_next`; becomes `last_word` when the frames swap.
    pub last_word_next: u32,
    /// Start time of the prepared frame, or `None` if it is not built yet.
    pub g0_next: Option<GpsTime>,

    /// Raw subframe words (53 rows × 10 words), produced by [`navmsg::eph_to_subframes`].
    pub sbf: [[u32; 10]; 53],

    /// Azimuth and elevation of the satellite seen from the receiver (radians).
    pub azel: [f64; 2],

    /// Geometric range satellite → receiver (metres), updated every step.
    /// Used to compute the free-space path-loss gain factor.
    pub d: f64,

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

        // Generate the spreading code; the chip rate comes from the constellation.
        let (code, code_len) = match constellation {
            Constellation::Gps => {
                let raw = codegen::generate(prn);
                let bipolar = codegen::to_bipolar(&raw);
                (bipolar.to_vec(), 1023usize)
            }
            Constellation::BeiDou => {
                let c = crate::gps_sim::beidou::generate_b1c_data(prn);
                let len = c.len();
                (c, len)
            }
            Constellation::Galileo => {
                let arr = crate::gps_sim::galileo::generate_e1b(prn);
                let c: Vec<i8> = arr.to_vec();
                let len = c.len();
                (c, len)
            }
        };
        let chip_rate = constellation.chip_rate();

        // Use GPS-format nav message for all constellations (simulation approximation).
        // For BeiDou and Galileo, the nav message format differs from GPS LNAV, but
        // a GPS-style placeholder works for receiver spoofing because receivers use
        // their own nav data databases for PVT.
        let sbf = navmsg::eph_to_subframes(eph, iono);

        // Initialise Doppler from the pseudorange rate so the first 100 ms step
        // already has the correct frequency offset, not a silent zero-Doppler artifact.
        let f_carr = -rho.rate / LAMBDA_L1;
        let f_code = chip_rate + f_carr / CARR_TO_CODE;

        let mut ch = Self {
            constellation,
            prn,
            code,
            code_len,
            chip_rate,
            f_carr,
            f_code,
            carr_phase: 0.0,
            code_phase: 0.0,
            iword: 0,
            ibit: 0,
            icode: 0,
            data_bit: 1,
            code_ca: 1,
            dwrd: [0u32; WORDS_PER_FRAME],
            g0: navmsg::frame_start(grx),
            last_word: 0,
            dwrd_next: [0u32; WORDS_PER_FRAME],
            last_word_next: 0,
            g0_next: None,
            sbf,
            azel: rho.azel,
            d: rho.d,
            ipage: 0,
        };

        // Align the bit counters to the signal's transmit time first — that
        // decides which frame the channel starts in — then build that frame.
        ch.init_code_phase(&rho, grx);
        let (dwrd, last) = navmsg::generate_nav_msg(&ch.sbf, ch.g0, ch.ipage, 0);
        ch.dwrd = dwrd;
        ch.last_word = last;
        ch.refresh_data_bit();

        Some(ch)
    }

    /// Re-read the current navigation bit after `dwrd` or the counters change.
    pub fn refresh_data_bit(&mut self) {
        let word = self.dwrd.get(self.iword).copied().unwrap_or(0);
        self.data_bit = ((word >> (29 - self.ibit)) & 1) as i32 * 2 - 1;
    }

    /// Advance to the next navigation bit and update [`Self::data_bit`].
    ///
    /// Called once per 20 code periods (one 50 bps bit).  On crossing the end of
    /// the 30-second frame it swaps in the frame prepared by
    /// [`Self::prepare_next_frame`], so the bit stream continues seamlessly and
    /// the parity chain carries over.
    pub fn advance_nav_bit(&mut self) {
        self.ibit += 1;
        if self.ibit >= 30 {
            // Start of a new navigation word.
            self.ibit = 0;
            self.iword += 1;
            if self.iword >= WORDS_PER_FRAME {
                self.iword = 0;
                if let Some(g) = self.g0_next.take() {
                    self.dwrd = self.dwrd_next;
                    self.last_word = self.last_word_next;
                    self.g0 = g;
                    self.ipage = (self.ipage + 1) % 25;
                }
            }
        }
        self.refresh_data_bit();
    }

    /// Build the next 30-second frame once the channel enters its final subframe.
    ///
    /// Doing this a subframe early means the swap in [`Self::advance_nav_bit`] is
    /// a plain copy, so no stale word is ever transmitted at the boundary.  It is
    /// idempotent: repeated calls within the same frame do nothing.
    ///
    /// Driving this from the channel's own word counter — rather than from a
    /// wall-clock step index — keeps each satellite's frames aligned with its own
    /// Doppler-shifted bit stream.
    pub fn prepare_next_frame(&mut self) {
        if self.g0_next.is_some() || self.iword < WORDS_PER_FRAME - 10 {
            return;
        }
        let g_next = self.g0.add_secs(navmsg::FRAME_SECS);
        let page_next = (self.ipage + 1) % 25;
        let (words, last) = navmsg::generate_nav_msg(&self.sbf, g_next, page_next, self.last_word);
        self.dwrd_next = words;
        self.last_word_next = last;
        self.g0_next = Some(g_next);
    }

    // ── Private helpers ───────────────────────────────────────────────────────

    /// Align code phase and navigation bit counters to the signal's transmit time.
    ///
    /// The bit arriving at the receiver at `grx` left the satellite `range / c`
    /// earlier, so the nav-message position is fixed by the **transmit** time, not
    /// by `grx`.  Everything is measured from the start of the 30-second frame
    /// containing that transmit time, which is exactly the reference the receiver
    /// reconstructs from the TOW in the HOW word.
    ///
    /// Getting this offset wrong does not stop a receiver acquiring or tracking —
    /// it only corrupts the time solution, which is why it survives every
    /// signal-level check and still prevents a fix.
    fn init_code_phase(&mut self, rho: &RangeResult, grx: GpsTime) {
        // Transmit time of the signal now arriving at the receiver.
        let t_tx = grx.sec - rho.range / SPEED_OF_LIGHT;

        // Start of the frame containing that transmit time.
        self.g0 = navmsg::frame_start(GpsTime {
            week: grx.week,
            sec: t_tx,
        });

        // Elapsed time into the frame — always within [0, 30 000) ms.
        let ms = (t_tx - self.g0.sec) * 1000.0;
        let ims = ms as usize;

        // Sub-millisecond fractional code phase (chips).
        // Scale by code_len since all three constellations have 1 ms code epochs.
        self.code_phase = (ms - ims as f64) * self.code_len as f64;

        // Navigation word/bit/code indices derived from elapsed ms.
        // 1 word = 30 bits × 20 ms = 600 ms (same timing for all constellations here).
        self.iword = (ims / 600).min(WORDS_PER_FRAME - 1);
        self.ibit = ((ims % 600) / 20).min(29);
        self.icode = (ims % 20).min(19);

        // Current chip at the initial code phase.
        let chip_idx = self.code_phase as usize % self.code_len;
        self.code_ca = self.code.get(chip_idx).copied().unwrap_or(0) as i32;
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[expect(
    clippy::indexing_slicing,
    reason = "test code indexes fixed-size nav arrays and bit slices with loop-bounded indices"
)]
#[cfg(test)]
mod tests {
    use super::{CARR_TO_CODE, Channel, LAMBDA_L1};
    use crate::gps_sim::coords::llh_to_ecef;
    use crate::gps_sim::orbit::compute_range;
    use crate::gps_sim::types::{Constellation, Ephemeris, GpsTime, IonoUtc, Location};

    const WEEK: i32 = 2300;

    /// Ephemeris for a satellite parked more or less overhead of `rx_llh()`, so
    /// `Channel::new` sees it above the horizon.
    fn overhead_eph() -> Ephemeris {
        let a = 26_560_000.0_f64;
        Ephemeris {
            valid: true,
            sqrta: a.sqrt(),
            ecc: 0.0,
            inc0: 0.95,
            m0: 0.9,
            omg0: 0.1,
            aop: 0.0,
            omgdot: -8.0e-9,
            idot: 0.0,
            deltan: 0.0,
            toe: GpsTime {
                week: WEEK,
                sec: 0.0,
            },
            toc: GpsTime {
                week: WEEK,
                sec: 0.0,
            },
            ..Default::default()
        }
    }

    /// Amsterdam, roughly.
    fn rx_ecef() -> [f64; 3] {
        llh_to_ecef(Location::degrees(52.3791, 4.9003, 5.0))
    }

    /// Search for a GPS time at which the test satellite is visible.
    fn visible_epoch(eph: &Ephemeris, iono: &IonoUtc) -> GpsTime {
        let rx = rx_ecef();
        (0..24 * 60)
            .map(|m| GpsTime {
                week: WEEK,
                sec: f64::from(m) * 60.0,
            })
            .find(|&g| compute_range(eph, iono, g, rx).is_some())
            .expect("test satellite should be visible at some point in 24 h")
    }

    /// Regression guard for the `f_carr = 0.0` bug: a new channel must start with
    /// the Doppler implied by the pseudorange rate, not with zero.  A zero here
    /// gives every satellite a silent zero-Doppler first 100 ms step.
    #[test]
    fn doppler_initialised_from_range_rate() {
        let eph = overhead_eph();
        let iono = IonoUtc::default();
        let g = visible_epoch(&eph, &iono);
        let rx = rx_ecef();
        let rho = compute_range(&eph, &iono, g, rx).expect("visible at chosen epoch");

        let ch = Channel::new(Constellation::Gps, 1, &eph, &iono, g, rx)
            .expect("channel should be created for a visible satellite");

        let expected = -rho.rate / LAMBDA_L1;
        assert!(
            (ch.f_carr - expected).abs() < 1e-6,
            "f_carr was {}, expected {expected} (derived from rho.rate)",
            ch.f_carr,
        );
        assert!(
            ch.f_carr.abs() > 1.0,
            "a moving GPS satellite must have non-zero Doppler; got {}",
            ch.f_carr,
        );
    }

    /// The code NCO must start at `chip_rate + f_carr / CARR_TO_CODE` — dividing
    /// by 1540, not multiplying.
    #[test]
    fn code_rate_follows_carrier_doppler() {
        let eph = overhead_eph();
        let iono = IonoUtc::default();
        let g = visible_epoch(&eph, &iono);

        let ch = Channel::new(Constellation::Gps, 1, &eph, &iono, g, rx_ecef())
            .expect("channel should be created for a visible satellite");

        let expected = ch.chip_rate + ch.f_carr / CARR_TO_CODE;
        assert!((ch.f_code - expected).abs() < 1e-9);
        // Code Doppler is ~1540× smaller than carrier Doppler, so f_code stays
        // within a few Hz of the nominal chip rate.
        assert!((ch.f_code - ch.chip_rate).abs() < 10.0);
    }

    // ── Navigation bit stream (receiver-side decoding) ────────────────────────
    //
    // The signal-chain tests prove the RF is well formed.  A receiver will
    // acquire and track such a signal and still never report a position,
    // because everything that decides *position* lives in the 50 bps data layer
    // on top.  These tests play receiver on that layer: pull the bit stream out
    // exactly as the IQ generator does, hunt the preamble, check parity on every
    // word, and confirm the decoded TOW matches where the bits actually sit in
    // time.

    use crate::gps_sim::navmsg::{PARITY_MASKS, SUBFRAME_SECS, WORDS_PER_FRAME};
    use crate::gps_sim::types::consts::SPEED_OF_LIGHT;

    /// Pack 30 transmitted bits (MSB first) into the encoder's word layout:
    /// data in bits 29–6, parity in 5–0.
    fn pack_word(bits: &[u32]) -> u32 {
        bits.iter().fold(0u32, |acc, &b| (acc << 1) | b)
    }

    /// A receiver's parity check on a word, given the previously received word.
    fn parity_ok(word: u32, prev: u32) -> bool {
        let received = word & 0x3FFF_FFC0;
        let d29_star = (prev >> 1) & 1;
        let d30_star = prev & 1;

        // Undo the D30* data complement before recomputing.
        let d = if d30_star == 1 {
            received ^ 0x3FFF_FFC0
        } else {
            received
        };

        let carry_is_d29 = [true, false, true, false, false, true];
        let mut expected = 0u32;
        for (i, (&mask, &from_d29)) in PARITY_MASKS.iter().zip(carry_is_d29.iter()).enumerate() {
            let carry = if from_d29 { d29_star } else { d30_star };
            expected |= ((carry + (mask & d).count_ones()) % 2) << (5 - i as u32);
        }
        expected == (word & 0x3F)
    }

    /// Recover the 24 data bits of a word, undoing the D30* complement.
    fn word_data(word: u32, prev: u32) -> u32 {
        let data = (word >> 6) & 0x00FF_FFFF;
        if prev & 1 == 1 {
            !data & 0x00FF_FFFF
        } else {
            data
        }
    }

    /// [`overhead_eph`] with every clock and harmonic term populated.
    ///
    /// The orbit is unchanged, so the satellite is still visible, but no nav
    /// word encodes to zero by accident — which would otherwise mask a word the
    /// encoder failed to write.
    fn overhead_eph_populated() -> Ephemeris {
        let mut eph = overhead_eph();
        eph.ecc = 0.004_312;
        eph.deltan = 4.7e-9;
        eph.idot = 2.4e-10;
        eph.crs = -17.5;
        eph.crc = 233.7;
        eph.cuc = -1.02e-6;
        eph.cus = 8.11e-6;
        eph.cic = 1.86e-8;
        eph.cis = -9.31e-8;
        eph.af0 = -1.23e-4;
        eph.af1 = -9.09e-12;
        eph.tgd = -1.02e-8;
        eph.iode = 61;
        eph.iodc = 61;
        eph.sva = 2;
        eph
    }

    /// Duration of one 50 bps navigation bit.
    const BIT_SECS: f64 = 0.02;

    /// Collect `n_bits` navigation bits from a real channel, driving the same
    /// `prepare_next_frame` / `advance_nav_bit` pair the IQ generator uses.
    ///
    /// Returns the bits together with the **transmit time at which the first
    /// captured bit begins**, derived from `grx` and the pseudorange alone.
    ///
    /// That ground truth is deliberately computed without consulting the
    /// channel's own `g0` or word counters. A test that reads its expectation
    /// out of the same state it is checking will happily agree with a timeline
    /// that is uniformly shifted -- which is precisely the failure mode being
    /// guarded against here.
    fn capture_bits(n_bits: usize) -> (Vec<u32>, f64) {
        let iono = IonoUtc::default();
        let eph = overhead_eph_populated();
        let grx = visible_epoch(&eph, &iono);
        let rx = rx_ecef();

        let rho = compute_range(&eph, &iono, grx, rx).expect("visible at chosen epoch");
        // The bit arriving now left the satellite range/c ago...
        let t_tx = grx.sec - rho.range / SPEED_OF_LIGHT;
        // ...and nav bits sit on a 20 ms grid anchored to the GPS week epoch.
        let bit0_start = (t_tx / BIT_SECS).floor() * BIT_SECS;

        let mut ch = Channel::new(Constellation::Gps, 1, &eph, &iono, grx, rx)
            .expect("channel should be created for a visible satellite");

        let mut bits = Vec::with_capacity(n_bits);
        bits.push(u32::from(ch.data_bit > 0));
        for _ in 1..n_bits {
            ch.prepare_next_frame();
            ch.advance_nav_bit();
            bits.push(u32::from(ch.data_bit > 0));
        }

        (bits, bit0_start)
    }

    /// Bit index of the first subframe boundary at or after bit 0, computed
    /// purely from transmit time.
    fn first_subframe_index(bit0_start: f64) -> usize {
        let first_sf_time = (bit0_start / SUBFRAME_SECS).ceil() * SUBFRAME_SECS;
        ((first_sf_time - bit0_start) / BIT_SECS).round() as usize
    }

    /// Every word a receiver pulls off the wire must pass parity.
    ///
    /// Spanning three frames covers two frame boundaries, so this also proves
    /// the parity chain survives the 30-second buffer swap. Before the carry
    /// bits were folded into the parity, roughly three words in four failed
    /// here -- enough that no subframe ever validated and no receiver could
    /// decode ephemeris, however clean the RF looked.
    #[test]
    fn transmitted_words_pass_receiver_parity() {
        let n_words = WORDS_PER_FRAME * 3 + 20;
        let (bits, bit0_start) = capture_bits(n_words * 30);
        let skip = first_subframe_index(bit0_start);

        let mut prev = 0u32;
        let mut checked = 0usize;
        let mut failures = 0usize;
        let mut first_failure = None;

        let mut i = skip;
        while i + 30 <= bits.len() {
            let word = pack_word(&bits[i..i + 30]);
            // The first word has no predecessor inside the capture.
            if checked > 0 && !parity_ok(word, prev) {
                failures += 1;
                first_failure.get_or_insert(checked);
            }
            prev = word;
            checked += 1;
            i += 30;
        }

        assert!(checked > 140, "expected >3 frames of words, got {checked}");
        assert_eq!(
            failures, 0,
            "{failures} of {checked} words failed receiver parity \
             (first at word {first_failure:?})",
        );
    }

    /// No word may be blank: a run of zero bits breaks subframe sync.
    ///
    /// The frame buffer used to be ten words longer than one frame, so every
    /// wrap transmitted six seconds of zeros.
    #[test]
    fn no_blank_words_in_the_stream() {
        let (bits, bit0_start) = capture_bits(WORDS_PER_FRAME * 3 * 30);
        let skip = first_subframe_index(bit0_start);

        let mut blank = 0usize;
        let mut i = skip;
        while i + 30 <= bits.len() {
            if bits[i..i + 30].iter().all(|&b| b == 0) {
                blank += 1;
            }
            i += 30;
        }
        assert_eq!(blank, 0, "{blank} all-zero words in the transmitted stream");
    }

    /// A subframe must begin exactly on every 6-second GPS epoch, carrying the
    /// TLM preamble.
    ///
    /// Anchored to transmit time, so a bit stream that is internally tidy but
    /// sits at the wrong absolute offset fails here.
    #[test]
    fn subframes_start_on_six_second_epochs() {
        let (bits, bit0_start) = capture_bits(WORDS_PER_FRAME * 2 * 30);
        let skip = first_subframe_index(bit0_start);

        let mut found = 0usize;
        let mut idx = skip;
        while idx + 8 <= bits.len() {
            let byte = bits[idx..idx + 8].iter().fold(0u32, |a, &b| (a << 1) | b);
            assert!(
                byte == 0x8B || byte == (!0x8Bu32 & 0xFF),
                "no TLM preamble at the 6 s epoch t={:.2}s (bit {idx}); got 0x{byte:02X}",
                bit0_start + idx as f64 * BIT_SECS,
            );
            found += 1;
            idx += 300; // one subframe
        }
        assert!(found >= 8, "expected at least 8 subframes, found {found}");
    }

    /// The decoded TOW must agree with where the subframe actually sits in GPS
    /// time.
    ///
    /// This is the check neither a spectrum plot nor an acquisition search can
    /// make, and the one that decides whether a receiver's position solution
    /// closes: a TOW off by one subframe puts every satellite 6 seconds -- some
    /// 23 km of orbit -- from where the receiver places it, so the residuals
    /// never converge and no fix is reported.
    #[test]
    fn decoded_tow_matches_transmit_time() {
        let (bits, bit0_start) = capture_bits((WORDS_PER_FRAME * 2 + 20) * 30);
        let skip = first_subframe_index(bit0_start);

        let mut checks = 0usize;
        let mut idx = skip;

        while idx + 60 <= bits.len() {
            let tlm = pack_word(&bits[idx..idx + 30]);
            let how = pack_word(&bits[idx + 30..idx + 60]);
            let tow = word_data(how, tlm) >> 7;

            // Ground truth: where this subframe actually starts, in GPS seconds.
            let subframe_start = bit0_start + idx as f64 * BIT_SECS;
            // IS-GPS-200 20.3.3.2: the HOW names the *next* subframe boundary.
            let expected = (subframe_start / SUBFRAME_SECS).round() as u32 + 1;

            assert_eq!(
                tow,
                expected,
                "subframe transmitted at t={subframe_start:.2}s carries TOW={tow}, \
                 expected {expected} -- a receiver would place this subframe {}s from \
                 where it really is",
                (i64::from(tow) - i64::from(expected)) * 6,
            );

            checks += 1;
            idx += 300;
        }

        assert!(
            checks >= 8,
            "expected to verify TOW on at least 8 subframes, verified {checks}",
        );
    }

    /// Each constellation must get its own chip rate and code length wired in.
    #[test]
    fn chip_rate_and_code_length_per_constellation() {
        let iono = IonoUtc::default();
        let rx = rx_ecef();

        for (constellation, expected_len) in [
            (Constellation::Gps, 1023usize),
            (Constellation::Galileo, 4092),
            (Constellation::BeiDou, 10230),
        ] {
            let mut eph = overhead_eph();
            eph.constellation = constellation;
            let g = visible_epoch(&eph, &iono);
            let ch = Channel::new(constellation, 1, &eph, &iono, g, rx)
                .expect("channel should be created for a visible satellite");

            assert_eq!(
                ch.chip_rate,
                constellation.chip_rate(),
                "{constellation:?} chip rate",
            );
            assert_eq!(ch.code_len, expected_len, "{constellation:?} code length");
            assert_eq!(ch.code.len(), expected_len, "{constellation:?} code buffer");
            // Codes are bipolar ±1 — a 0 here means an ungenerated chip.
            assert!(
                ch.code.iter().all(|&c| c == 1 || c == -1),
                "{constellation:?} code must be bipolar",
            );
        }
    }
}
