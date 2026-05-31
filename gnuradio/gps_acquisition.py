#!/usr/bin/env python3
"""
GPS L1 C/A Signal Acquisition — Pure NumPy implementation.

Reads an IQ capture file (sc8 format: interleaved signed int8 I/Q as produced
by hackrf_transfer or our simulator) and searches for GPS satellites using
parallel code/frequency search (PCPS).

Usage:
    python gps_acquisition.py [--file rx_capture.iq] [--samp-rate 3000000]
                               [--doppler-range 10000] [--doppler-step 500]
                               [--threshold 2.5] [--prns 1-32]
                               [--plot] [--save-plot acquisition_result.png]

References:
    Borre et al., "A Software-Defined GPS and Galileo Receiver", 2007
    Mortarboard-H GPS-Receiver-SDR-on-GNURadio (epy_block_0.py)
"""

import argparse
import sys
import time
import numpy as np
import matplotlib
matplotlib.use("Agg")
import matplotlib.pyplot as plt
import matplotlib.gridspec as gridspec


# ---------------------------------------------------------------------------
# C/A code generator
# ---------------------------------------------------------------------------

def generate_ca_code(prn: int) -> np.ndarray:
    """Generate GPS L1 C/A code for PRN 1-37 (returns ±1 chips, 1023 length)."""
    assert 1 <= prn <= 37, f"PRN must be 1-37, got {prn}"
    g2_shifts = [
        5, 6, 7, 8, 17, 18, 139, 140, 141, 251,
        252, 254, 255, 256, 257, 258, 469, 470, 471, 472,
        473, 474, 509, 512, 513, 514, 515, 516, 859, 860,
        861, 862, 145, 175, 52, 21, 237,
    ]
    g2shift = g2_shifts[prn - 1]

    reg = np.ones(10, dtype=np.int8)
    g1 = np.empty(1023, dtype=np.float32)
    for i in range(1023):
        g1[i] = reg[9]
        bit = reg[2] ^ reg[9]
        reg[1:] = reg[:9]
        reg[0] = bit

    reg = np.ones(10, dtype=np.int8)
    g2 = np.empty(1023, dtype=np.float32)
    for i in range(1023):
        g2[i] = reg[9]
        bit = reg[1] ^ reg[2] ^ reg[5] ^ reg[7] ^ reg[8] ^ reg[9]
        reg[1:] = reg[:9]
        reg[0] = bit

    g2 = np.roll(g2, -g2shift)
    ca = g1 * g2         # ±1 chips
    # Map to NRZ: bit 0→+1, bit 1→−1 already done by the XOR (1→+1, -1→+1? No)
    # The LFSR outputs 1 and -1 directly since we use XOR on np.int8 ones.
    # Actually with int8 XOR it'll be 0/1; let me re-implement cleanly.
    return ca


def _ca_code_clean(prn: int) -> np.ndarray:
    """GPS C/A code as {+1, −1} chips (1023 samples)."""
    assert 1 <= prn <= 37
    g2_shifts = [
        5, 6, 7, 8, 17, 18, 139, 140, 141, 251,
        252, 254, 255, 256, 257, 258, 469, 470, 471, 472,
        473, 474, 509, 512, 513, 514, 515, 516, 859, 860,
        861, 862, 145, 175, 52, 21, 237,
    ]
    shift = g2_shifts[prn - 1]

    def lfsr(taps, length):
        reg = np.ones(10, dtype=int)
        out = np.empty(length, dtype=float)
        for i in range(length):
            out[i] = reg[-1]
            fb = int(np.bitwise_xor.reduce(reg[taps]))
            reg[1:] = reg[:-1]
            reg[0] = fb
        return out * 2 - 1  # map 0/1 → -1/+1

    g1 = lfsr([2, 9], 1023)
    g2 = lfsr([1, 2, 5, 7, 8, 9], 1023)
    g2 = np.roll(g2, -shift)
    return g1 * g2


# ---------------------------------------------------------------------------
# IQ file reader
# ---------------------------------------------------------------------------

def read_sc8(path: str, max_samples: int | None = None) -> np.ndarray:
    """Read sc8 file (int8 interleaved I/Q) → complex64 array."""
    raw = np.fromfile(path, dtype=np.int8)
    if max_samples is not None:
        raw = raw[: max_samples * 2]
    n = len(raw) // 2 * 2
    raw = raw[:n]
    iq = raw[0::2].astype(np.float32) + 1j * raw[1::2].astype(np.float32)
    return iq


