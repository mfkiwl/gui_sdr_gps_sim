//! GPS navigation message builder.
//!
//! # Message structure (IS-GPS-200 §20.3)
//!
//! ```text
//! Superframe = 25 frames = 12.5 min
//!   Frame    = 5 subframes = 30 s
//!     Subframe = 10 words = 6 s
//!       Word   = 30 bits = 24 data + 6 parity = 20 ms
//! ```
//!
//! ## Subframe contents
//! | SF | Content |
//! |----|---------|
//! | 1  | Clock: WN, IODC, SVH, URA, TGD, TOC, af0/af1/af2 |
//! | 2  | Ephemeris 1: IODE, Crs, Δn, M0, Cuc, e, Cus, √A, TOE |
//! | 3  | Ephemeris 2: Cic, Ω0, Cis, i0, Crc, ω, Ω̇, IDOT, IODE |
//! | 4  | Almanac SVs 25–32, iono/UTC (page 18), health (page 25) |
//! | 5  | Almanac SVs 1–24, health+TOA (page 25) |
//!
//! ## Word format (stored in a `u32`)
//! ```text
//! bits 31–30: D29*, D30* (parity carry-in from previous word)
//! bits 29– 6: 24 data bits (MSB = bit 29)
//! bits  5– 0: 6 parity bits D25–D30
//! ```
//!
//! ## Internal `sbf` array layout (53 rows × 10 words)
//! - Row 0 → Subframe 1 (ephemeris + clock)
//! - Row 1 → Subframe 2
//! - Row 2 → Subframe 3
//! - Rows 3 + 2·p, 4 + 2·p (p = 0..25): SF4 page p+1, SF5 page p+1

use super::types::{Ephemeris, GpsTime, IonoUtc, consts::GPS_PI};

// ── Constants ─────────────────────────────────────────────────────────────────

/// TLM word preamble, pre-shifted for direct OR into word position.
/// Preamble = 0x8B (10001011) occupies bits 29–22.
pub const PREAMBLE: u32 = 0x8B0000 << 6;

/// Alternating-bit pattern used for empty/unused almanac pages
/// (IS-GPS-200 §20.3.3.5.1).
pub const EMPTY_WORD: u32 = 0xAAAA_AAAA;

/// IS-GPS-200 Table 20-XIV parity masks (D25–D30).
///
/// Each mask selects which data bits contribute to one parity bit via
/// even-parity XOR.
pub(crate) const PARITY_MASKS: [u32; 6] = [
    0x3B1F_3480, // D25
    0x1D8F_9A40, // D26
    0x2EC7_CD00, // D27
    0x1763_E680, // D28
    0x2BB1_F340, // D29
    0x0B7A_89C0, // D30
];

// ── Parity ────────────────────────────────────────────────────────────────────

/// Apply IS-GPS-200 Table 20-XIV parity to a 30-bit navigation word.
///
/// `source` must have D29\* in bit 31 and D30\* in bit 30 — the two parity bits
/// of the previously transmitted word — and the 24 data bits in 29–6.
///
/// Two rules from the standard, both of which a receiver checks on every word:
///
/// 1. If D30\* is set, the 24 data bits are transmitted complemented (§20.3.5).
/// 2. **Each parity bit XORs in one of the carry bits**: D25, D27 and D30 chain
///    from D29\*, while D26, D28 and D29 chain from D30\*. Parity is computed
///    over the *uncomplemented* data.
///
/// Omitting rule 2 leaves the parity correct only when both carry bits happen to
/// be zero — about one word in four — which no receiver will accept.
///
/// `nib` selects the handling for words 2 and 10 of every subframe, whose bits
/// 23 and 24 are non-information-bearing: they are solved so that the resulting
/// D29 and D30 are zero, as the standard requires.
///
/// # Returns
/// The 30-bit word with 6 parity bits in bits 5–0.
pub fn compute_checksum(source: u32, nib: bool) -> u32 {
    // Data bits only — the carry bits must not leak into the parity sums.
    let mut d = source & 0x3FFF_FFC0;
    let d29_star = (source >> 31) & 1;
    let d30_star = (source >> 30) & 1;

    if nib {
        // Solve bits 23 and 24 so that D29 and D30 come out zero.
        if (d30_star + (PARITY_MASKS[4] & d).count_ones()) % 2 != 0 {
            d ^= 1 << 6;
        }
        if (d29_star + (PARITY_MASKS[5] & d).count_ones()) % 2 != 0 {
            d ^= 1 << 7;
        }
    }

    // Transmitted data bits: complemented when D30* is set.
    let mut word = if d30_star == 1 { d ^ 0x3FFF_FFC0 } else { d };

    // Which carry bit feeds each of D25..D30.
    const CARRY_IS_D29: [bool; 6] = [true, false, true, false, false, true];

    for (i, (&mask, &from_d29)) in PARITY_MASKS.iter().zip(CARRY_IS_D29.iter()).enumerate() {
        let carry = if from_d29 { d29_star } else { d30_star };
        word |= ((carry + (mask & d).count_ones()) % 2) << (5 - i as u32);
    }

    word
}

