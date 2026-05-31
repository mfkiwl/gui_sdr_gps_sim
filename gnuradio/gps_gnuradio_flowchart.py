#!/usr/bin/env python3
"""
Generate a GNU Radio GPS L1 C/A decoder flowchart diagram (PNG).

This produces a publication-quality block-diagram showing the complete
signal processing chain from HackRF source to position fix.

Usage:
    python gps_gnuradio_flowchart.py [--output flowchart.png]
"""

import argparse
import matplotlib
matplotlib.use("Agg")
import matplotlib.pyplot as plt
import matplotlib.patches as mpatches
from matplotlib.patches import FancyArrowPatch, FancyBboxPatch
import numpy as np


# ---------------------------------------------------------------------------
# Drawing helpers
# ---------------------------------------------------------------------------

def draw_block(ax, x, y, w, h, label, sublabel="", color="#4A90D9",
               text_color="white", fontsize=9):
    box = FancyBboxPatch((x - w / 2, y - h / 2), w, h,
                         boxstyle="round,pad=0.03",
                         facecolor=color, edgecolor="#2c2c2c", linewidth=1.2,
                         zorder=3)
    ax.add_patch(box)
    ax.text(x, y + (0.04 if sublabel else 0), label, ha="center", va="center",
            fontsize=fontsize, fontweight="bold", color=text_color, zorder=4,
            wrap=True)
    if sublabel:
        ax.text(x, y - 0.13, sublabel, ha="center", va="center",
                fontsize=7, color=text_color, alpha=0.85, zorder=4)


def arrow(ax, x0, y0, x1, y1, label="", color="#333333"):
    ax.annotate("", xy=(x1, y1), xytext=(x0, y0),
                arrowprops=dict(arrowstyle="-|>", color=color,
                                lw=1.4, mutation_scale=14),
                zorder=2)
    if label:
        mx, my = (x0 + x1) / 2, (y0 + y1) / 2
        ax.text(mx + 0.03, my + 0.04, label, fontsize=7, color="#555", zorder=5)


def section_bg(ax, x, y, w, h, color, label):
    box = FancyBboxPatch((x, y), w, h,
                         boxstyle="round,pad=0.02",
                         facecolor=color, edgecolor="#aaa", linewidth=0.8,
                         alpha=0.25, zorder=1)
    ax.add_patch(box)
    ax.text(x + 0.07, y + h - 0.07, label, fontsize=8, color="#555",
            fontweight="bold", va="top", zorder=2)


# ---------------------------------------------------------------------------
# Main diagram
# ---------------------------------------------------------------------------

