//! GPS L1 C/A Gold code generator.
//!
//! Each PRN (1–32) uses two 10-stage maximal-length linear feedback shift
//! registers (G1 and G2).  Their raw 1023-chip sequences are generated once;
//! for each PRN the G2 sequence is cyclically shifted by a PRN-specific
//! **delay** before XOR-ing with G1, producing a unique Gold code.
//!
//! # LFSR polynomials (IS-GPS-200 Table 3-Ia)
//! - **G1**: x¹⁰ + x³ + 1  — feedback: stages 3 and 10 (0-indexed: 2 and 9)
//! - **G2**: x¹⁰ + x⁹ + x⁸ + x⁶ + x³ + x² + 1
//!   feedback: stages 2,3,6,8,9,10 (0-indexed: 1,2,5,7,8,9)
//!
//! Both registers start in the all-ones state.
//!
//! # Phase assignment via delay (IS-GPS-200 Table 3-Ia)
//! For each PRN the C/A code chip is: `G1[i] XOR G2[(i + 1023 − delay) % 1023]`
//! where `delay` is the per-PRN chip offset listed in IS-GPS-200 Table 3-Ia.
//! This is the canonical delay method, equivalent to the two-tap phase
//! selection method but implemented as a cyclic shift of the raw G2 sequence.
//!
//! # Verification
//! PRN 1 first chips: `1 1 1 1 1 1 0 1 0 1 0 1 0 0 1 0 0 0 0 0 0 1 1 …`

/// G2 chip delays for PRN 1–32 (chips), from IS-GPS-200 Table 3-Ia.
///
/// `ca[i] = g1[i] XOR g2[(i + 1023 - delay) % 1023]`
const G2_DELAYS: [usize; 32] = [
    5, 6, 7, 8, 17, 18, 139, 140, 141, 251,
    252, 254, 255, 256, 257, 258, 469, 470, 471, 472,
    473, 474, 509, 512, 513, 514, 515, 516, 859, 860,
    861, 862,
];

/// Generate the C/A spreading code for `prn` (1-based, 1–32).
///
/// Returns 1023 chips as raw `{0, 1}` values (`i8`).  Use [`to_bipolar`] to
/// convert to the `{−1, +1}` representation used in IQ signal generation.
///
/// The algorithm follows IS-GPS-200 §3.3.2.3 exactly:
/// - Run G1 and G2 for 1023 chips each to build raw sequences.
/// - C/A chip `i` = `G1[i] XOR G2[(i + 1023 − delay) % 1023]`.
///
/// # Panics
/// Panics if `prn` is not in the range 1–32.
#[expect(clippy::indexing_slicing, reason = "G2_DELAYS indexed by (prn-1) which is 0..31; g1/g2 regs are [u8;10] indexed at known positions; g1_seq/g2_seq/code are [;1023] indexed by i<1023")]
pub fn generate(prn: u8) -> [i8; 1023] {
    assert!((1..=32).contains(&prn), "PRN must be 1–32, got {prn}");
    let delay = G2_DELAYS[(prn - 1) as usize];

    // Both registers start at the all-ones state.
    let mut g1_reg = [1u8; 10];
    let mut g2_reg = [1u8; 10];

    // Generate raw G1 and G2 sequences (output from stage 10 = index 9).
    let mut g1_seq = [0u8; 1023];
    let mut g2_seq = [0u8; 1023];

    for i in 0..1023 {
        g1_seq[i] = g1_reg[9];
        g2_seq[i] = g2_reg[9];

        // Advance G1: feedback = stage3 XOR stage10 (0-indexed: 2, 9)
        let fb1 = g1_reg[2] ^ g1_reg[9];
        g1_reg.rotate_right(1);
        g1_reg[0] = fb1;

        // Advance G2: feedback = stages 2,3,6,8,9,10 (0-indexed: 1,2,5,7,8,9)
        let fb2 = g2_reg[1] ^ g2_reg[2] ^ g2_reg[5] ^ g2_reg[7] ^ g2_reg[8] ^ g2_reg[9];
        g2_reg.rotate_right(1);
        g2_reg[0] = fb2;
    }

    // Combine: G1[i] XOR G2[(i + 1023 - delay) % 1023]
    let mut code = [0i8; 1023];
    for i in 0..1023 {
        let j = (i + 1023 - delay) % 1023;
        code[i] = (g1_seq[i] ^ g2_seq[j]) as i8;
    }

    code
}

/// Convert a raw `{0, 1}` chip array to bipolar `{−1, +1}`.
///
/// Mapping: `0 → −1`, `1 → +1`.  The bipolar form is used directly in the
/// IQ accumulation loop where the chip is multiplied by the carrier phasor.
#[expect(clippy::indexing_slicing, reason = "from_fn guarantees i<1023; code is [i8;1023]")]
pub fn to_bipolar(code: &[i8; 1023]) -> [i8; 1023] {
    std::array::from_fn(|i| 2 * code[i] - 1)
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn code_length_all_prns() {
        for prn in 1u8..=32 {
            assert_eq!(generate(prn).len(), 1023, "PRN {prn} code length mismatch");
        }
    }

    #[test]
    fn code_values_binary() {
        for prn in 1u8..=32 {
            let code = generate(prn);
            assert!(code.iter().all(|&b| b == 0 || b == 1),
                "PRN {prn} has non-binary chip values");
        }
    }

    /// PRN 1 first 23 chips, verified against the reference C simulator
    /// (multi-sdr-gps-sim) which uses IS-GPS-200 delay=5 for PRN 1.
    #[test]
    fn prn1_first_chips() {
        let code = generate(1);
        let expected: &[i8] = &[1,1,0,0,1,0,0,0,0,0,1,1,1,0,0,1,0,1,0,0,1,0,0];
        assert_eq!(&code[..23], expected, "PRN 1 chip sequence mismatch");
    }

    /// All PRNs must have 511 or 512 ones (balanced Gold code property).
    ///
    /// GPS C/A codes of length 2^10 − 1 = 1023 have bipolar sums of ±1.
    #[test]
    fn code_balance() {
        for prn in 1u8..=32 {
            let code = generate(prn);
            let ones = code.iter().filter(|&&b| b == 1).count();
            assert!(ones == 511 || ones == 512,
                "PRN {prn}: expected 511 or 512 ones, got {ones}");
        }
    }

    /// Different PRNs must produce different codes.
    #[test]
    #[expect(clippy::indexing_slicing, reason = "i and j are bounded by codes.len() which is 32")]
    fn codes_are_unique() {
        let codes: Vec<[i8; 1023]> = (1u8..=32).map(generate).collect();
        for i in 0..codes.len() {
            for j in (i + 1)..codes.len() {
                assert_ne!(codes[i], codes[j],
                    "PRN {} and PRN {} have identical codes", i + 1, j + 1);
            }
        }
    }

    #[test]
    fn bipolar_conversion() {
        let code    = generate(1);
        let bipolar = to_bipolar(&code);
        assert!(bipolar.iter().all(|&b| b == -1 || b == 1));
        // PRN 1 has 512 ones → bipolar sum = 512 − 511 = +1.
        let sum: i32 = bipolar.iter().map(|&b| b as i32).sum();
        assert_eq!(sum, 1);
    }
}