// ── Ephemeris → subframe words ────────────────────────────────────────────────

/// Encode satellite ephemeris into a 53×10 array of raw navigation words.
///
/// Rows 0–2 contain subframes 1–3 (clock and ephemeris data).
/// Rows 3 + 2·p, 4 + 2·p (p = 0..25) contain subframe 4/5 pages, which are
/// filled with [`EMPTY_WORD`] bit patterns (alternating 1s and 0s) except for
/// the ionospheric/UTC page (page 18 of subframe 4, row 37).
///
/// All floating-point fields are scaled to integer representations per
/// IS-GPS-200 Table 20-III before packing into bit fields.
///
/// Parity bits and TOW values are **not** set here; they are injected per
/// simulation step by [`generate_nav_msg`].
#[expect(
    clippy::indexing_slicing,
    reason = "sbf rows/words indexed with literals (0..52, 0..9) and loop-bounded indices all within the [[u32;10];53] bounds; alpha/beta arrays are [f64;4]"
)]
pub fn eph_to_subframes(eph: &Ephemeris, iono: &IonoUtc) -> [[u32; 10]; 53] {
    let mut sbf = [[0u32; 10]; 53];
    let data_id: u32 = 1; // always 1 for GPS

    // Helper: scale a float, round to integer, mask to `bits` bits, shift left.
    let pack = |val: f64, scale: f64, bits: u32, sh: u32| -> u32 {
        let mask = (1u32 << bits) - 1;
        let int_val = (val / scale).round() as i64 as u32;
        (int_val & mask) << sh
    };

    // ── Subframe 1: Clock data ────────────────────────────────────────────────
    sbf[0][0] = PREAMBLE;
    sbf[0][1] = 0x1u32 << 8; // HOW: subframe ID = 1 (TOW filled by generate_nav_msg)
    sbf[0][2] = ((eph.toe.week as u32 & 0x3FF) << 20) // GPS week number (10 bits)
              | (2u32 << 12)                           // L2 code flag
              | pack(eph.sva as f64, 1.0, 4, 8)        // URA index
              | pack(eph.svh as f64, 1.0, 6, 2); // SV health
    sbf[0][3] = pack(eph.iodc as f64, 1.0, 2, 22)     // IODC MSBs
              | pack(eph.tgd, f64::powi(2.0, -31), 8, 6); // group delay (s)
    sbf[0][4] = pack(eph.iodc as f64, 1.0, 8, 22)     // IODC LSBs
              | pack(eph.toc.sec, 16.0, 16, 6); // clock reference time
    sbf[0][5] = pack(eph.af2, f64::powi(2.0, -55), 8, 22)  // clock drift rate
              | pack(eph.af1, f64::powi(2.0, -43), 16, 6); // clock drift
    sbf[0][6] = pack(eph.af0, f64::powi(2.0, -31), 22, 8); // clock bias
    // Words 7–9: unused — fill with EMPTY_WORD pattern (24 data bits each).
    for w in 7..=9 {
        sbf[0][w] = (EMPTY_WORD & 0x00FF_FFFF) << 6;
    }

    // ── Subframe 2: Ephemeris 1 ───────────────────────────────────────────────
    sbf[1][0] = PREAMBLE;
    sbf[1][1] = 0x2u32 << 8;
    sbf[1][2] = pack(eph.iode as f64, 1.0, 8, 22)           // IODE
              | pack(eph.crs, f64::powi(2.0, -5), 16, 6); // Crs (m)
    sbf[1][3] = pack(eph.deltan / GPS_PI, f64::powi(2.0, -43), 16, 14) // Δn (rad/s)
              | pack(eph.m0 / GPS_PI, f64::powi(2.0, -31), 8, 6); // M0 MSBs
    sbf[1][4] = pack(eph.m0 / GPS_PI, f64::powi(2.0, -31), 24, 6); // M0 LSBs
    sbf[1][5] = pack(eph.cuc, f64::powi(2.0, -29), 16, 14) // Cuc (rad)
              | pack(eph.ecc, f64::powi(2.0, -33), 8, 6); // eccentricity MSBs
    sbf[1][6] = pack(eph.ecc, f64::powi(2.0, -33), 24, 6); // eccentricity LSBs
    sbf[1][7] = pack(eph.cus, f64::powi(2.0, -29), 16, 14) // Cus (rad)
              | pack(eph.sqrta, f64::powi(2.0, -19), 8, 6); // √A MSBs
    sbf[1][8] = pack(eph.sqrta, f64::powi(2.0, -19), 24, 6); // √A LSBs
    sbf[1][9] = pack(eph.toe.sec, 16.0, 16, 14)              // TOE
              | pack(eph.iodc as f64, 1.0, 5, 8); // FIT/IODC LSB

    // ── Subframe 3: Ephemeris 2 ───────────────────────────────────────────────
    sbf[2][0] = PREAMBLE;
    sbf[2][1] = 0x3u32 << 8;
    sbf[2][2] = pack(eph.cic, f64::powi(2.0, -29), 16, 14)     // Cic (rad)
              | pack(eph.omg0 / GPS_PI, f64::powi(2.0, -31), 8, 6); // Ω0 MSBs
    sbf[2][3] = pack(eph.omg0 / GPS_PI, f64::powi(2.0, -31), 24, 6); // Ω0 LSBs
    sbf[2][4] = pack(eph.cis, f64::powi(2.0, -29), 16, 14)     // Cis (rad)
              | pack(eph.inc0 / GPS_PI, f64::powi(2.0, -31), 8, 6); // i0 MSBs
    sbf[2][5] = pack(eph.inc0 / GPS_PI, f64::powi(2.0, -31), 24, 6); // i0 LSBs
    sbf[2][6] = pack(eph.crc, f64::powi(2.0, -5), 16, 14)      // Crc (m)
              | pack(eph.aop / GPS_PI, f64::powi(2.0, -31), 8, 6); // ω MSBs
    sbf[2][7] = pack(eph.aop / GPS_PI, f64::powi(2.0, -31), 24, 6); // ω LSBs
    sbf[2][8] = pack(eph.omgdot / GPS_PI, f64::powi(2.0, -43), 24, 6); // Ω̇ (rad/s)
    sbf[2][9] = pack(eph.iode as f64, 1.0, 8, 22)               // IODE
              | pack(eph.idot / GPS_PI, f64::powi(2.0, -43), 14, 8); // IDOT (rad/s)

    // ── Subframes 4 & 5: empty almanac pages ─────────────────────────────────
    // Each page pair occupies rows 3+2p (SF4) and 4+2p (SF5), p = 0..25.
    for p in 0..25usize {
        for (row, sf_id) in [(3 + 2 * p, 4u32), (4 + 2 * p, 5u32)] {
            sbf[row][0] = PREAMBLE;
            sbf[row][1] = sf_id << 8; // subframe ID in HOW
            // Word 2: dataId(2b) + svId(6b) + 16b EMPTY
            sbf[row][2] = (data_id << 28) | ((EMPTY_WORD & 0xFFFF) << 6);
            // Words 3–8: 24 data bits of EMPTY pattern each
            for w in 3..=8 {
                sbf[row][w] = (EMPTY_WORD & 0x00FF_FFFF) << 6;
            }
            // Word 9: 22-bit EMPTY + 2 reserved bits
            sbf[row][9] = (EMPTY_WORD & 0x003F_FFFF) << 8;
        }
    }

    // ── Subframe 4, page 18: ionosphere/UTC parameters ───────────────────────
    // Row index = 3 + 2*17 = 37.
    if iono.valid {
        let row = 37usize;
        sbf[row][2] = (data_id << 28) | (18u32 << 22); // data ID + page ID
        // α₀–α₃: words 3–4 (IS-GPS-200 Table 20-X).
        // Scale: α₀ = 2^-30, α₁ = 2^-27, α₂ = 2^-24, α₃ = 2^-24 (s/semi-circle^n).
        let scales_a = [
            f64::powi(2.0, -30),
            f64::powi(2.0, -27),
            f64::powi(2.0, -24),
            f64::powi(2.0, -24),
        ];
        let scales_b = [
            f64::powi(2.0, 11),
            f64::powi(2.0, 14),
            f64::powi(2.0, 16),
            f64::powi(2.0, 16),
        ];
        sbf[row][3] = pack(iono.alpha[0], scales_a[0], 8, 22)
            | pack(iono.alpha[1], scales_a[1], 8, 14)
            | pack(iono.alpha[2], scales_a[2], 8, 6);
        sbf[row][4] = pack(iono.alpha[3], scales_a[3], 8, 22)
            | pack(iono.beta[0], scales_b[0], 8, 14)
            | pack(iono.beta[1], scales_b[1], 8, 6);
        sbf[row][5] =
            pack(iono.beta[2], scales_b[2], 8, 22) | pack(iono.beta[3], scales_b[3], 8, 14);
        // A0, A1, tot, wnt — UTC parameters (IS-GPS-200 Table 20-IX).
        sbf[row][6] = pack(iono.a0, f64::powi(2.0, -30), 24, 6);
        sbf[row][7] = pack(iono.a1, f64::powi(2.0, -50), 24, 6);
        sbf[row][8] = ((iono.tot as u32 & 0xFF) << 22)
            | ((iono.wnt as u32 & 0xFF) << 14)
            | ((iono.dtls as u32 & 0xFF) << 6);
    }

    sbf
}

