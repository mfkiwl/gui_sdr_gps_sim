//! End-to-end signal-chain tests.
//!
//! These run the full simulator — synthetic RINEX → ephemeris → channel
//! allocation → IQ generation → sc8 file — and then assert on the properties of
//! the generated baseband that a GPS receiver actually depends on:
//!
//! * a sinc²-shaped main lobe well above the sidelobes (the signal is spread,
//!   not a DC tone),
//! * C/A code nulls at ±1.023 MHz,
//! * IQ RMS inside the usable range of the sc8 quantiser,
//! * balanced I and Q amplitudes.
//!
//! They are the Rust counterpart of `gnuradio/plot_iq_file.py`, and they cover
//! the regressions listed in `test_result.md`: an unspread carrier (chip index
//! not advancing per sample), a wrapped accumulator (missing gain compensation),
//! and zero initial Doppler.

use std::f64::consts::PI;
use std::fmt::Write as _;
use std::path::Path;

use rustfft::FftPlanner;
use rustfft::num_complex::Complex;

use gui_sdr_gps_sim::gps_sim::{GpsTime, Location, SdrOutput, Simulator, StartTime};

// ── Synthetic RINEX 2.11 navigation file ─────────────────────────────────────

/// Epoch used for every generated ephemeris: 2026-05-31 00:00:00 UTC.
const EPOCH: (i32, i32, i32, i32, i32, f64) = (26, 5, 31, 0, 0, 0.0);

/// GPS week / seconds-of-week matching [`EPOCH`].
const TOE_WEEK: i32 = 2367;
const TOE_SEC: f64 = 0.0;

/// Orbit inclination (rad) — nominal GPS 55°.
const INC0: f64 = 0.9599;
/// Rate of right ascension (rad/s) — nominal GPS.
const OMGDOT: f64 = -8.0e-9;

/// Format one f64 as a RINEX `D`-exponent field: exactly 19 characters,
/// `[-]0.dddddddddddd D[+-]ee`.
///
/// The parser reads these by fixed column offset, so the width must be exact.
fn d19(v: f64) -> String {
    if v == 0.0 {
        return " 0.000000000000D+00".to_owned();
    }
    let mut exp = v.abs().log10().floor() as i32 + 1;
    let mut mant = v / 10f64.powi(exp);
    // Guard against rounding pushing the mantissa back up to 1.0.
    if mant.abs() >= 1.0 {
        mant /= 10.0;
        exp += 1;
    }
    let sign = if exp < 0 { '-' } else { '+' };
    let s = format!("{mant:>15.12}D{sign}{:02}", exp.abs());
    debug_assert_eq!(s.len(), 19, "RINEX field must be 19 chars, got {s:?}");
    s
}

/// Write the four broadcast-orbit values of one continuation line.
fn orbit_line(out: &mut String, values: [f64; 4]) {
    out.push_str("   ");
    for v in values {
        out.push_str(&d19(v));
    }
    out.push('\n');
}

