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

use super::types::{Ephemeris, IonoUtc, GpsTime, consts::GPS_PI};

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
const PARITY_MASKS: [u32; 6] = [
    0x3B1F_3480, // D25
    0x1D8F_9A40, // D26
    0x2EC7_CD00, // D27
    0x1763_E680, // D28
    0x2BB1_F340, // D29
    0x0B7A_89C0, // D30
];

// ── Parity ────────────────────────────────────────────────────────────────────

/// Apply IS-GPS-200 parity to a 30-bit navigation word.
///
/// `source` must already have D29\* in bit 31 and D30\* in bit 30.
/// If D30\* is set, data bits 29–6 are complemented before the parity
/// computation (IS-GPS-200 §20.3.5).
///
/// # Returns
/// The 30-bit word with 6 parity bits appended in bits 5–0.
pub fn compute_checksum(source: u32, d30_star: bool) -> u32 {
    // Complement data bits if D30* is set.
    let d = if d30_star { source ^ 0x3FFF_FFC0 } else { source };

    // Compute each parity bit as even-parity over the selected data bits.
    let parity: u32 = PARITY_MASKS
        .iter()
        .enumerate()
        .map(|(bit, &mask)| ((d & mask).count_ones() % 2) << (5 - bit as u32))
        .sum();

    (d & 0xFFFF_FFC0) | parity
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
#[expect(clippy::indexing_slicing, reason = "sbf rows/words indexed with literals (0..52, 0..9) and loop-bounded indices all within the [[u32;10];53] bounds; alpha/beta arrays are [f64;4]")]
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
              | pack(eph.svh as f64, 1.0, 6, 2);       // SV health
    sbf[0][3] = pack(eph.iodc as f64, 1.0, 2, 22)     // IODC MSBs
              | pack(eph.tgd, f64::powi(2.0, -31), 8, 6); // group delay (s)
    sbf[0][4] = pack(eph.iodc as f64, 1.0, 8, 22)     // IODC LSBs
              | pack(eph.toc.sec, 16.0, 16, 6);         // clock reference time
    sbf[0][5] = pack(eph.af2, f64::powi(2.0, -55), 8, 22)  // clock drift rate
              | pack(eph.af1, f64::powi(2.0, -43), 16, 6);  // clock drift
    sbf[0][6] = pack(eph.af0, f64::powi(2.0, -31), 22, 8); // clock bias
    // Words 7–9: unused — fill with EMPTY_WORD pattern (24 data bits each).
    for w in 7..=9 { sbf[0][w] = (EMPTY_WORD & 0x00FF_FFFF) << 6; }

    // ── Subframe 2: Ephemeris 1 ───────────────────────────────────────────────
    sbf[1][0] = PREAMBLE;
    sbf[1][1] = 0x2u32 << 8;
    sbf[1][2] = pack(eph.iode as f64, 1.0, 8, 22)           // IODE
              | pack(eph.crs, f64::powi(2.0, -5), 16, 6);   // Crs (m)
    sbf[1][3] = pack(eph.deltan / GPS_PI, f64::powi(2.0, -43), 16, 14) // Δn (rad/s)
              | pack(eph.m0 / GPS_PI, f64::powi(2.0, -31), 8, 6);      // M0 MSBs
    sbf[1][4] = pack(eph.m0 / GPS_PI, f64::powi(2.0, -31), 24, 6);    // M0 LSBs
    sbf[1][5] = pack(eph.cuc, f64::powi(2.0, -29), 16, 14) // Cuc (rad)
              | pack(eph.ecc, f64::powi(2.0, -33), 8, 6);   // eccentricity MSBs
    sbf[1][6] = pack(eph.ecc, f64::powi(2.0, -33), 24, 6); // eccentricity LSBs
    sbf[1][7] = pack(eph.cus, f64::powi(2.0, -29), 16, 14) // Cus (rad)
              | pack(eph.sqrta, f64::powi(2.0, -19), 8, 6); // √A MSBs
    sbf[1][8] = pack(eph.sqrta, f64::powi(2.0, -19), 24, 6); // √A LSBs
    sbf[1][9] = pack(eph.toe.sec, 16.0, 16, 14)              // TOE
              | pack(eph.iodc as f64, 1.0, 5, 8);            // FIT/IODC LSB

    // ── Subframe 3: Ephemeris 2 ───────────────────────────────────────────────
    sbf[2][0] = PREAMBLE;
    sbf[2][1] = 0x3u32 << 8;
    sbf[2][2] = pack(eph.cic, f64::powi(2.0, -29), 16, 14)     // Cic (rad)
              | pack(eph.omg0 / GPS_PI, f64::powi(2.0, -31), 8, 6); // Ω0 MSBs
    sbf[2][3] = pack(eph.omg0 / GPS_PI, f64::powi(2.0, -31), 24, 6); // Ω0 LSBs
    sbf[2][4] = pack(eph.cis, f64::powi(2.0, -29), 16, 14)     // Cis (rad)
              | pack(eph.inc0 / GPS_PI, f64::powi(2.0, -31), 8, 6);  // i0 MSBs
    sbf[2][5] = pack(eph.inc0 / GPS_PI, f64::powi(2.0, -31), 24, 6); // i0 LSBs
    sbf[2][6] = pack(eph.crc, f64::powi(2.0, -5), 16, 14)      // Crc (m)
              | pack(eph.aop / GPS_PI, f64::powi(2.0, -31), 8, 6);  // ω MSBs
    sbf[2][7] = pack(eph.aop / GPS_PI, f64::powi(2.0, -31), 24, 6); // ω LSBs
    sbf[2][8] = pack(eph.omgdot / GPS_PI, f64::powi(2.0, -43), 24, 6); // Ω̇ (rad/s)
    sbf[2][9] = pack(eph.iode as f64, 1.0, 8, 22)               // IODE
              | pack(eph.idot / GPS_PI, f64::powi(2.0, -43), 14, 8);  // IDOT (rad/s)

    // ── Subframes 4 & 5: empty almanac pages ─────────────────────────────────
    // Each page pair occupies rows 3+2p (SF4) and 4+2p (SF5), p = 0..25.
    for p in 0..25usize {
        for (row, sf_id) in [(3 + 2 * p, 4u32), (4 + 2 * p, 5u32)] {
            sbf[row][0] = PREAMBLE;
            sbf[row][1] = sf_id << 8; // subframe ID in HOW
            // Word 2: dataId(2b) + svId(6b) + 16b EMPTY
            sbf[row][2] = (data_id << 28) | ((EMPTY_WORD & 0xFFFF) << 6);
            // Words 3–8: 24 data bits of EMPTY pattern each
            for w in 3..=8 { sbf[row][w] = (EMPTY_WORD & 0x00FF_FFFF) << 6; }
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
        let scales_a = [f64::powi(2.0, -30), f64::powi(2.0, -27),
                        f64::powi(2.0, -24), f64::powi(2.0, -24)];
        let scales_b = [f64::powi(2.0,  11), f64::powi(2.0,  14),
                        f64::powi(2.0,  16), f64::powi(2.0,  16)];
        sbf[row][3] = pack(iono.alpha[0], scales_a[0], 8, 22)
                    | pack(iono.alpha[1], scales_a[1], 8, 14)
                    | pack(iono.alpha[2], scales_a[2], 8, 6);
        sbf[row][4] = pack(iono.alpha[3], scales_a[3], 8, 22)
                    | pack(iono.beta[0],  scales_b[0], 8, 14)
                    | pack(iono.beta[1],  scales_b[1], 8, 6);
        sbf[row][5] = pack(iono.beta[2], scales_b[2], 8, 22)
                    | pack(iono.beta[3], scales_b[3], 8, 14);
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

/// Inject GPS time (TOW), carry bits (D29\*/D30\*), and parity into 60 nav words.
///
/// Called once per 30-second interval (= one navigation message cycle) to
/// update the `dwrd` ring that the IQ generation loop reads bit-by-bit.
///
/// # Parameters
/// - `sbf`:   Raw 53×10 subframe array from [`eph_to_subframes`].
/// - `grx`:   Current GPS receiver time.
/// - `ipage`: Which subframe 4/5 page to broadcast (0-indexed, 0–24).
///
/// # Returns
/// 60 decoded nav words (6 subframes × 10 words), ready for bit extraction:
/// `bit = (dwrd[iword] >> (29 - ibit)) & 1`.
#[expect(clippy::indexing_slicing, reason = "sbf[row][w]: row is in rows[] (max 4+2*24=52<53), w<10; dwrd[base+w]: base+w<60")]
pub fn generate_nav_msg(sbf: &[[u32; 10]; 53], grx: GpsTime, ipage: usize) -> [u32; 60] {
    let mut dwrd = [0u32; 60];

    // Subframes 1–3 are in rows 0–2.
    // Subframe 4 page = row 3 + 2*ipage; Subframe 5 page = row 4 + 2*ipage.
    let rows = [0usize, 1, 2, 3 + 2 * ipage, 4 + 2 * ipage];

    for (sf_idx, &row) in rows.iter().enumerate() {
        let base = sf_idx * 10; // offset into dwrd[]

        // TOW count = number of 6-second intervals elapsed in the current week,
        // counting the interval that *starts* with this subframe.
        // sf_idx offsets successive subframes within the frame.
        let tow = (grx.sec / 6.0) as u32 + sf_idx as u32 + 1;

        let mut prev_word = 0u32;
        for w in 0..10 {
            let mut word = sbf[row][w];

            // Inject TOW into HOW word (word index 1), bits 29–13.
            if w == 1 {
                word = (word & !(0x1FFFF << 13)) | ((tow & 0x1FFFF) << 13);
            }

            // Prepend D29*/D30* carry bits from the previous word (bits 31–30).
            // The parity computation expects them in bits 31–30 of `source`.
            word = (word & 0x3FFF_FFFF) | ((prev_word & 0xC000_0000) >> 2);
            let d30_star = (prev_word >> 30) & 1 == 1;

            let checked = compute_checksum(word, d30_star);
            dwrd[base + w] = checked;
            prev_word = checked;
        }
    }

    dwrd
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parity_zero_data_zero_carry() {
        // All-zero data with no carry bits → all parity bits must be zero.
        let result = compute_checksum(0, false);
        assert_eq!(result & 0x3F, 0, "parity of zero word should be 0");
    }

    #[test]
    fn parity_all_ones_no_carry() {
        // 24 data bits all set → check that parity is deterministic (not zero).
        let data = 0x3FFF_FFC0u32; // 24 data bits set, carry = 0
        let result = compute_checksum(data, false);
        // Parity bits are in [5:0]; data bits should pass through.
        assert_eq!(result & 0x3FFF_FFC0, data);
    }

    #[test]
    fn parity_d30_star_flips_data() {
        // With D30* set, data bits must be complemented before parity.
        let data = 0x0000_0040u32; // only one data bit set
        let with_carry = compute_checksum(data, true);
        let without_carry = compute_checksum(data, false);
        // The parity should differ because data bits are complemented.
        assert_ne!(with_carry & 0x3F, without_carry & 0x3F,
            "D30* should change parity");
    }

    #[test]
    fn generate_nav_msg_60_words() {
        let eph = super::super::types::Ephemeris::default();
        let iono = super::super::types::IonoUtc::default();
        let sbf = eph_to_subframes(&eph, &iono);
        let dwrd = generate_nav_msg(&sbf, GpsTime { week: 2300, sec: 0.0 }, 0);
        assert_eq!(dwrd.len(), 60);
        // Preamble should survive in word 0 (subframe 1 TLM word).
        // After checksum the top 8 data bits contain 0x8B.
        let tlm_data = (dwrd[0] >> 22) & 0xFF;
        assert_eq!(tlm_data, 0x8B, "TLM preamble 0x8B not found after parity");
    }
}
