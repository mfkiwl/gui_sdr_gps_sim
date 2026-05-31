#!/usr/bin/env python3
"""
Analyse a GPS L1 C/A IQ file produced by gui_sdr_gps_sim.

Usage:
    python gnuradio/plot_iq_file.py [iq_file] [--rate SAMP_RATE]

Produces:  gps_signal_analysis.png
"""

import sys
import argparse
import numpy as np
import matplotlib
matplotlib.use("Agg")
import matplotlib.pyplot as plt
import matplotlib.gridspec as gridspec
from scipy import signal as sp

# ── CLI ───────────────────────────────────────────────────────────────────────
parser = argparse.ArgumentParser(description="GPS IQ file analyser")
parser.add_argument("iq_file", nargs="?", default="gps_signal.iq")
parser.add_argument("--rate", type=float, default=3_000_000.0)
args = parser.parse_args()

FS = args.rate
IQ_FILE = args.iq_file

# ── Load sc8 IQ file (interleaved signed-8-bit I/Q) ──────────────────────────
print(f"Reading {IQ_FILE} ...")
raw = np.fromfile(IQ_FILE, dtype=np.int8)
i_raw = raw[0::2].astype(np.float32)
q_raw = raw[1::2].astype(np.float32)
iq = (i_raw + 1j * q_raw) / 128.0          # normalise to ~[-1, 1]

n_total = len(iq)
duration = n_total / FS
print(f"  {n_total:,} samples  |  {duration:.2f} s  |  "
      f"RMS I={np.sqrt(np.mean(i_raw**2))/128:.3f}  Q={np.sqrt(np.mean(q_raw**2))/128:.3f}")

# Use at most 1 second for the plots (enough, and keeps FFT averages clean)
n_use = min(n_total, int(FS))
iq_use = iq[:n_use]

# ── Compute PSD via Welch ─────────────────────────────────────────────────────
FFT_SIZE = 4096
freqs, psd = sp.welch(iq_use, fs=FS, nperseg=FFT_SIZE,
                      window="blackmanharris", return_onesided=False)
freqs = np.fft.fftshift(freqs)
psd   = np.fft.fftshift(psd)
psd_db = 10 * np.log10(psd + 1e-20)

# ── Build figure ──────────────────────────────────────────────────────────────
fig = plt.figure(figsize=(15, 10))
fig.suptitle(
    f"GPS L1 C/A Baseband Signal Analysis  --  {IQ_FILE}\n"
    f"{FS/1e6:.3f} MSPS, 8-bit sc8,  {duration:.2f} s recorded",
    fontsize=13, fontweight="bold"
)
gs = gridspec.GridSpec(2, 2, figure=fig, hspace=0.4, wspace=0.32)

# ---- Panel 0: FFT spectrum (full top row) -----------------------------------
ax0 = fig.add_subplot(gs[0, :])
ax0.plot(freqs / 1e6, psd_db, color="royalblue", linewidth=0.7)
ax0.set_xlabel("Frequency offset from GPS L1 carrier (MHz)")
ax0.set_ylabel("PSD (dB/Hz)")
ax0.set_title("FFT Spectrum  --  expect flat ~2 MHz main lobe (GPS C/A sinc^2), nulls at +/-1.023 MHz")
ax0.set_xlim(-FS / 2e6, FS / 2e6)
ax0.grid(True, alpha=0.3)
for null_mhz in (-1.023, 1.023):
    ax0.axvline(null_mhz, color="orange", linewidth=1.0, linestyle="--", alpha=0.8)
peak_db = psd_db.max()
ax0.annotate("+/-1.023 MHz nulls", xy=(1.023, peak_db - 14),
             xytext=(1.35, peak_db - 8),
             arrowprops=dict(arrowstyle="->", color="orange"),
             color="orange", fontsize=9)
ax0.enable_autoscale = False

# ---- Panel 1: IQ constellation ---------------------------------------------
ax1 = fig.add_subplot(gs[1, 0])
n_scatter = min(10_000, n_use)
ax1.scatter(iq_use[:n_scatter].real, iq_use[:n_scatter].imag,
            s=0.5, alpha=0.2, color="steelblue", rasterized=True)
r_rms = float(np.sqrt(np.mean(np.abs(iq_use)**2)))
theta = np.linspace(0, 2 * np.pi, 300)
ax1.plot(r_rms * np.cos(theta), r_rms * np.sin(theta),
         "orange", linewidth=1.2, linestyle="--", label=f"RMS={r_rms:.3f}")