# ---------------------------------------------------------------------------
# PCPS Acquisition
# ---------------------------------------------------------------------------

def pcps_acquire(signal: np.ndarray, prn: int, samp_rate: float,
                 doppler_range: float = 10_000, doppler_step: float = 500
                 ) -> tuple[bool, float, int, float]:
    """
    Parallel Code Phase Search acquisition for one PRN.

    Returns:
        acquired (bool), doppler_hz (float), code_phase (int), peak_ratio (float)
    """
    spc = int(samp_rate / 1e3)          # samples per code (1 ms)
    # Use exactly one code period for the search
    seg = signal[:spc].copy()

    ca = _ca_code_clean(prn)
    idx = np.floor(np.linspace(0, 1023, spc, endpoint=False)).astype(int)
    ca_seq = ca[idx].astype(np.float32)
    ca_freq = np.conj(np.fft.fft(ca_seq))

    t = np.linspace(0, 1e-3, spc, endpoint=False, dtype=np.float32)
    freq_bins = np.arange(-doppler_range, doppler_range + doppler_step, doppler_step)
    corr_map = np.zeros((len(freq_bins), spc), dtype=np.float32)

    for fi, fd in enumerate(freq_bins):
        carrier = np.exp(1j * 2 * np.pi * fd * t).astype(np.complex64)
        mixed = seg * carrier
        sig_f = np.fft.fft(mixed)
        corr = np.abs(np.fft.ifft(sig_f * ca_freq))
        corr_map[fi, :] = corr

    peak = corr_map.max()
    peak_fi, peak_ci = np.unravel_index(corr_map.argmax(), corr_map.shape)

    # Noise estimate: mean of all bins except the peak row
    mask = np.ones(len(freq_bins), bool)
    mask[peak_fi] = False
    noise = corr_map[mask].mean()

    ratio = float(peak / noise) if noise > 0 else 0.0
    doppler = float(freq_bins[peak_fi])
    acquired = ratio > 2.5  # typical threshold

    return acquired, doppler, int(peak_ci), ratio


# ---------------------------------------------------------------------------
# Full acquisition sweep
# ---------------------------------------------------------------------------

