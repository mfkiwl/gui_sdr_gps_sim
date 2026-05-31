# GNU Radio flow graphs for gui_sdr_gps_sim

---

## `gps_network_to_hackrf` — Network stream → Visualize → HackRF TX

**The main flow graph.** Receives the raw IQ stream from the app's network output
(UDP or TCP), shows spectrum/waterfall/time/constellation, and simultaneously
re-transmits via a HackRF One.

### App settings

| App output | Setting | GNU Radio side |
|---|---|---|
| **TCP** (recommended) | Set *TCP port* e.g. `4568` | GR connects as TCP client to `127.0.0.1:4568` |
| **UDP** | Set *UDP address* e.g. `127.0.0.1:4567` | GR listens on UDP port `4567` |

> **TCP is recommended** — no packet loss, no datagram alignment issues.
> The app is the TCP **server** (it waits for a connection). Start GNU Radio
> **after** starting the simulation in the app.

### Signal path

```
[gui_sdr_gps_sim]
      │  8-bit signed i8, interleaved [I0,Q0,I1,Q1,…]
      │  3 MSPS, GPS L1 C/A  1575.42 MHz
      │
      ▼  TCP (continuous stream) or UDP (32 768-byte datagrams)
      │
[network_tcp_source / network_udp_source]   (raw bytes, uint8/char)
      │
[interleaved_char_to_complex  ×1/128]       i8 → complex float  ±1.0
      │
      ├──► [qtgui_freq_sink_c]              Tab 0 — FFT spectrum
      ├──► [qtgui_waterfall_sink_c]         Tab 1 — Waterfall
      ├──► [keep_one_in_n ×100]
      │        └──► [qtgui_time_sink_c]     Tab 2 — Time domain
      ├──► [qtgui_const_sink_c]             Tab 3 — Constellation
      │
      └──► [osmosdr_sink  hackrf=0]         HackRF TX @ 1575.42 MHz / 3 MSPS
```

### Usage

```bash
# TCP mode (default — app TCP port = 4568)
python gps_network_to_hackrf.py

# UDP mode (app UDP address = 127.0.0.1:4567)
python gps_network_to_hackrf.py --mode udp

# Custom ports / gain
python gps_network_to_hackrf.py --mode tcp --tcp-port 4568 --tx-gain 25

# With RF amp (+14 dB, be careful)
python gps_network_to_hackrf.py --amp

# Open in GNU Radio Companion for visual editing
gnuradio-companion gps_network_to_hackrf.grc
```

In the `.grc` file: enable **one** source block only (UDP or TCP — both are wired
but TCP is enabled by default; disable the other).

### TX gain tuning

| Control | Range | Note |
|---|---|---|
| `--tx-gain` / slider | 0–47 dB | TX VGA — primary power control |
| `--amp` | off / on | RF amp +14 dB — use only for long cable runs |

Start at 20 dB and increase if the receiving GPS device doesn't get a fix.

---

## `gps_iq_file_analyzer` — IQ file signal analyser (no hardware needed)

Reads an IQ file written by the app's **IQ File** output mode and displays
the same four analysis panels (FFT, waterfall, time domain, constellation)
without requiring a second HackRF or any radio hardware.

### Step 1 — generate the IQ file

In gui_sdr_gps_sim set **Output = IQ File**, enter a path (e.g. `gps_signal.iq`),
set **Duration** to 30 s, then click **Start**.  The app writes sc8 samples at
real-time pace (~30 s wall clock, ~180 MB file).

### Step 2 — run the analyser

```bash
# Default file name (gps_signal.iq) and sample rate (3 MSPS)
python gps_iq_file_analyzer.py

# Custom file and/or rate
python gps_iq_file_analyzer.py path/to/my_signal.iq --rate 4M
```

The file loops continuously so the displays stay live.

### Signal path

```
[IQ file on disk]
      │  sc8: interleaved signed-8-bit I/Q  [I0,Q0,I1,Q1,…]
      │
[blocks_file_source]      (repeat=True → file loops)
      │
[interleaved_char_to_complex  ×1/128]   i8 → complex float ±1.0
      │
[throttle  @  samp_rate]   paces playback to real time
      │
      ├──► [qtgui_freq_sink_c]          Tab 0 — FFT spectrum
      ├──► [qtgui_waterfall_sink_c]     Tab 1 — Waterfall
      ├──► [keep_one_in_n ×100]
      │        └──► [qtgui_time_sink_c] Tab 2 — Time domain (~30 kHz)
      └──► [qtgui_const_sink_c]         Tab 3 — IQ constellation
```

### What to look for

| View | Good signal | Bad signal |
|---|---|---|
| **FFT Spectrum** | Flat plateau ~2 MHz wide (GPS C/A null-to-null), gentle sidelobes, no DC spike | DC spike only, very narrow peak, or flat noise floor |
| **Waterfall** | Steady horizontal band ~2 MHz wide, time-invariant | No band, or sweeping/variable feature |
| **Time Domain** | Band-limited noise appearance, I and Q of similar amplitude | Clipped flat-tops (saturation), or zero output |
| **Constellation** | Roughly circular cloud, radius ~0.7–1.0 (good 8-bit quantisation) | Tiny dot (too quiet), rectangular blob (clipping), or ring-only (IQ imbalance) |

---

## `gps_l1_analyzer` — HackRF RX spectrum analyzer

Standalone receiver using a **second HackRF in RX mode**. Useful for verifying
the transmitted signal over the air or through a cable+attenuator.

```bash
python gps_l1_analyzer.py
gnuradio-companion gps_l1_analyzer.grc
```

---

## Requirements

```bash
# Ubuntu/Debian
sudo apt install gnuradio gr-osmosdr python3-pyqt5

# Arch
sudo pacman -S gnuradio gr-osmosdr python-pyqt5

# Windows: install GNU Radio via the official installer (includes gr-osmosdr)
# HackRF: replace driver with WinUSB via Zadig  https://zadig.akeo.ie/
```

GNU Radio 3.10+ · Python 3 · PyQt5 · sip

---

## Wire format reference

| Field | Value |
|---|---|
| IQ encoding | `i8` (signed 8-bit), interleaved `[I, Q, I, Q, …]` |
| On-wire type | `u8` (same bits, reinterpreted) |
| Sample rate | 3 000 000 sps |
| Center freq | 1 575 420 000 Hz (GPS L1 C/A) |
| UDP datagram | 32 768 bytes = 16 384 complex pairs |
| TCP | continuous byte stream, no framing |
| GR conversion | `interleaved_char_to_complex(scale=1/128)` → complex float ±1.0 |