ax1.set_xlim(-1.5, 1.5)
ax1.set_ylim(-1.5, 1.5)
ax1.set_aspect("equal")
ax1.set_xlabel("I (norm.)")
ax1.set_ylabel("Q (norm.)")
ax1.set_title("IQ Constellation  --  expect circular cloud, radius 0.5-1.0")
ax1.grid(True, alpha=0.3)
ax1.legend(fontsize=9)

# ---- Panel 2: time domain (decimated x100) ----------------------------------
ax2 = fig.add_subplot(gs[1, 1])
dec = 100
iq_dec = iq_use[::dec]
t_dec_ms = np.arange(len(iq_dec)) * dec / FS * 1e3
n_plot = min(1024, len(iq_dec))
ax2.plot(t_dec_ms[:n_plot], iq_dec[:n_plot].real,
         color="cyan", linewidth=0.7, label="I")
ax2.plot(t_dec_ms[:n_plot], iq_dec[:n_plot].imag,
         color="tomato", linewidth=0.7, label="Q")
ax2.set_xlabel("Time (ms)")
ax2.set_ylabel("Amplitude (norm.)")
ax2.set_title("Time Domain (dec x100, ~30 kHz)  --  I and Q should look like equal noise")
ax2.set_ylim(-1.5, 1.5)
ax2.grid(True, alpha=0.3)
ax2.legend(fontsize=9)

# ---- Histogram of I and Q samples (inset) -----------------------------------
ax_hist = ax1.inset_axes([0.55, 0.02, 0.43, 0.32])
ax_hist.hist(i_raw[:n_use].astype(np.int32), bins=64, range=(-128, 128),
             color="cyan", alpha=0.6, label="I", density=True)
ax_hist.hist(q_raw[:n_use].astype(np.int32), bins=64, range=(-128, 128),
             color="tomato", alpha=0.5, label="Q", density=True)
ax_hist.set_title("8-bit histogram", fontsize=7)
ax_hist.tick_params(labelsize=6)
ax_hist.legend(fontsize=6)

# ---- Annotation: pass/fail ─────────────────────────────────────────────────
# Main-lobe check: average PSD in the central band should be >> sidelobe average.
# With multiple satellites at different Dopplers the PSD is non-uniform within the
# main lobe, so checking a single point is fragile -- compare band averages instead.
main_mask = np.abs(freqs) < 0.9e6
side_mask = (np.abs(freqs) > 1.2e6) & (np.abs(freqs) < 1.45e6)
main_avg_db = 10 * np.log10(np.mean(psd[main_mask]) + 1e-20)
side_avg_db = 10 * np.log10(np.mean(psd[side_mask]) + 1e-20)
bw_check = "PASS" if (main_avg_db - side_avg_db) > 6.0 else "FAIL"

# C/A null check: nulls at +-1.023 MHz must be at least 8 dB below the spectral peak.
null_p = psd_db[np.argmin(np.abs(freqs - 1.023e6))]
null_n = psd_db[np.argmin(np.abs(freqs + 1.023e6))]
null_check = "PASS" if null_p < (peak_db - 8) and null_n < (peak_db - 8) else "FAIL"

# RMS: with 8-12 channels and realistic path-loss the sc8 signal runs ~0.15-0.35 normalized.
rms_check = "PASS" if 0.10 < r_rms < 1.2 else "FAIL"
sym_check = "PASS" if abs(np.sqrt(np.mean(i_raw**2)) - np.sqrt(np.mean(q_raw**2))) / 128 < 0.05 else "FAIL"

checks = (
    f"Signal checks:\n"
    f"  Main lobe avg > sidelobes by >6 dB:                {bw_check}  "
    f"({main_avg_db - side_avg_db:.1f} dB)\n"
    f"  C/A nulls at +-1.023 MHz (>8 dB below peak):      {null_check}\n"
    f"  IQ RMS in range [0.10, 1.2]:                       {rms_check}  "
    f"(actual {r_rms:.3f})\n"
    f"  I/Q amplitude balance <5%:                         {sym_check}"
)
fig.text(0.5, 0.01, checks, ha="center", va="bottom", fontsize=10,
         family="monospace",
         bbox=dict(boxstyle="round,pad=0.5", facecolor="lightyellow", alpha=0.9))

out = "gps_signal_analysis.png"
fig.savefig(out, dpi=140, bbox_inches="tight")
print(f"Saved -> {out}")