/// Build a RINEX 2.11 GPS navigation file with `n_sv` satellites spread across
/// six orbital planes, so that a useful number are above the horizon from any
/// mid-latitude receiver.
///
/// The orbits are nominal-GPS but idealised (circular, no harmonic corrections);
/// that is deliberate — the test is about the signal chain, not orbit fidelity.
fn synth_rinex(n_sv: usize) -> String {
    let mut out = String::new();

    // ── Header ───────────────────────────────────────────────────────────────
    out.push_str("     2.11           N: GPS NAV DATA                     RINEX VERSION / TYPE\n");
    out.push_str(
        "synth_rinex         test                20260531 000000 UTC PGM / RUN BY / DATE\n",
    );
    out.push_str("     0.1211D-07  0.2235D-07 -0.1192D-06 -0.1192D-06          ION ALPHA\n");
    out.push_str("     0.9626D+05  0.9830D+05 -0.6554D+05 -0.5243D+06          ION BETA\n");
    out.push_str(
        "     0.000000000000D+00  0.000000000000D+00     0  2367          DELTA-UTC: A0,A1,T,W\n",
    );
    out.push_str("    18                                                      LEAP SECONDS\n");
    out.push_str("                                                            END OF HEADER\n");

    // ── Records ──────────────────────────────────────────────────────────────
    let a = 26_559_800.0_f64; // semi-major axis (m) — nominal GPS
    let sqrta = a.sqrt();
    let toe_sec = TOE_SEC;
    let toe_week = f64::from(TOE_WEEK);
    let (yy, mm, dd, hh, mi, ss) = EPOCH;

    for i in 0..n_sv {
        let prn = i + 1;
        // Six planes, satellites evenly spaced within each plane.
        let plane = i % 6;
        let slot = i / 6;
        let omg0 = 2.0 * PI * plane as f64 / 6.0;
        let per_plane = n_sv.div_ceil(6).max(1);
        let m0 = 2.0 * PI * slot as f64 / per_plane as f64 + PI * plane as f64 / 6.0; // stagger planes against each other

        // Line 1: PRN (I2), epoch (5×I3 + F5.1), then af0/af1/af2.
        writeln!(
            out,
            "{prn:2} {yy:2} {mm:2} {dd:2} {hh:2} {mi:2}{ss:5.1}{}{}{}",
            d19(0.0),
            d19(0.0),
            d19(0.0),
        )
        .expect("writing to a String cannot fail");

        // Broadcast orbit lines 1–7 — four fields each, in RINEX field order.
        orbit_line(&mut out, [0.0, 0.0, 0.0, m0]); //  0-3:  iode, crs, deltan, m0
        orbit_line(&mut out, [0.0, 0.0, 0.0, sqrta]); //  4-7:  cuc, ecc, cus, sqrtA
        orbit_line(&mut out, [toe_sec, 0.0, omg0, 0.0]); //  8-11: toe, cic, omg0, cis
        orbit_line(&mut out, [INC0, 0.0, 0.0, OMGDOT]); // 12-15: i0, crc, aop, omgdot
        orbit_line(&mut out, [0.0, 0.0, toe_week, 0.0]); // 16-19: idot, L2 codes, week, L2P
        orbit_line(&mut out, [0.0, 0.0, 0.0, 0.0]); // 20-23: sva, svh, tgd, iodc
        orbit_line(&mut out, [0.0, 4.0, 0.0, 0.0]); // 24-27: ttx, fit, spare, spare
    }

    out
}

// ── IQ analysis helpers ──────────────────────────────────────────────────────

/// One sc8 IQ recording, split into normalised I and Q channels.
struct IqCapture {
    i: Vec<f64>,
    q: Vec<f64>,
}

impl IqCapture {
    fn load(path: &Path) -> Self {
        let raw = std::fs::read(path).expect("IQ file should exist after the run");
        assert!(
            raw.len() >= 2,
            "IQ file is empty — the simulator produced no samples",
        );
        let mut i = Vec::with_capacity(raw.len() / 2);
        let mut q = Vec::with_capacity(raw.len() / 2);
        for pair in raw.chunks_exact(2) {
            // sc8: interleaved signed 8-bit, normalised to ±1.0.
            let (Some(&si), Some(&sq)) = (pair.first(), pair.get(1)) else {
                unreachable!("chunks_exact(2) always yields two elements")
            };
            i.push(f64::from(si as i8) / 127.0);
            q.push(f64::from(sq as i8) / 127.0);
        }
        Self { i, q }
    }

    fn len(&self) -> usize {
        self.i.len()
    }

    fn rms_i(&self) -> f64 {
        rms(&self.i)
    }

    fn rms_q(&self) -> f64 {
        rms(&self.q)
    }

    /// Averaged power spectrum over `n_frames` FFT frames of `n_fft` samples,
    /// returned in FFT bin order (bin 0 = DC).
    fn spectrum(&self, n_fft: usize, n_frames: usize) -> Vec<f64> {
        assert!(
            self.len() >= n_fft * n_frames,
            "not enough samples ({}) for {n_frames} frames of {n_fft}",
            self.len(),
        );

        let fft = FftPlanner::<f64>::new().plan_fft_forward(n_fft);
        let mut acc = vec![0.0_f64; n_fft];

        // Hann window suppresses spectral leakage that would otherwise fill the
        // C/A nulls we are trying to measure.
        let window: Vec<f64> = (0..n_fft)
            .map(|n| 0.5 - 0.5 * (2.0 * PI * n as f64 / n_fft as f64).cos())
            .collect();

        for (i_frame, q_frame) in self
            .i
            .chunks_exact(n_fft)
            .zip(self.q.chunks_exact(n_fft))
            .take(n_frames)
        {
            let mut buf: Vec<Complex<f64>> = i_frame
                .iter()
                .zip(q_frame)
                .zip(&window)
                .map(|((&i, &q), &w)| Complex::new(i * w, q * w))
                .collect();
            fft.process(&mut buf);
            for (a, c) in acc.iter_mut().zip(buf.iter()) {
                *a += c.norm_sqr();
            }
        }

        for a in &mut acc {
            *a /= n_frames as f64;
        }
        acc
    }
}

fn rms(v: &[f64]) -> f64 {
    (v.iter().map(|x| x * x).sum::<f64>() / v.len() as f64).sqrt()
}

