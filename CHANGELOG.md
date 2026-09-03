# Changelog

All notable changes to this project are documented here.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.2.0] — 2026-09-03

**Receivers can now get a position fix.** Every release before this one produced
a signal that a GPS receiver would acquire and track and then quietly discard.
Seven separate defects in the navigation data layer and the pseudorange model
each prevented a fix on their own, and none of them was visible to a spectrum
plot or an acquisition search — which is why they survived so long.

Verified end to end against a real phone and against a second HackRF: the phone
now locks and reports a position roughly 10 m from the simulated point.

### Fixed — navigation message

- **SV health was broadcast as "bad" for every satellite.** The L2-code flag,
  URA and SV health sat at the wrong bit offsets in subframe 1 word 3, putting
  the L2 flag on the health field's MSB. IS-GPS-200 Table 20-VII reads that as
  *"some or all navigation data are bad"*, so receivers dropped every satellite
  after tracking it perfectly.
- **Subframe 1's clock block was three words early.** TGD, IODC, `toc`, `af0`,
  `af1` and `af2` were written into words 4–7 instead of 7–10, and words 7–9
  were then overwritten with filler. Receivers read their clock corrections out
  of the alternating dummy pattern.
- **The split 32-bit orbital elements carried the wrong byte.** M0, e, √A, Ω0,
  i0 and ω are each split into an 8-bit MSB field and a 24-bit LSB field; the
  MSB field was packed from the *low* byte of the scaled integer, placing the
  decoded orbit thousands of kilometres away.
- **The navigation frame ran 30 s behind the signal.** A channel starting inside
  the final subframe of a frame reached its first wrap before the next frame had
  been prepared, and the wrap silently re-transmitted the old frame. Every frame
  afterwards stayed exactly one frame behind.
- **Subframe 4 page 18** (ionosphere and UTC) had its α/β coefficients shifted a
  word, A₀/A₁ swapped and split at the wrong bit, `tot` unscaled, and the
  wrong page SV ID.

### Fixed — time, ephemeris and pseudoranges

- **`StartTime::Now` did not mean now.** It computed the current GPS time and
  then discarded it for the RINEX file's first epoch, stamping a run started at
  22:00 with a timestamp from midnight. Receivers holding network time or cached
  assistance data reject that.
- **Ephemeris selection could pick a one-satellite set.** Broadcast files group
  records by epoch and satellites do not upload together, so a day's file
  routinely ends with a set holding a single satellite. Selection now merges
  per satellite — each contributes its own most recent record.
- **The analytic range rate was wrong by up to 5.5 m/s per satellite.** `dν/dt`
  was written through sin ν and cos ν, an expression that collapses
  algebraically to plain `dE/dt` and cancels the eccentricity factor entirely.
- **The code phase was integrated, never re-anchored.** It is now reset from the
  current pseudorange at the top of every 100 ms step, and the Doppler is
  derived as a backward difference of that pseudorange — which also captures the
  *receiver's* own motion, something the satellite-velocity projection cannot
  express. Routes now follow their path instead of drifting off it.

Measured on a 75 s static capture, before → after: position scatter
28.7 / 3.2 / 38.4 m East/North/Up → **0.7 / 0.4 / 0.8 m**; horizontal drift
98.5 m over 48 s → **1.3 m over 31 s**. On a 15 m/s route: fitted speed
15.03 m/s against 15.00 true, 0.26 m along-track misfit.

### Fixed — transmission

- **Generation was paced by a wall clock on top of FIFO back-pressure**, which
  can only ever run slower than the hardware consumes. Measured at 0.35 % slow —
  enough to drain the FIFO and then underrun the HackRF on every buffer. Pacing
  is now left to back-pressure, as in the C reference; a 45 s scenario generates
  in about 6 s instead of 45.

### Added

- `gnuradio/gps_nav_decode.py` — a complete software receiver over an IQ
  capture: acquire, track, decode the navigation message, form pseudoranges and
  solve for position. Acquisition cannot tell you whether a receiver will fix;
  this can. See [Verifying a capture](README.md#verifying-a-capture).
- A warning, shown in the app and not only in the log, when the ephemeris is
  more than two hours past its reference time. IS-GPS-200 gives a 4-hour curve
  fit, so an ephemeris is only valid for `toe ± 2 h` and a strict receiver
  discards it beyond that.

### Changed

- Default TX VGA gain is now **47 dB** rather than 20 dB. 20 dB is well below
  what a receiver needs over the air. Existing installations keep their saved
  value; change it once in *Settings*.

## [0.1.0] — 2026-03-21

First public release. Multi-constellation signal generation (GPS L1 C/A,
BeiDou B1C, Galileo E1-B), route creation via OpenRouteService, GeoJSON import
or map drawing, waypoint management, and HackRF / file / UDP / TCP output.

[0.2.0]: https://github.com/okiedocus/gui_sdr_gps_sim/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/okiedocus/gui_sdr_gps_sim/releases/tag/v0.1.0