// ── Real-time navigation message injection ────────────────────────────────────

// ── Frame geometry ────────────────────────────────────────────────────────────

/// Subframes in one navigation frame (IS-GPS-200 §20.3.2).
pub const SUBFRAMES_PER_FRAME: usize = 5;

/// Words in one frame — 5 subframes × 10 words.
///
/// At 50 bps this is exactly 30 s: 50 words × 30 bits × 20 ms. The frame buffer
/// is sized to that so the word counter wraps precisely on the 30-second GPS
/// frame boundary, with no dead words to transmit.
pub const WORDS_PER_FRAME: usize = SUBFRAMES_PER_FRAME * 10;

/// Duration of one navigation frame in seconds.
pub const FRAME_SECS: f64 = 30.0;

/// Duration of one subframe in seconds.
pub const SUBFRAME_SECS: f64 = 6.0;

/// Round `t` down to the start of the navigation frame containing it.
///
/// Frames are aligned to 30-second GPS epochs; a receiver assumes every subframe
/// begins on a 6-second boundary and derives transmit time from that assumption,
/// so the generated bit stream must honour it.
pub fn frame_start(t: GpsTime) -> GpsTime {
    GpsTime {
        week: t.week,
        // The epsilon absorbs the float drift from repeated 0.1 s accumulation.
        sec: ((t.sec + 1e-6) / FRAME_SECS).floor() * FRAME_SECS,
    }
}