/// Mean power (linear) of the bins whose frequency lies in `[lo, hi)` Hz,
/// measured as an absolute offset from DC so it covers both spectrum halves.
fn band_power(spectrum: &[f64], sample_rate: f64, lo: f64, hi: f64) -> f64 {
    let n = spectrum.len();
    let bin_hz = sample_rate / n as f64;
    let mut sum = 0.0;
    let mut count = 0usize;
    for (k, p) in spectrum.iter().enumerate() {
        // Map bins above Nyquist to negative frequencies.
        let f = if k <= n / 2 {
            k as f64 * bin_hz
        } else {
            (k as f64 - n as f64) * bin_hz
        };
        let fa = f.abs();
        if fa >= lo && fa < hi {
            sum += p;
            count += 1;
        }
    }
    assert!(count > 0, "no FFT bins in band {lo}..{hi} Hz");
    sum / count as f64
}

fn db(ratio: f64) -> f64 {
    10.0 * ratio.max(f64::MIN_POSITIVE).log10()
}

// ── Fixture ──────────────────────────────────────────────────────────────────

/// Amsterdam Centraal — the reference location used throughout `test_result.md`.
fn amsterdam() -> Location {
    Location::degrees(52.3791, 4.9003, 5.0)
}

/// Run the simulator for `secs` seconds into a temporary IQ file and load it.
fn generate_capture(secs: u32) -> (IqCapture, f64) {
    let dir = tempfile::tempdir().expect("create temp dir");
    let rinex_path = dir.path().join("synth.26n");
    let iq_path = dir.path().join("out.iq");

    std::fs::write(&rinex_path, synth_rinex(32)).expect("write synthetic RINEX");

    let sample_rate = 3_000_000.0_f64;

    Simulator::builder()
        .rinex(rinex_path.to_string_lossy().as_ref())
        .location(amsterdam())
        // Pin the start to the ephemeris epoch so the test does not drift with
        // the wall clock.
        .start_time(StartTime::Gps(GpsTime {
            week: TOE_WEEK,
            sec: TOE_SEC,
        }))
        .duration_secs(secs)
        .output(SdrOutput::IqFile {
            path: iq_path.to_string_lossy().into_owned(),
        })
        .build()
        .expect("synthetic RINEX should parse and build a Simulator")
        .run()
        .expect("simulation to an IQ file should succeed");

    let capture = IqCapture::load(&iq_path);
    let expected = (secs as f64 * sample_rate) as usize;
    assert!(
        capture.len() >= expected / 2,
        "expected ~{expected} samples for {secs}s, got {}",
        capture.len(),
    );

    (capture, sample_rate)
}

// ── Tests ────────────────────────────────────────────────────────────────────

/// The synthetic RINEX fixture must actually parse and yield visible satellites —
/// if it does not, every other test in this file is vacuous.
#[test]
fn synthetic_rinex_parses_with_visible_satellites() {
    let dir = tempfile::tempdir().expect("create temp dir");
    let path = dir.path().join("synth.26n");
    std::fs::write(&path, synth_rinex(32)).expect("write synthetic RINEX");

    let nav = gui_sdr_gps_sim::gps_sim::rinex::load(path.to_string_lossy().as_ref())
        .expect("synthetic RINEX should parse");

    let valid = nav
        .eph()
        .first()
        .map(|set| set.iter().filter(|e| e.valid).count())
        .unwrap_or(0);
    assert_eq!(valid, 32, "all 32 synthetic ephemerides should be valid");
}

/// The generated baseband must be spread, not a carrier.
///
/// The main-lobe band deliberately **excludes** DC: an unmodulated carrier puts
/// all its power in the DC bin, which would otherwise sail through this check.
/// Measuring 50 kHz–1.023 MHz against the first sidelobe tests that the power is
/// spread across the lobe, which is what a correlating receiver needs.
///
/// This is the regression guard for the `chip_idx`-at-code-boundary bug: freeze
/// the chip index and the modulation collapses, dropping this ratio to ~0 dB.
#[test]
fn main_lobe_is_above_sidelobes() {
    let (capture, sample_rate) = generate_capture(2);
    let spectrum = capture.spectrum(4096, 64);

    // Main lobe of GPS L1 C/A spans ±1.023 MHz; the first sidelobe sits beyond it.
    let main = band_power(&spectrum, sample_rate, 50_000.0, 1_023_000.0);
    let side = band_power(&spectrum, sample_rate, 1_100_000.0, 1_450_000.0);
    let ratio_db = db(main / side);

    assert!(
        ratio_db > 6.0,
        "main lobe is only {ratio_db:.1} dB above the sidelobes (need >6 dB) — \
         the signal is not properly spread",
    );
}