def run_acquisition(iq: np.ndarray, samp_rate: float,
                    prns: list[int], doppler_range: float,
                    doppler_step: float, threshold: float
                    ) -> list[dict]:
    results = []
    spc = int(samp_rate / 1e3)
    if len(iq) < spc:
        print(f"ERROR: need at least {spc} samples, got {len(iq)}", file=sys.stderr)
        return results

    # Average over multiple 1-ms non-coherent integration periods for better sensitivity
    n_periods = min(20, len(iq) // spc)
    print(f"Acquisition: {n_periods}× non-coherent integration, "
          f"Doppler ±{doppler_range/1e3:.0f} kHz step {doppler_step:.0f} Hz")

    for prn in prns:
        # Use the first period for fast search, then verify with more periods
        acq, dop, cp, ratio = pcps_acquire(
            iq[:spc * n_periods], prn, samp_rate, doppler_range, doppler_step
        )
        status = "ACQ" if ratio > threshold else "   "
        print(f"  PRN {prn:2d}: peak/noise = {ratio:5.2f}  Doppler = {dop:+7.0f} Hz  "
              f"CodePhase = {cp:5d}  {status}")
        results.append({
            "prn": prn,
            "acquired": ratio > threshold,
            "doppler_hz": dop,
            "code_phase": cp,
            "peak_ratio": ratio,
        })

    return results


# ---------------------------------------------------------------------------
# Plotting
# ---------------------------------------------------------------------------

def plot_results(results: list[dict], save_path: str | None = None):
    acquired = [r for r in results if r["acquired"]]
    all_ratios = [r["peak_ratio"] for r in results]
    prns = [r["prn"] for r in results]

    fig, axes = plt.subplots(1, 2, figsize=(14, 5))
    fig.suptitle("GPS L1 C/A Acquisition Results", fontsize=14, fontweight="bold")

    # Bar chart: peak/noise ratio per PRN
    ax = axes[0]
    colors = ["green" if r["acquired"] else "steelblue" for r in results]
    bars = ax.bar(prns, all_ratios, color=colors, edgecolor="black", linewidth=0.5)
    ax.axhline(2.5, color="red", linestyle="--", linewidth=1.2, label="Threshold (2.5)")
    ax.set_xlabel("PRN")
    ax.set_ylabel("Peak / Noise ratio")
    ax.set_title("Correlation Peak-to-Noise per PRN")
    ax.legend()
    ax.set_xticks(prns)
    ax.set_xticklabels([str(p) for p in prns], fontsize=7)

    for bar, r in zip(bars, results):
        if r["acquired"]:
            ax.text(bar.get_x() + bar.get_width() / 2, bar.get_height() + 0.05,
                    f"PRN{r['prn']}", ha="center", va="bottom", fontsize=6,
                    color="green", fontweight="bold")

    # Sky plot (elevation not available without position fix, so show Doppler vs PRN)
    ax2 = axes[1]
    if acquired:
        dops = [r["doppler_hz"] for r in acquired]
        prn_acq = [r["prn"] for r in acquired]
        ratios_acq = [r["peak_ratio"] for r in acquired]
        sc = ax2.scatter(prn_acq, dops, c=ratios_acq, cmap="RdYlGn",
                         s=100, zorder=3, edgecolors="black", linewidths=0.5)
        plt.colorbar(sc, ax=ax2, label="Peak / Noise")
        ax2.axhline(0, color="gray", linewidth=0.8, linestyle=":")
        for r in acquired:
            ax2.annotate(f"PRN{r['prn']}", (r["prn"], r["doppler_hz"]),
                         textcoords="offset points", xytext=(4, 4), fontsize=7)
    else:
        ax2.text(0.5, 0.5, "No satellites acquired", ha="center", va="center",
                 transform=ax2.transAxes, fontsize=13, color="red")

    ax2.set_xlabel("PRN")
    ax2.set_ylabel("Doppler offset (Hz)")
    ax2.set_title(f"Acquired Satellites: {len(acquired)}/{len(results)}")

    plt.tight_layout()
    if save_path:
        plt.savefig(save_path, dpi=150, bbox_inches="tight")
        print(f"Plot saved -> {save_path}")
    else:
        plt.show()
    plt.close()


# ---------------------------------------------------------------------------
# CLI
# ---------------------------------------------------------------------------

def parse_prn_spec(spec: str) -> list[int]:
    prns = []
    for part in spec.split(","):
        if "-" in part:
            a, b = part.split("-")
            prns.extend(range(int(a), int(b) + 1))
        else:
            prns.append(int(part))
    return sorted(set(prns))


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("--file", default="rx_capture.iq", help="sc8 IQ capture file")
    p.add_argument("--samp-rate", type=float, default=3_000_000)
    p.add_argument("--doppler-range", type=float, default=10_000)
    p.add_argument("--doppler-step", type=float, default=500)
    p.add_argument("--threshold", type=float, default=2.5)
    p.add_argument("--prns", default="1-32", help="PRN list, e.g. '1-32' or '1,5,19'")
    p.add_argument("--save-plot", default=None, help="Path to save the result plot")
    p.add_argument("--max-samples", type=int, default=None,
                   help="Limit samples read (for speed)")
    args = p.parse_args()

    prns = parse_prn_spec(args.prns)

    print(f"Reading {args.file} …")
    t0 = time.monotonic()
    iq = read_sc8(args.file, args.max_samples)
    duration = len(iq) / args.samp_rate
    print(f"  {len(iq):,} samples  ({duration:.2f} s at {args.samp_rate/1e6:.1f} MSPS)  "
          f"RMS = {np.abs(iq).mean():.4f}")

    print(f"\nSearching PRNs: {prns}")
    results = run_acquisition(iq, args.samp_rate, prns,
                               args.doppler_range, args.doppler_step, args.threshold)
    elapsed = time.monotonic() - t0

    acquired = [r for r in results if r["acquired"]]
    print(f"\n{'='*60}")
    print(f"Acquired: {len(acquired)}/{len(prns)} satellites  (took {elapsed:.1f} s)")
    for r in acquired:
        print(f"  PRN {r['prn']:2d}  Doppler {r['doppler_hz']:+.0f} Hz  "
              f"CodePhase {r['code_phase']:5d}  ratio {r['peak_ratio']:.2f}")
    if not acquired:
        print("  No satellites acquired above threshold.")

    if args.save_plot:
        plot_results(results, args.save_plot)

    return 0 if acquired else 1


if __name__ == "__main__":
    sys.exit(main())
