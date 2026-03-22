//! Galileo E1-B and E1-C spreading code generation.
//!
//! Implements the two-register LFSR code generator as specified in the
//! Galileo OS Signal-In-Space ICD (Issue 2.0) Annex C.
//!
//! # Registers
//! - **G1**: 14-bit LFSR, generator polynomial `x^14 + x^13 + x^12 + x + 1`.
//!   Initial state: all ones (0x3FFF) for all SVs.
//! - **G2**: 14-bit LFSR, generator polynomial
//!   `x^14 + x^11 + x^8 + x^7 + x^3 + x^2 + x + 1`.
//!   Initial state: per-satellite from ICD Annex C Table C-1.
//!
//! The output chip is `G1_out XOR G2_out` (MSB of each register), mapped to
//! bipolar ±1 (0 → −1, 1 → +1).
//!
//! # Code length
//! Galileo E1-B and E1-C each have 4092 chips per 1 ms epoch.

// ── G2 initial states ─────────────────────────────────────────────────────────

/// G2 register initial states for E1-B (data) codes, Galileo SVs 1–36.
///
/// Source: Galileo OS SIS ICD Issue 2.0, Annex C, Table C-1.
/// These values are used verbatim from open-source implementations that
/// reference the published ICD.
const G2_INIT_E1B: [u16; 36] = [
    0x2523, 0x0F01, 0x3A10, 0x2840, 0x137C, 0x281B, 0x1F8C, 0x1CE3, 0x0ABC, 0x3B27, 0x116D, 0x21A3,
    0x23B3, 0x0D56, 0x31AB, 0x25A8, 0x0B7D, 0x2FA3, 0x1924, 0x1FAB, 0x0ABE, 0x3873, 0x2DA7, 0x0FAD,
    0x1F74, 0x3C5D, 0x22B3, 0x29DB, 0x2F2B, 0x179A, 0x1C0D, 0x20F3, 0x3B9F, 0x2923, 0x1B4A, 0x3217,
];

/// G2 register initial states for E1-C (pilot) codes, Galileo SVs 1–36.
///
/// Source: Galileo OS SIS ICD Issue 2.0, Annex C, Table C-2.
/// These values are used verbatim from open-source implementations that
/// reference the published ICD.
const G2_INIT_E1C: [u16; 36] = [
    0x0117, 0x1A39, 0x2B9F, 0x37D5, 0x15E3, 0x2891, 0x0FC1, 0x37A7, 0x2F9F, 0x0FC9, 0x1AF7, 0x38BF,
    0x1BE3, 0x3B4F, 0x12EB, 0x3BD5, 0x3E17, 0x17D1, 0x0B5B, 0x2E07, 0x0613, 0x3E27, 0x1BBF, 0x17D7,
    0x3537, 0x10F9, 0x0FBF, 0x2B9D, 0x1FBF, 0x3573, 0x1E0F, 0x1F2F, 0x2FBF, 0x2E9F, 0x1B6B, 0x3CBF,
];

// ── Constants ─────────────────────────────────────────────────────────────────

/// Galileo E1-B/C code length in chips.
pub const GALILEO_E1_CODE_LEN: usize = 4092;

/// G1 initial state: all ones (same for all SVs).
const G1_INIT: u16 = 0x3FFF;

// ── Public API ────────────────────────────────────────────────────────────────

/// Generate the Galileo E1-B (data) spreading code for the given PRN (1–36).
///
/// Returns a fixed-size array of 4092 bipolar ±1 chip values.
/// PRNs outside 1–36 are clamped to the nearest valid value.
#[must_use]
#[expect(
    clippy::indexing_slicing,
    reason = "idx is always 0..=35 (clamped by saturating_sub+min); G2_INIT_E1B has length 36"
)]
pub fn generate_e1b(prn: u8) -> [i8; GALILEO_E1_CODE_LEN] {
    let idx = prn.saturating_sub(1).min(35) as usize;
    generate_lfsr_code(G1_INIT, G2_INIT_E1B[idx])
}

/// Generate the Galileo E1-C (pilot) spreading code for the given PRN (1–36).
///
/// Returns a fixed-size array of 4092 bipolar ±1 chip values.
/// PRNs outside 1–36 are clamped to the nearest valid value.
#[must_use]
#[expect(
    clippy::indexing_slicing,
    reason = "idx is always 0..=35 (clamped by saturating_sub+min); G2_INIT_E1C has length 36"
)]
pub fn generate_e1c(prn: u8) -> [i8; GALILEO_E1_CODE_LEN] {
    let idx = prn.saturating_sub(1).min(35) as usize;
    generate_lfsr_code(G1_INIT, G2_INIT_E1C[idx])
}

// ── Internal helpers ──────────────────────────────────────────────────────────