// ── Real-time navigation message injection ────────────────────────────────────

/// Build one 30-second navigation frame: 5 subframes, 50 words.
///
/// # Parameters
/// - `sbf`:       Raw 53×10 subframe array from [`eph_to_subframes`].
/// - `g0`:        Frame start time. **Must be 30-second aligned** — pass
///   [`frame_start`]. The TOW written into each HOW is derived from it, so a
///   misaligned `g0` makes the receiver place every subframe at the wrong
///   instant and biases its time solution.
/// - `ipage`:     Which subframe 4/5 almanac page to broadcast (0–24).
/// - `prev_word`: Last word of the *previous* frame, so that parity chains
///   unbroken across the frame boundary. Pass 0 for the very first frame.
///
/// # Returns
/// The 50 frame words plus the final word, to be fed back as `prev_word` next
/// time. Bits are extracted as `(dwrd[iword] >> (29 - ibit)) & 1`.
#[expect(
    clippy::indexing_slicing,
    reason = "sbf[row][w]: row is in rows[] (max 4+2*24=52<53), w<10; dwrd[base+w]: base+w<50"
)]
pub fn generate_nav_msg(
    sbf: &[[u32; 10]; 53],
    g0: GpsTime,
    ipage: usize,
    prev_word: u32,
) -> ([u32; WORDS_PER_FRAME], u32) {
    let mut dwrd = [0u32; WORDS_PER_FRAME];

    // Subframes 1–3 are in rows 0–2.
    // Subframe 4 page = row 3 + 2*ipage; Subframe 5 page = row 4 + 2*ipage.
    let rows = [0usize, 1, 2, 3 + 2 * ipage, 4 + 2 * ipage];

    // TOW count of the frame start, in 6-second units.
    let tow_base = (g0.sec / SUBFRAME_SECS) as u32;

    let mut prev = prev_word;

    for (sf_idx, &row) in rows.iter().enumerate() {
        let base = sf_idx * 10;

        // IS-GPS-200 §20.3.3.2: the HOW carries the TOW count of the *start of
        // the next* subframe, hence the +1.
        let tow = tow_base + sf_idx as u32 + 1;

        for w in 0..10 {
            let mut word = sbf[row][w];

            // Inject TOW into HOW word (word index 1), bits 29–13.
            if w == 1 {
                word = (word & !(0x1FFFF << 13)) | ((tow & 0x1FFFF) << 13);
            }

            // Carry the previous word's D29/D30 into bits 31–30.
            word = (word & 0x3FFF_FFFF) | ((prev << 30) & 0xC000_0000);

            // Words 2 and 10 of every subframe carry the two solved bits.
            let nib = w == 1 || w == 9;

            let checked = compute_checksum(word, nib);
            dwrd[base + w] = checked;
            prev = checked;
        }
    }

    (dwrd, prev)
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[expect(
    clippy::indexing_slicing,
    reason = "test code indexes fixed-size nav arrays and bit slices with loop-bounded indices"
)]
#[cfg(test)]
mod tests {
    use super::{
        EMPTY_WORD, FRAME_SECS, PARITY_MASKS, SUBFRAMES_PER_FRAME, WORDS_PER_FRAME,
        eph_to_subframes, frame_start, generate_nav_msg,
    };
    use crate::gps_sim::types::{Ephemeris, GpsTime, IonoUtc};

    /// Verify one word the way a GPS receiver does: recover the data bits using
    /// D30\*, recompute the six parity bits, and compare against what was sent.
    ///
    /// This is the decode direction, not a re-run of the encoder — it operates on
    /// the transmitted (possibly complemented) word and reverses that step first.
    fn receiver_parity_ok(word: u32, prev_word: u32) -> bool {
        let d29_star = prev_word & 0x2 != 0;
        let d30_star = prev_word & 0x1 != 0;

        // Recover the uncomplemented data bits.
        let received = word & 0x3FFF_FFC0;
        let d = if d30_star {
            received ^ 0x3FFF_FFC0
        } else {
            received
        };

        let carry_is_d29 = [true, false, true, false, false, true];
        let mut expected = 0u32;
        for (i, (&mask, &from_d29)) in PARITY_MASKS.iter().zip(carry_is_d29.iter()).enumerate() {
            let carry = u32::from(if from_d29 { d29_star } else { d30_star });
            expected |= ((carry + (mask & d).count_ones()) % 2) << (5 - i as u32);
        }

        expected == (word & 0x3F)
    }

    /// A fully-populated ephemeris. Every field is non-zero on purpose: a
    /// degenerate all-zero ephemeris produces legitimately-zero nav words, which
    /// would mask a word the encoder never wrote.
    fn realistic_eph(g0_sec: f64) -> Ephemeris {
        let mut eph = Ephemeris::default();
        eph.valid = true;
        eph.sqrta = 5153.6;
        eph.ecc = 0.004_312;
        eph.m0 = 0.913_7;
        eph.inc0 = 0.961_2;
        eph.omg0 = -1.234_5;
        eph.aop = 0.784_1;
        eph.omgdot = -8.13e-9;
        eph.idot = 2.4e-10;
        eph.deltan = 4.7e-9;
        eph.crs = -17.5;
        eph.crc = 233.7;
        eph.cuc = -1.02e-6;
        eph.cus = 8.11e-6;
        eph.cic = 1.86e-8;
        eph.cis = -9.31e-8;
        eph.af0 = -1.23e-4;
        eph.af1 = -9.09e-12;
        eph.af2 = 3.2e-18;
        eph.tgd = -1.02e-8;
        eph.iode = 61;
        eph.iodc = 61;
        eph.sva = 2;
        eph.svh = 0;
        eph.toe = GpsTime {
            week: 2367,
            sec: g0_sec,
        };
        eph.toc = eph.toe;
        eph
    }

    fn test_frame(g0_sec: f64) -> ([u32; WORDS_PER_FRAME], u32) {
        let eph = realistic_eph(g0_sec);
        let iono = IonoUtc::default();
        let sbf = eph_to_subframes(&eph, &iono);
        generate_nav_msg(
            &sbf,
            GpsTime {
                week: 2367,
                sec: g0_sec,
            },
            0,
            0,
        )
    }

    /// Every word of a frame must pass a receiver's parity check.
    ///
    /// This is the check that was failing for ~75% of words: the parity bits
    /// omitted their D29\*/D30\* carry term, so only words that happened to
    /// follow a word ending in `00` were accepted.
    #[test]
    fn every_word_passes_receiver_parity() {
        let (dwrd, _) = test_frame(216_000.0);

        let mut prev = 0u32;
        for (i, &word) in dwrd.iter().enumerate() {
            assert!(
                receiver_parity_ok(word, prev),
                "word {i} (subframe {}, word {}) fails receiver parity: \
                 0x{word:08X} after prev 0x{prev:08X}",
                i / 10 + 1,
                i % 10 + 1,
            );
            prev = word;
        }
    }

    /// Parity must chain unbroken across the 30 s frame boundary, or the first
    /// word of every frame fails and the receiver loses subframe sync.
    #[test]
    fn parity_chains_across_frame_boundary() {
        let (frame_a, last_a) = test_frame(216_000.0);

        let sbf = eph_to_subframes(&realistic_eph(216_000.0), &IonoUtc::default());
        let (frame_b, _) = generate_nav_msg(
            &sbf,
            GpsTime {
                week: 2367,
                sec: 216_030.0,
            },
            1,
            last_a,
        );

        assert_eq!(last_a, frame_a[WORDS_PER_FRAME - 1]);
        assert!(
            receiver_parity_ok(frame_b[0], last_a),
            "first word of the next frame fails parity against the previous frame's last word",
        );
    }

    /// IS-GPS-200: words 2 and 10 of every subframe must end with D29 = D30 = 0.
    ///
    /// An external constraint the encoder has to satisfy by solving the two
    /// non-information-bearing bits — independent of how parity is computed.
    #[test]
    fn words_2_and_10_have_zero_trailing_parity() {
        let (dwrd, _) = test_frame(216_000.0);

        for sf in 0..SUBFRAMES_PER_FRAME {
            for w in [1usize, 9] {
                let word = dwrd[sf * 10 + w];
                assert_eq!(
                    word & 0x3,
                    0,
                    "subframe {} word {}: D29/D30 must be zero, got 0x{word:08X}",
                    sf + 1,
                    w + 1,
                );
            }
        }
    }

    /// No word may be left blank — a run of zero bits destroys subframe sync.
    #[test]
    fn frame_is_completely_populated() {
        let (dwrd, _) = test_frame(216_000.0);
        let blank: Vec<usize> = (0..WORDS_PER_FRAME).filter(|&i| dwrd[i] == 0).collect();
        assert!(
            blank.is_empty(),
            "frame has {} unpopulated words at {blank:?}",
            blank.len(),
        );
        assert_eq!(WORDS_PER_FRAME, 50, "a frame is 30 s at 50 bps");
    }

    /// Each subframe starts with the TLM preamble and carries its own ID.
    #[test]
    fn subframe_headers_are_wellformed() {
        let (dwrd, _) = test_frame(216_000.0);

        for sf in 0..SUBFRAMES_PER_FRAME {
            let tlm = dwrd[sf * 10];
            // The TLM word may be transmitted complemented; check both polarities.
            let preamble = (tlm >> 22) & 0xFF;
            assert!(
                preamble == 0x8B || preamble == (!0x8Bu32 & 0xFF),
                "subframe {} has no 0x8B preamble (got 0x{preamble:02X})",
                sf + 1,
            );

            let how = dwrd[sf * 10 + 1];
            let d30_star = dwrd[sf * 10] & 0x1 != 0;
            let how_data = if d30_star { how ^ 0x3FFF_FFC0 } else { how };
            let sfid = (how_data >> 8) & 0x7;
            assert_eq!(sfid, sf as u32 + 1, "subframe ID in HOW word");
        }
    }

    /// The TOW in each HOW must name the start of the *next* subframe, derived
    /// from a 30-second-aligned frame start.
    ///
    /// This is what a receiver converts into transmit time; if the frame is not
    /// aligned, or the TOW is off by a subframe, the position solution is wrong
    /// even though acquisition and tracking work perfectly.
    #[test]
    fn tow_matches_aligned_frame_start() {
        let g0_sec = 216_000.0;
        let (dwrd, _) = test_frame(g0_sec);
        let tow_base = (g0_sec / 6.0) as u32;

        for sf in 0..SUBFRAMES_PER_FRAME {
            let how = dwrd[sf * 10 + 1];
            let d30_star = dwrd[sf * 10] & 0x1 != 0;
            let how_data = if d30_star { how ^ 0x3FFF_FFC0 } else { how };
            let tow = (how_data >> 13) & 0x1FFFF;
            assert_eq!(
                tow,
                tow_base + sf as u32 + 1,
                "subframe {} TOW should point at the next subframe boundary",
                sf + 1,
            );
        }
    }

    /// `frame_start` must snap to 30 s and tolerate accumulated float drift.
    #[test]
    fn frame_start_aligns_to_30s() {
        for (input, expected) in [
            (216_000.0, 216_000.0),
            (216_017.3, 216_000.0),
            (216_029.999, 216_000.0),
            (216_030.0, 216_030.0),
            // Drift from repeatedly adding 0.1 s must not fall into the previous frame.
            (216_029.999_999_9, 216_030.0),
        ] {
            let got = frame_start(GpsTime {
                week: 2367,
                sec: input,
            });
            assert!(
                (got.sec - expected).abs() < 1e-6,
                "frame_start({input}) = {}, expected {expected}",
                got.sec,
            );
        }
        assert!((FRAME_SECS - 30.0).abs() < f64::EPSILON);
    }

    /// Unused almanac pages still have to be valid, parity-bearing words.
    #[test]
    fn empty_pages_are_not_all_zero() {
        let sbf = eph_to_subframes(&Ephemeris::default(), &IonoUtc::default());
        assert_ne!(EMPTY_WORD, 0);
        // Subframe 4/5 page 1 lives at rows 3 and 4.
        for row in [3usize, 4] {
            for w in 2..10 {
                assert_ne!(sbf[row][w], 0, "row {row} word {w} is blank");
            }
        }
    }
}