def build_flowchart(output_path: str):
    fig, ax = plt.subplots(figsize=(18, 10))
    ax.set_xlim(0, 18)
    ax.set_ylim(0, 10)
    ax.axis("off")
    fig.patch.set_facecolor("#f8f9fa")
    ax.set_facecolor("#f8f9fa")

    ax.set_title("GNU Radio GPS L1 C/A Receiver — Full Flowchart",
                 fontsize=15, fontweight="bold", pad=14)

    # ── Section backgrounds ──────────────────────────────────────────────────
    section_bg(ax, 0.2, 6.5, 3.2, 3.2, "#b3d9ff", "RF FRONT-END")
    section_bg(ax, 3.6, 6.5, 3.0, 3.2, "#b3ffcc", "SIGNAL CONDITIONING")
    section_bg(ax, 6.8, 6.5, 3.5, 3.2, "#ffe0b3", "ACQUISITION (per PRN)")
    section_bg(ax, 10.5, 6.5, 3.5, 3.2, "#e8b3ff", "TRACKING LOOPS")
    section_bg(ax, 14.2, 6.5, 3.5, 3.2, "#ffb3b3", "NAV DECODE & FIX")
    section_bg(ax, 0.2, 0.4, 17.5, 5.8, "#e8e8e8", "DETAILED SIGNAL PATH (main data flow)")

    # ── Top row: high-level blocks ───────────────────────────────────────────
    # RF source
    draw_block(ax, 1.8, 8.1, 2.6, 0.9, "HackRF One", "osmosdr_source\n1575.42 MHz / 3 MSPS", "#1565C0")
    # Low-pass filter
    draw_block(ax, 5.1, 8.1, 2.4, 0.9, "Low-Pass Filter", "cutoff 2.046 MHz\nGaussian taps", "#2E7D32")
    # Acquisition
    draw_block(ax, 8.05, 8.1, 3.0, 0.9, "PCPS Acquisition", "FFT-based code+freq\n±10 kHz, 500 Hz step", "#E65100")
    # Tracking
    draw_block(ax, 12.25, 8.1, 3.0, 0.9, "DLL + PLL\nTracking", "Early/Prompt/Late\ncarrier + code loop", "#6A1B9A")
    # Nav decode
    draw_block(ax, 15.95, 8.1, 3.0, 0.9, "Nav Message\nDecoder", "50 bps BPSK\nSubframes 1-5", "#B71C1C")

    # Arrows top row
    arrow(ax, 3.1, 8.1, 3.9, 8.1, "complex64\n3 MSPS")
    arrow(ax, 6.3, 8.1, 6.55, 8.1, "filtered IQ")
    arrow(ax, 9.55, 8.1, 10.75, 8.1, "acq result\n(PRN, freq, phase)")
    arrow(ax, 13.75, 8.1, 14.45, 8.1, "nav bits")

    # ── Bottom row: detailed signal path ─────────────────────────────────────
    BLK = "#4A90D9"
    GRN = "#388E3C"
    ORG = "#F57C00"
    PRP = "#7B1FA2"
    RED = "#C62828"
    GRY = "#546E7A"

    # Row 1 of detail (y=4.9)
    draw_block(ax, 1.6,  4.9, 2.6, 0.75, "File Source /\nHackRF Source", "gr.file_source or\nosmosdr.source", BLK)
    draw_block(ax, 4.5,  4.9, 2.2, 0.75, "Stream to\nVector", "vectorLen =\nsamp_rate / 1e3", GRY)
    draw_block(ax, 7.1,  4.9, 2.4, 0.75, "C/A Code Gen", "PRN 1–32\nG1 × G2 LFSR", GRN)
    draw_block(ax, 9.9,  4.9, 2.4, 0.75, "FFT / IFFT\nCorrelator", "PCPS: ifft(fft(s)·\nconj(fft(ca)))", ORG)
    draw_block(ax, 12.8, 4.9, 2.4, 0.75, "Peak Detector", "max(|corr_map|)\nvs noise floor", ORG)
    draw_block(ax, 15.6, 4.9, 2.0, 0.75, "Acq\nDecision", "ratio > 2.5\n→ acquired", ORG)

    arrow(ax, 2.9,  4.9, 3.4,  4.9, "sc8 IQ")
    arrow(ax, 5.6,  4.9, 5.9,  4.9, "vectors\n1 ms")
    arrow(ax, 7.1,  4.55, 7.7, 4.55)
    arrow(ax, 8.3,  4.9, 8.7,  4.9, "code FFT*")
    arrow(ax, 11.1, 4.9, 11.6, 4.9, "corr map")
    arrow(ax, 14.0, 4.9, 14.6, 4.9, "peak")

    # Doppler search annotation
    ax.annotate("Doppler\nsearch loop\n±10 kHz",
                xy=(9.9, 4.55), xytext=(9.9, 3.55),
                arrowprops=dict(arrowstyle="->", color="#999", lw=1),
                ha="center", fontsize=7, color="#666",
                bbox=dict(boxstyle="round,pad=0.2", fc="#fff3e0", ec="#ffb300", lw=0.8))

    # Row 2 of detail (y=2.9) — tracking chain
    draw_block(ax, 1.6,  2.9, 2.6, 0.75, "Carrier NCO", "f_carr = f_L1 +\nDoppler estimate", PRP)
    draw_block(ax, 4.5,  2.9, 2.2, 0.75, "Carrier\nWipe-off", "sig × exp(−j2πft)", PRP)
    draw_block(ax, 7.1,  2.9, 2.4, 0.75, "Code NCO", "f_code = chip_rate\n+ carr/CARR2CODE", PRP)
    draw_block(ax, 9.9,  2.9, 2.4, 0.75, "E/P/L\nCorrelators", "Early, Prompt, Late\n0.5 chip spacing", PRP)
    draw_block(ax, 12.8, 2.9, 2.4, 0.75, "DLL\nDiscrim.", "⟨E⟩−⟨L⟩\n÷(⟨E⟩+⟨L⟩)", PRP)
    draw_block(ax, 15.6, 2.9, 2.0, 0.75, "PLL\nDiscrim.", "atan2(Q,I)\nFLL assist", PRP)

    arrow(ax, 2.9,  2.9, 3.4,  2.9)
    arrow(ax, 5.6,  2.9, 5.9,  2.9)
    arrow(ax, 8.3,  2.9, 8.7,  2.9, "E/P/L\ncodes")
    arrow(ax, 11.1, 2.9, 11.6, 2.9, "E/P/L\naccums")
    arrow(ax, 14.0, 2.9, 14.6, 2.9)

    # DLL→Code NCO feedback
    ax.annotate("", xy=(7.1, 2.55), xytext=(12.8, 2.55),
                arrowprops=dict(arrowstyle="<-", color="#6A1B9A", lw=1.2,
                                connectionstyle="arc3,rad=0.0"))
    ax.text(9.9, 2.37, "DLL feedback (code freq correction)", ha="center",
            fontsize=7, color="#6A1B9A")

    # PLL→Carrier NCO feedback
    ax.annotate("", xy=(1.6, 2.55), xytext=(15.6, 2.55),
                arrowprops=dict(arrowstyle="<-", color="#6A1B9A", lw=1.2,
                                connectionstyle="arc3,rad=0.0"))
    ax.text(8.6, 2.18, "PLL feedback (carrier freq + phase correction)",
            ha="center", fontsize=7, color="#6A1B9A")

    # Row 3 of detail (y=1.1) — nav decode + position
    draw_block(ax, 3.8,  1.1, 2.6, 0.75, "Bit Sync\n(20 ms)", "Prompt accumulate\nover 20 code periods", RED)
    draw_block(ax, 7.1,  1.1, 2.4, 0.75, "Nav Message\nParity (BCH)", "D29*/D30* Hamming\nparity check", RED)
    draw_block(ax, 10.5, 1.1, 2.6, 0.75, "Subframe\nDecoder", "SF1: clock  SF2/3: eph\nTOW, week number", RED)
    draw_block(ax, 14.0, 1.1, 3.4, 0.75, "Least-Squares\nPosition Fix", "≥4 SVs: WGS-84\nlat/lon/alt + UTC", "#B71C1C")

    arrow(ax, 5.1,  1.1, 5.9,  1.1, "nav bits\n50 bps")
    arrow(ax, 8.3,  1.1, 9.2,  1.1, "verified\nbits")
    arrow(ax, 11.8, 1.1, 12.3, 1.1, "ephemeris\n+ TOW")
    arrow(ax, 15.6, 8.1, 17.1, 8.1)
    ax.text(17.25, 8.1, "✓", fontsize=18, color="green", ha="center", va="center",
            fontweight="bold", zorder=5)

    # Connect tracking row to nav decode
    arrow(ax, 15.6, 2.55, 16.5, 2.55)
    ax.annotate("", xy=(3.8, 1.5), xytext=(15.6, 2.55),
                arrowprops=dict(arrowstyle="-|>", color="#C62828", lw=1.2,
                                connectionstyle="angle,angleA=0,angleB=90"))
    ax.text(10.5, 2.1, "nav bits → position pipeline", ha="center",
            fontsize=7, color="#C62828")

    # ── Legend ───────────────────────────────────────────────────────────────
    legend_items = [
        mpatches.Patch(color=BLK, label="RF / Source"),
        mpatches.Patch(color=GRN, label="Code Generation"),
        mpatches.Patch(color=ORG, label="Acquisition"),
        mpatches.Patch(color=PRP, label="Tracking (DLL/PLL)"),
        mpatches.Patch(color=RED, label="Navigation Decode"),
        mpatches.Patch(color=GRY, label="GNU Radio infrastructure"),
    ]
    ax.legend(handles=legend_items, loc="lower right", fontsize=8,
              framealpha=0.9, title="Block category", title_fontsize=8)

    # ── Parameter table ───────────────────────────────────────────────────────
    params = [
        ["Parameter", "Value"],
        ["Center frequency", "1 575.420 MHz (GPS L1)"],
        ["Sample rate", "3.000 MSPS (sc8)"],
        ["Samples / code period", "3 000"],
        ["Doppler search range", "±10 kHz in 500 Hz steps"],
        ["Coherent integration", "1 ms (1 code period)"],
        ["Acquisition threshold", "peak/noise > 2.5"],
        ["Tracking: DLL spacing", "0.5 chip"],
        ["Tracking: PLL BW", "18 Hz"],
        ["Nav bit rate", "50 bps (BPSK)"],
        ["Min SVs for position fix", "4"],
    ]
    table = ax.table(cellText=params[1:], colLabels=params[0],
                     loc="lower left", cellLoc="left",
                     bbox=[0.0, 0.0, 0.27, 0.52])
    table.auto_set_font_size(False)
    table.set_fontsize(7.5)
    for (r, c), cell in table.get_celld().items():
        if r == 0:
            cell.set_facecolor("#1565C0")
            cell.set_text_props(color="white", fontweight="bold")
        elif r % 2 == 0:
            cell.set_facecolor("#e3f2fd")
        cell.set_edgecolor("#aaa")

    plt.tight_layout(rect=[0, 0, 1, 0.97])
    plt.savefig(output_path, dpi=150, bbox_inches="tight", facecolor=fig.get_facecolor())
    print(f"Flowchart saved -> {output_path}")
    plt.close()


if __name__ == "__main__":
    p = argparse.ArgumentParser()
    p.add_argument("--output", default="gps_gnuradio_flowchart.png")
    args = p.parse_args()
    build_flowchart(args.output)