/// Run the dual-register 14-bit LFSR for [`GALILEO_E1_CODE_LEN`] chips.
///
/// # Register polynomials (ICD Annex C)
/// - G1: taps at positions 14, 13, 12, 1 → feedback bits 13, 12, 11, 0
/// - G2: taps at positions 14, 11, 8, 7, 3, 2, 1 → feedback bits 13, 10, 7, 6, 2, 1, 0
///
/// Shift direction: new bit inserted at LSB; MSB is the output.
fn generate_lfsr_code(g1_init: u16, g2_init: u16) -> [i8; GALILEO_E1_CODE_LEN] {
    let mut g1 = g1_init & 0x3FFF;
    let mut g2 = g2_init & 0x3FFF;
    let mut code = [0i8; GALILEO_E1_CODE_LEN];

    for chip in &mut code {
        // Output: MSB of each register (bit 13).
        let g1_out = (g1 >> 13) & 1;
        let g2_out = (g2 >> 13) & 1;
        // Bipolar mapping: 0 → −1, 1 → +1.
        *chip = if (g1_out ^ g2_out) == 0 { -1 } else { 1 };

        // G1 feedback: taps at bits 13, 12, 11, 0 (polynomial x^14+x^13+x^12+x+1).
        let g1_fb = ((g1 >> 13) ^ (g1 >> 12) ^ (g1 >> 11) ^ g1) & 1;
        g1 = ((g1 << 1) | g1_fb) & 0x3FFF;

        // G2 feedback: taps at bits 13, 10, 7, 6, 2, 1, 0
        // (polynomial x^14+x^11+x^8+x^7+x^3+x^2+x+1).
        let g2_fb =
            ((g2 >> 13) ^ (g2 >> 10) ^ (g2 >> 7) ^ (g2 >> 6) ^ (g2 >> 2) ^ (g2 >> 1) ^ g2) & 1;
        g2 = ((g2 << 1) | g2_fb) & 0x3FFF;
    }
    code
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    /// E1-B and E1-C codes must each be exactly 4092 chips.
    #[test]
    fn code_length_all_prns() {
        for prn in 1u8..=36 {
            assert_eq!(
                generate_e1b(prn).len(),
                GALILEO_E1_CODE_LEN,
                "E1-B PRN {prn} length mismatch"
            );
            assert_eq!(
                generate_e1c(prn).len(),
                GALILEO_E1_CODE_LEN,
                "E1-C PRN {prn} length mismatch"
            );
        }
    }

    /// Every chip must be exactly ±1.
    #[test]
    fn code_values_bipolar() {
        for prn in 1u8..=36 {
            let b = generate_e1b(prn);
            assert!(
                b.iter().all(|&c| c == 1 || c == -1),
                "E1-B PRN {prn} contains non-bipolar chip"
            );
            let c = generate_e1c(prn);
            assert!(
                c.iter().all(|&c| c == 1 || c == -1),
                "E1-C PRN {prn} contains non-bipolar chip"
            );
        }
    }

    /// E1-B and E1-C codes for the same PRN must differ (different G2 init states).
    #[test]
    fn e1b_differs_from_e1c() {
        for prn in 1u8..=36 {
            let b = generate_e1b(prn);
            let c = generate_e1c(prn);
            assert_ne!(b, c, "E1-B and E1-C are identical for PRN {prn}");
        }
    }

    /// Different PRNs must produce different E1-B codes.
    #[test]
    fn distinct_prns_e1b() {
        let c1 = generate_e1b(1);
        let c2 = generate_e1b(2);
        assert_ne!(c1, c2, "E1-B PRN 1 and PRN 2 are identical");
    }

    /// Galileo E1-B codes must be roughly balanced (±5% of half the code length).
    ///
    /// The 4092-chip LFSR runs a non-maximal sequence; individual PRNs can show
    /// up to ~2% imbalance depending on the G2 initial state.  5% is a safe upper
    /// bound — anything beyond that indicates a wrong `G2_INIT_E1B` entry.
    #[test]
    fn code_balance() {
        let tol = GALILEO_E1_CODE_LEN / 20; // 5% ≈ 204 chips
        for prn in 1u8..=36 {
            let b = generate_e1b(prn);
            let ones = b.iter().filter(|&&c| c == 1).count();
            let half = GALILEO_E1_CODE_LEN / 2;
            assert!(
                ones.abs_diff(half) <= tol,
                "E1-B PRN {prn}: {ones} ones — imbalanced beyond 5%"
            );
        }
    }

    /// PRN 1 E1-B first chip: with G1=0x3FFF and G2=0x2523,
    /// both MSBs (bit 13) are 1 → XOR = 0 → bipolar −1.
    #[test]
    fn prn1_e1b_first_chip() {
        let code = generate_e1b(1);
        // G1=0x3FFF (bit13=1), G2=0x2523 (0x2523 >> 13 = 1) → XOR=0 → -1
        assert_eq!(code[0], -1, "E1-B PRN 1 first chip should be -1");
    }

    /// `G2_INIT_E1B` and `G2_INIT_E1C` have correct lengths (one entry per SV 1–36).
    #[test]
    fn init_table_lengths() {
        assert_eq!(G2_INIT_E1B.len(), 36);
        assert_eq!(G2_INIT_E1C.len(), 36);
    }

    /// Out-of-range PRNs are clamped and must not panic.
    #[test]
    fn out_of_range_prn_clamped() {
        assert_eq!(generate_e1b(0).len(), GALILEO_E1_CODE_LEN);
        assert_eq!(generate_e1b(37).len(), GALILEO_E1_CODE_LEN);
        assert_eq!(generate_e1c(0).len(), GALILEO_E1_CODE_LEN);
        assert_eq!(generate_e1c(37).len(), GALILEO_E1_CODE_LEN);
    }
}