/// The spectrum must dip at ±1.023 MHz, where the C/A code's sinc² envelope has
/// its first null.
///
/// This checks the *shape* of the lobe rather than merely its presence, so it
/// catches distortions — such as a wrapping accumulator — that fill the nulls in.
/// The reference band skips DC for the same reason as
/// [`main_lobe_is_above_sidelobes`].
#[test]
fn ca_code_nulls_present() {
    let (capture, sample_rate) = generate_capture(2);
    let spectrum = capture.spectrum(4096, 64);

    let peak = band_power(&spectrum, sample_rate, 50_000.0, 500_000.0);
    // A narrow band straddling the theoretical null at 1.023 MHz.
    let null = band_power(&spectrum, sample_rate, 990_000.0, 1_056_000.0);
    let depth_db = db(peak / null);

    assert!(
        depth_db > 3.0,
        "null at ±1.023 MHz is only {depth_db:.1} dB below the peak (need >3 dB)",
    );
}

/// The sc8 quantiser must land in its usable range.
///
/// Too low wastes dynamic range; too high means the accumulator is wrapping,
/// which is what the missing gain compensation used to cause.
#[test]
fn iq_amplitude_in_usable_range() {
    let (capture, _) = generate_capture(1);

    let (ri, rq) = (capture.rms_i(), capture.rms_q());
    for (name, r) in [("I", ri), ("Q", rq)] {
        assert!(
            (0.02..=1.2).contains(&r),
            "{name} RMS {r:.4} is outside the usable sc8 range 0.02–1.2",
        );
    }

    // Wrapping shows up as a large population of full-scale samples.
    let clipped = capture
        .i
        .iter()
        .chain(capture.q.iter())
        .filter(|x| x.abs() >= 1.0)
        .count();
    let clipped_frac = clipped as f64 / (capture.len() * 2) as f64;
    assert!(
        clipped_frac < 0.01,
        "{:.2}% of samples are at full scale — the IQ accumulator is wrapping",
        clipped_frac * 100.0,
    );
}

/// I and Q must carry the same power; a large imbalance means the quadrature
/// arms diverged somewhere in the accumulation loop.
#[test]
fn iq_channels_are_balanced() {
    let (capture, _) = generate_capture(1);

    let (ri, rq) = (capture.rms_i(), capture.rms_q());
    let imbalance = (ri - rq).abs() / ri.max(rq);
    assert!(
        imbalance < 0.05,
        "I/Q imbalance is {:.1}% (I={ri:.4}, Q={rq:.4}), need <5%",
        imbalance * 100.0,
    );
}

/// Power must not be concentrated at DC.
///
/// A DC-dominated spectrum is the specific failure mode of an unspread signal,
/// and it is invisible to the RMS checks above.
#[test]
fn power_is_not_concentrated_at_dc() {
    let (capture, sample_rate) = generate_capture(1);
    let spectrum = capture.spectrum(4096, 32);

    let dc = band_power(&spectrum, sample_rate, 0.0, 10_000.0);
    let spread = band_power(&spectrum, sample_rate, 100_000.0, 900_000.0);

    assert!(
        db(dc / spread) < 20.0,
        "DC bin is {:.1} dB above the spread band — the carrier is not modulated",
        db(dc / spread),
    );
}

/// Cross a 30-second navigation frame boundary through the full simulator.
///
/// The frame swap is exercised at the bit level by the unit tests in
/// `channel.rs`; this checks the integrated path, where `prepare_next_frame` is
/// driven from the 100 ms step loop and `advance_nav_bit` from the per-sample
/// loop.  It is `#[ignore]`d because the simulator gates itself to real time, so
/// a 35-second run takes 35 seconds of wall clock.
///
/// Run with: `cargo test --release --test signal_chain -- --ignored`
#[test]
#[ignore = "runs in real time — takes ~35 s"]
fn survives_a_frame_boundary() {
    let (capture, sample_rate) = generate_capture(35);

    // The signal must still be well formed on the far side of the swap: analyse
    // only the last 5 seconds, well past the 30 s boundary.
    let tail_start = capture.len().saturating_sub((5.0 * sample_rate) as usize);
    let tail = IqCapture {
        i: capture.i.get(tail_start..).unwrap_or_default().to_vec(),
        q: capture.q.get(tail_start..).unwrap_or_default().to_vec(),
    };

    let spectrum = tail.spectrum(4096, 64);
    let main = band_power(&spectrum, sample_rate, 50_000.0, 1_023_000.0);
    let side = band_power(&spectrum, sample_rate, 1_100_000.0, 1_450_000.0);
    assert!(
        db(main / side) > 6.0,
        "signal degraded after the frame boundary: {:.1} dB",
        db(main / side),
    );
}
