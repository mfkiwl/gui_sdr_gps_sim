#!/usr/bin/env python3
"""
Synthesise a GPS L1 C/A baseband signal that matches what gui_sdr_gps_sim
generates, then analyse and plot it.

Produces:  gps_signal_analysis.png  (four-panel figure)

No GnuRadio required -- only numpy, scipy, matplotlib.
Run with:  python gnuradio/gen_and_plot.py
"""

import numpy as np
import matplotlib.pyplot as plt
import matplotlib.gridspec as gridspec
from scipy import signal as sp

# ── Simulation parameters matching gui_sdr_gps_sim defaults ──────────────────
FS       = 3_000_000      # sample rate: 3 MSPS
F_CHIP   = 1_023_000      # GPS C/A chip rate: 1.023 Mcps
DURATION = 0.5            # seconds to generate (0.5 s = 1.5 M samples, fast)
N        = int(FS * DURATION)
t        = np.arange(N, dtype=np.float64) / FS

# ── Visible GPS satellites (realistic Doppler and path-loss values) ───────────
# Each entry: (prn_seed, doppler_hz, code_phase_chips, path_gain, nav_bit)
#
# path_gain = 20_200_000 / range_m × ant_linear_gain
# For a satellite at 20 200 km (GPS altitude) directly overhead: gain = 1.0.
# Near-horizon satellites are further and have antenna pattern attenuation.
SATS = [
    (1,  +1_230,  412.3, 0.95,  1),   # high elevation
    (5,  -830,    102.7, 0.88, -1),
    (8,  +2_100,  876.1, 0.82,  1),
    (10, -1_450,  234.5, 0.75,  1),
    (15, +540,    667.9, 0.91, -1),
    (20, -2_200,  445.2, 0.71,  1),   # low elevation – more path loss
    (25, +1_780,  123.4, 0.84, -1),
    (30, -680,    789.0, 0.79,  1),
]

# ── Generate baseband IQ signal ───────────────────────────────────────────────
baseband = np.zeros(N, dtype=np.complex128)

for prn_seed, doppler, phase0_chips, gain, nav_bit in SATS:
    # Pseudo-random C/A code (1023 chips, ±1).
    # Using a seeded RNG gives each PRN a unique but repeatable code pattern.
    # The spectral shape (sinc² @ F_CHIP) is identical to real C/A codes.
    rng  = np.random.default_rng(prn_seed)
    code = rng.choice([-1, 1], size=1023)

    chip_time   = t * F_CHIP + phase0_chips
    chip_idx    = chip_time.astype(np.int64) % 1023
    code_samp   = code[chip_idx]           # ±1 spreading code at sample rate

    # Random carrier phase per satellite.
    phi0 = rng.uniform(0, 2 * np.pi)
    carrier = np.exp(1j * (2.0 * np.pi * doppler * t + phi0))

    baseband += gain * nav_bit * code_samp * carrier

# ── Add thermal noise ─────────────────────────────────────────────────────────
# SNR of GPS in space is about -20 dB; simulate ~-10 dB SNR at baseband
rng_noise = np.random.default_rng(0)
noise_sigma = 0.35
baseband += noise_sigma * (rng_noise.standard_normal(N)
                           + 1j * rng_noise.standard_normal(N))

# ── Quantise to 8-bit signed (sc8) matching our simulator's >>4 output ────────
# Our simulator accumulates 8–12 channels of ±250 × gain ~ ±0.95 and
# right-shifts by 4.  Scale so that the RMS of the summed signal maps to
# roughly ±80 in the 8-bit range (same headroom as the real simulator).
rms = np.sqrt(np.mean(np.abs(baseband) ** 2))
scale = 80.0 / rms

i_float = baseband.real * scale
q_float = baseband.imag * scale

i_i8 = np.clip(np.round(i_float), -128, 127).astype(np.int8)
q_i8 = np.clip(np.round(q_float), -128, 127).astype(np.int8)

# Interleave into sc8 byte stream [I0, Q0, I1, Q1, …]
sc8 = np.empty(2 * N, dtype=np.int8)
sc8[0::2] = i_i8
sc8[1::2] = q_i8

print(f"Generated {DURATION:.2f} s  |  {N:,} complex samples  |  "
      f"{len(sc8):,} bytes  |  RMS ~{rms:.3f}  ->  scaled RMS ~{rms*scale:.1f} LSB")

# ── Decode sc8 back to complex float (same path as GnuRadio analyser) ────────
iq_cf = (sc8[0::2].astype(np.float32) + 1j * sc8[1::2].astype(np.float32)) / 128.0

# ── Build the four-panel figure ───────────────────────────────────────────────
fig = plt.figure(figsize=(15, 10))
fig.suptitle(
    "GPS L1 C/A Baseband Signal Analysis\n"
    f"3 MSPS · 8-bit sc8 · {len(SATS)} satellites · {DURATION:.2f} s",
    fontsize=14, fontweight="bold"
)
gs = gridspec.GridSpec(2, 2, figure=fig, hspace=0.38, wspace=0.32)

# ─ Panel 0: FFT Spectrum ──────────────────────────────────────────────────────
ax0 = fig.add_subplot(gs[0, :])   # full top row

FFT_SIZE = 4096
f, psd = sp.welch(iq_cf, fs=FS, nperseg=FFT_SIZE, window="blackmanharris",
                  return_onesided=False)
f = np.fft.fftshift(f)
psd = np.fft.fftshift(psd)
psd_db = 10 * np.log10(psd + 1e-20)

ax0.plot(f / 1e6, psd_db, color="royalblue", linewidth=0.8)
ax0.set_xlabel("Frequency offset from L1 (MHz)")
ax0.set_ylabel("Power Spectral Density (dB/Hz)")
ax0.set_title("FFT Spectrum  --  GPS C/A sinc² main lobe ~2 MHz null-to-null, sidelobes outside")
ax0.set_xlim(-FS / 2e6, FS / 2e6)
ax0.grid(True, alpha=0.3)

# Annotate the null-to-null bandwidth
for x in (-1.023, 1.023):
    ax0.axvline(x, color="orange", linewidth=1.0, linestyle="--", alpha=0.8)
ax0.annotate("null @ ±1.023 MHz", xy=(1.023, psd_db.max() - 12),
             xytext=(1.4, psd_db.max() - 8),
             arrowprops=dict(arrowstyle="->", color="orange"), color="orange", fontsize=9)

# ─ Panel 1: IQ Constellation (scatter) ───────────────────────────────────────
ax1 = fig.add_subplot(gs[1, 0])

# Show only first 8 000 samples to keep the plot readable
n_show = 8_000
ax1.scatter(iq_cf[:n_show].real, iq_cf[:n_show].imag,
            s=0.6, alpha=0.25, color="steelblue", rasterized=True)
ax1.set_xlim(-1.5, 1.5)
ax1.set_ylim(-1.5, 1.5)
ax1.set_aspect("equal")
ax1.set_xlabel("I (norm.)")
ax1.set_ylabel("Q (norm.)")
ax1.set_title("IQ Constellation  --  circular cloud = good IQ balance\n"
              "and correct 8-bit quantisation")
ax1.grid(True, alpha=0.3)

# Mark the ±1 σ circle
theta = np.linspace(0, 2 * np.pi, 200)
r_rms = np.sqrt(np.mean(np.abs(iq_cf) ** 2))
ax1.plot(r_rms * np.cos(theta), r_rms * np.sin(theta),
         color="orange", linewidth=1.2, linestyle="--", label=f"RMS = {r_rms:.2f}")
ax1.legend(fontsize=8, loc="upper right")

# ─ Panel 2: Time domain (decimated) ──────────────────────────────────────────
ax2 = fig.add_subplot(gs[1, 1])

# Decimate by 100 (same as GnuRadio keep_one_in_n) to show ~30 kHz content
dec = 100
iq_dec = iq_cf[::dec]
t_dec = t[:len(iq_dec) * dec:dec] * 1e3   # ms
n_plot = min(1024, len(iq_dec))

ax2.plot(t_dec[:n_plot], iq_dec[:n_plot].real, color="cyan",    linewidth=0.7, label="I")
ax2.plot(t_dec[:n_plot], iq_dec[:n_plot].imag, color="tomato",  linewidth=0.7, label="Q")
ax2.set_xlabel("Time (ms)")
ax2.set_ylabel("Amplitude (norm.)")
ax2.set_title("Time Domain (dec ×100, ~30 kHz)  --  I and Q\n"
              "appear as band-limited noise with equal amplitudes")
ax2.set_ylim(-1.5, 1.5)
ax2.grid(True, alpha=0.3)
ax2.legend(fontsize=8, loc="upper right")

# ─ Annotation block: what to look for ────────────────────────────────────────
note = (
    "What a good GPS signal looks like:\n"
    "  Spectrum  flat ±1 MHz plateau -> narrow sidelobes  (GPS C/A sinc²)\n"
    "  Constellation  circular, radius ~0.6–1.0          (no IQ imbalance)\n"
    "  Time domain     I ~ Q amplitude, band-limited noise appearance\n\n"
    "Red flags:\n"
    "  DC spike only -> Doppler not applied\n"
    "  Rectangular blob -> 8-bit clipping / gain too high\n"
    "  Flat noise floor -> no signal / gain too low\n"
    "  Two dots on real axis -> carrier stripped (should not happen at baseband)"
)
fig.text(0.5, 0.01, note, ha="center", va="bottom", fontsize=8.5,
         bbox=dict(boxstyle="round,pad=0.5", facecolor="lightyellow", alpha=0.8))

out_path = "gps_signal_analysis.png"
fig.savefig(out_path, dpi=130, bbox_inches="tight")
print(f"Saved -> {out_path}")
plt.show()
