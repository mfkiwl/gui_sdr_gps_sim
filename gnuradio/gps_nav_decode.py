#!/usr/bin/env python3
"""Software GPS receiver over an sc8 IQ capture: acquire, track, decode the
navigation message, form pseudoranges and solve for position.

Acquisition proves the spreading code and the RF are right. It says nothing
about whether a receiver can *fix*, because everything that decides position
lives in the 50 bps data layer and in the code-phase alignment. This tool runs
that whole path and reports the answer as a position, which is the only check
that covers it end to end.

Usage
-----
    python gnuradio/gps_nav_decode.py --file gps_signal.iq
    python gnuradio/gps_nav_decode.py --file gps_signal.iq --truth 52.3791 4.9003 5.0

Needs at least ~35 s of capture: a receiver must see subframes 1-3 of one
30-second frame, and it only starts collecting at the first subframe boundary.

Reading the output
------------------
`subframe t_tx` must equal the GPS time the simulator reported at start-up. If
every satellite is short by a multiple of 30 s, the navigation frame is running
behind the signal and no receiver will fix, however clean the spectrum looks.

With no ionosphere/troposphere correction applied here, residuals of 20-60 m
against a known truth position are expected and correct: that is the delay the
simulator adds. Residuals in the kilometres mean the pseudoranges and the
broadcast ephemeris disagree.
"""

import argparse
import numpy as np

CHIP_RATE = 1_023_000.0
CA_LEN = 1023
C = 299_792_458.0
MU = 3.986005e14
OMEGA_E = 7.2921151467e-5
F_REL = -4.442807633e-10

# G2 chip delays for PRN 1-32, IS-GPS-200 Table 3-Ia.
G2_DELAY = [5, 6, 7, 8, 17, 18, 139, 140, 141, 251, 252, 254, 255, 256, 257,
            258, 469, 470, 471, 472, 473, 474, 509, 512, 513, 514, 515, 516,
            859, 860, 861, 862]

# IS-GPS-200 Table 20-XIV parity masks (D25-D30) and which carry bit each uses.
PARITY_MASKS = [0x3B1F3480, 0x1D8F9A40, 0x2EC7CD00,
                0x1763E680, 0x2BB1F340, 0x0B7A89C0]
CARRY_D29 = [True, False, True, False, False, True]


# ── Spreading code ───────────────────────────────────────────────────────────

def ca_code(prn):
    """Bipolar 1023-chip C/A code for `prn`."""
    g1r = np.ones(10, dtype=np.int8)
    g2r = np.ones(10, dtype=np.int8)
    g1 = np.zeros(CA_LEN, dtype=np.int8)
    g2 = np.zeros(CA_LEN, dtype=np.int8)
    for i in range(CA_LEN):
        g1[i], g2[i] = g1r[9], g2r[9]
        fb1 = g1r[2] ^ g1r[9]
        g1r[1:] = g1r[:-1]
        g1r[0] = fb1
        fb2 = g2r[1] ^ g2r[2] ^ g2r[5] ^ g2r[7] ^ g2r[8] ^ g2r[9]
        g2r[1:] = g2r[:-1]
        g2r[0] = fb2
    idx = (np.arange(CA_LEN) + CA_LEN - G2_DELAY[prn - 1]) % CA_LEN
    return (g1 ^ g2[idx]).astype(np.float64) * 2.0 - 1.0


def to_complex(raw, start, n):
    seg = raw[2 * start: 2 * (start + n)].astype(np.float32)
    return seg[0::2] + 1j * seg[1::2]


# ── Acquisition and tracking ─────────────────────────────────────────────────

def acquire(raw, prn, fs, n_ms=10, dopp_max=6000, dopp_step=250):
    """Parallel code-phase search. Returns (peak/second-peak, doppler, phase)."""
    spc = int(fs / 1000.0)
    code = ca_code(prn)
    code_s = code[(np.arange(spc) * CHIP_RATE / fs).astype(np.int64) % CA_LEN]
    cfft = np.conj(np.fft.fft(code_s))
    t = np.arange(spc) / fs
    best, surf = (0.0, 0.0, 0), None
    for dopp in np.arange(-dopp_max, dopp_max + 1, dopp_step):
        carr = np.exp(-2j * np.pi * dopp * t)
        acc = np.zeros(spc)
        for m in range(n_ms):
            x = to_complex(raw, m * spc, spc) * carr
            acc += np.abs(np.fft.ifft(np.fft.fft(x) * cfft)) ** 2
        if acc.max() > best[0]:
            best, surf = (acc.max(), dopp, int(acc.argmax())), acc
    pk, dopp, phase = best
    guard = int(fs / CHIP_RATE) + 1
    masked = surf.copy()
    masked[max(0, phase - guard): phase + guard + 1] = 0
    return pk / masked.max(), dopp, phase


def track(raw, prn, fs, dopp0, phase0, n_ms):
    """Costas PLL + normalised early-late DLL.

    Returns prompt I, prompt Q, and the receive sample position at which each
    1 ms code epoch occurs -- that position is what turns a decoded TOW into a
    pseudorange, so it is carried out rather than recomputed from a nominal rate.

    The position is fractional on purpose. Rounding it to the nearest sample
    costs +/-0.5 sample, and at 3 MSPS one sample is 100 m of range: a whole-sample
    index alone puts ~40 m of quantisation noise straight into the pseudorange,
    which is more than the errors worth measuring in a simulated signal.
    """
    code = ca_code(prn)
    carr_f, carr_p = float(dopp0), 0.0
    code_f, code_p = CHIP_RATE + carr_f / 1540.0, 0.0
    sample = int(phase0)
    T = 1e-3
    pll_wn, dll_wn = 20.0 / 0.53, 1.0 / 0.53
    pll_a, pll_b = 1.414 * pll_wn, pll_wn ** 2
    dll_a, dll_b = 1.414 * dll_wn, dll_wn ** 2
    pll_i = dll_i = 0.0

    oi, oq = np.zeros(n_ms), np.zeros(n_ms)
    idx = np.zeros(n_ms + 1)
    total = raw.size // 2
    k = 0
    for k in range(n_ms):
        # Sample position of this code epoch: back off the fraction of a chip
        # already accumulated at `sample`, converted to samples.
        idx[k] = sample - code_p * fs / code_f
        n = int(round((CA_LEN - code_p) * fs / code_f))
        if sample + n >= total:
            break
        x = to_complex(raw, sample, n) * np.exp(
            -1j * (2 * np.pi * carr_f * np.arange(n) / fs + carr_p))
        cp = code_p + np.arange(n) * code_f / fs
        E = np.dot(x, code[np.floor(cp - 0.5).astype(np.int64) % CA_LEN])
        P = np.dot(x, code[np.floor(cp).astype(np.int64) % CA_LEN])
        L = np.dot(x, code[np.floor(cp + 0.5).astype(np.int64) % CA_LEN])
        oi[k], oq[k] = P.real, P.imag

        err = np.arctan(P.imag / P.real) / (2 * np.pi) if P.real else 0.0
        pll_i += pll_b * err * T
        carr_f += pll_a * err + pll_i * T
        carr_p = (carr_p + 2 * np.pi * carr_f * n / fs) % (2 * np.pi)

        ae, al = abs(E), abs(L)
        derr = (ae - al) / (ae + al) if (ae + al) else 0.0
        dll_i += dll_b * derr * T
        code_f = CHIP_RATE + carr_f / 1540.0 - (dll_a * derr + dll_i * T)
        code_p = code_p + n * code_f / fs - CA_LEN
        sample += n
    idx[k + 1] = sample - code_p * fs / code_f
    return oi[:k], oq[:k], idx[:k + 1]


# ── Navigation message ───────────────────────────────────────────────────────

def parity_ok(word, prev):
    d29, d30 = (prev >> 1) & 1, prev & 1
    d = word & 0x3FFFFFC0
    if d30:
        d ^= 0x3FFFFFC0
    exp = 0
    for i, (m, c29) in enumerate(zip(PARITY_MASKS, CARRY_D29)):
        exp |= (((d29 if c29 else d30) + bin(m & d).count("1")) % 2) << (5 - i)
    return exp == (word & 0x3F)


def data24(word, prev):
    d = word & 0x3FFFFFC0
    if prev & 1:
        d ^= 0x3FFFFFC0
    return d >> 6


def fld(d, first, n):
    """Unsigned field of `n` bits at IS-GPS-200 data bit `first` (1-based)."""
    return (d >> (25 - first - n)) & ((1 << n) - 1)


def sfld(d, first, n):
    v = fld(d, first, n)
    return v - (1 << n) if v & (1 << (n - 1)) else v


def s32(hi, lo):
    v = (hi << 24) | lo
    return v - (1 << 32) if v & (1 << 31) else v


def word_at(b, i):
    return int("".join(map(str, b[i:i + 30])), 2)


def frame_sync(bits):
    """First bit index whose ten following words all pass parity, and polarity."""
    for pol in (0, 1):
        b = bits ^ pol
        for i in range(len(b) - 300):
            if int("".join(map(str, b[i:i + 8])), 2) not in (0x8B, 0x74):
                continue
            ok, prev = True, 0
            for k in range(10):
                word = word_at(b, i + 30 * k)
                if not parity_ok(word, prev):
                    ok = False
                    break
                prev = word
            if ok:
                return pol, i
    return None, None


def decode_sv(bits, start):
    """Collect subframes 1-3. Returns (ephemeris, anchor bit index, anchor TOW)."""
    subs, anchor = {}, None
    prev, i = 0, start
    while i + 300 <= len(bits):
        words, ok, p = [], True, prev
        for k in range(10):
            word = word_at(bits, i + 30 * k)
            if not parity_ok(word, p):
                ok = False
            words.append(word)
            p = word
        dd = [data24(words[k], words[k - 1] if k else prev) for k in range(10)]
        if anchor is None and ok:
            anchor = (i, fld(dd[1], 1, 17))
        if ok and (sfid := fld(dd[1], 20, 3)) in (1, 2, 3):
            subs[sfid] = dd
        prev = words[9]
        i += 300
    if not {1, 2, 3} <= set(subs):
        return None, None, None
    p2 = lambda e: 2.0 ** e
    d1, d2, d3 = subs[1], subs[2], subs[3]
    eph = dict(
        week=fld(d1[2], 1, 10), health=fld(d1[2], 17, 6),
        toc=fld(d1[7], 9, 16) * 16.0, tgd=sfld(d1[6], 17, 8) * p2(-31),
        af0=sfld(d1[9], 1, 22) * p2(-31), af1=sfld(d1[8], 9, 16) * p2(-43),
        af2=sfld(d1[8], 1, 8) * p2(-55),
        iodc=(fld(d1[2], 23, 2) << 8) | fld(d1[7], 1, 8),
        iode=fld(d2[2], 1, 8), crs=sfld(d2[2], 9, 16) * p2(-5),
        dn=sfld(d2[3], 1, 16) * p2(-43) * np.pi,
        m0=s32(fld(d2[3], 17, 8), fld(d2[4], 1, 24)) * p2(-31) * np.pi,
        cuc=sfld(d2[5], 1, 16) * p2(-29),
        ecc=((fld(d2[5], 17, 8) << 24) | fld(d2[6], 1, 24)) * p2(-33),
        cus=sfld(d2[7], 1, 16) * p2(-29),
        sqrta=((fld(d2[7], 17, 8) << 24) | fld(d2[8], 1, 24)) * p2(-19),
        toe=fld(d2[9], 1, 16) * 16.0, cic=sfld(d3[2], 1, 16) * p2(-29),
        omg0=s32(fld(d3[2], 17, 8), fld(d3[3], 1, 24)) * p2(-31) * np.pi,
        cis=sfld(d3[4], 1, 16) * p2(-29),
        inc0=s32(fld(d3[4], 17, 8), fld(d3[5], 1, 24)) * p2(-31) * np.pi,
        crc=sfld(d3[6], 1, 16) * p2(-5),
        aop=s32(fld(d3[6], 17, 8), fld(d3[7], 1, 24)) * p2(-31) * np.pi,
        omgdot=sfld(d3[8], 1, 24) * p2(-43) * np.pi,
        idot=sfld(d3[9], 9, 14) * p2(-43) * np.pi,
    )
    return eph, anchor[0], anchor[1]


# ── Orbit and position ───────────────────────────────────────────────────────

def sat_state(e, t):
    """ECEF position and clock correction at transmit time `t` (s of week)."""
    a = e["sqrta"] ** 2
    tk = t - e["toe"]
    tk -= 604800 * round(tk / 604800)
    mk = e["m0"] + (np.sqrt(MU / a ** 3) + e["dn"]) * tk
    ek = mk
    for _ in range(15):
        ek = mk + e["ecc"] * np.sin(ek)
    vk = np.arctan2(np.sqrt(1 - e["ecc"] ** 2) * np.sin(ek), np.cos(ek) - e["ecc"])
    phi = vk + e["aop"]
    u = phi + e["cus"] * np.sin(2 * phi) + e["cuc"] * np.cos(2 * phi)
    r = (a * (1 - e["ecc"] * np.cos(ek))
         + e["crs"] * np.sin(2 * phi) + e["crc"] * np.cos(2 * phi))
    i = (e["inc0"] + e["cis"] * np.sin(2 * phi)
         + e["cic"] * np.cos(2 * phi) + e["idot"] * tk)
    xp, yp = r * np.cos(u), r * np.sin(u)
    om = e["omg0"] + (e["omgdot"] - OMEGA_E) * tk - OMEGA_E * e["toe"]
    pos = np.array([xp * np.cos(om) - yp * np.cos(i) * np.sin(om),
                    xp * np.sin(om) + yp * np.cos(i) * np.cos(om),
                    yp * np.sin(i)])
    dtc = t - e["toc"]
    dtc -= 604800 * round(dtc / 604800)
    dts = (e["af0"] + e["af1"] * dtc + e["af2"] * dtc ** 2
           + F_REL * e["ecc"] * e["sqrta"] * np.sin(ek) - e["tgd"])
    return pos, dts


def ecef_to_llh(p):
    a, f = 6378137.0, 1 / 298.257223563
    b = a * (1 - f)
    e2, ep2 = f * (2 - f), (a * a - b * b) / (b * b)
    x, y, z = p
    r = np.hypot(x, y)
    th = np.arctan2(a * z, b * r)
    lat = np.arctan2(z + ep2 * b * np.sin(th) ** 3, r - e2 * a * np.cos(th) ** 3)
    n = a / np.sqrt(1 - e2 * np.sin(lat) ** 2)
    return np.degrees(lat), np.degrees(np.arctan2(y, x)), r / np.cos(lat) - n


def llh_to_ecef(lat, lon, h):
    a, f = 6378137.0, 1 / 298.257223563
    e2 = f * (2 - f)
    la, lo = np.radians(lat), np.radians(lon)
    n = a / np.sqrt(1 - e2 * np.sin(la) ** 2)
    return np.array([(n + h) * np.cos(la) * np.cos(lo),
                     (n + h) * np.cos(la) * np.sin(lo),
                     (n * (1 - e2) + h) * np.sin(la)])


def solve(obs):
    """Least squares for (x, y, z, clock). `obs` is [(prn, eph, t_tx), ...]."""
    x = np.array([0.0, 0.0, 6.4e6, 0.0])
    t_ref = max(o[2] for o in obs) + 0.075
    A = b = dx = None
    for _ in range(20):
        A, b = [], []
        for _prn, e, t_tx in obs:
            pos, dts = sat_state(e, t_tx)
            th = OMEGA_E * (t_ref + x[3] / C - t_tx)
            rot = np.array([[np.cos(th), np.sin(th), 0],
                            [-np.sin(th), np.cos(th), 0], [0, 0, 1]])
            d = rot @ pos - x[:3]
            rho = np.linalg.norm(d)
            A.append(np.concatenate([-d / rho, [1.0]]))
            b.append(C * (t_ref - t_tx) - (rho + x[3] - C * dts))
        A, b = np.array(A), np.array(b)
        dx = np.linalg.lstsq(A, b, rcond=None)[0]
        x = x + dx
        if np.linalg.norm(dx[:3]) < 1e-5:
            break
    return x, b - A @ dx


# ── Driver ───────────────────────────────────────────────────────────────────

def main():
    ap = argparse.ArgumentParser(description=__doc__,
                                 formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--file", required=True, help="sc8 interleaved int8 IQ capture")
    ap.add_argument("--samp-rate", type=float, default=3e6)
    ap.add_argument("--threshold", type=float, default=2.5,
                    help="acquisition peak/second-peak threshold")
    ap.add_argument("--dopp-max", type=float, default=6000.0,
                    help="half-width of the Doppler search (Hz). Two radios "
                         "running off independent crystals differ by tens of kHz "
                         "at L1 -- use 40000 for a HackRF-to-HackRF capture")
    ap.add_argument("--dopp-step", type=float, default=250.0,
                    help="Doppler search step (Hz)")
    ap.add_argument("--skip", type=float, default=0.0,
                    help="seconds to discard from the start of the capture")
    ap.add_argument("--seconds", type=float,
                    help="length of capture to use (default: all of it)")
    ap.add_argument("--truth", nargs=3, type=float, metavar=("LAT", "LON", "H"),
                    help="known simulated position, to report the error against")
    a = ap.parse_args()

    fs = a.samp_rate
    raw = np.fromfile(a.file, dtype=np.int8)
    if a.skip or a.seconds:
        lo = int(a.skip * fs) * 2
        hi = raw.size if a.seconds is None else lo + int(a.seconds * fs) * 2
        raw = raw[lo:hi]
    total_ms = int((raw.size // 2) / fs * 1000.0)
    print(f"{a.file}: {total_ms} ms @ {fs/1e6} MSPS, "
          f"I/Q rms {raw[0::2].std():.1f}/{raw[1::2].std():.1f}")
    if total_ms < 35000:
        print("  warning: under 35 s -- subframes 1-3 may not all be present")

    print("\n== acquisition ==")
    cands = []
    for prn in range(1, 33):
        r, d, ph = acquire(raw, prn, fs, dopp_max=a.dopp_max, dopp_step=a.dopp_step)
        if r > a.threshold:
            cands.append((r, prn, d, ph))
            print(f"  PRN {prn:2d}  pk/2nd {r:6.2f}  doppler {d:+6.0f} Hz")
    if not cands:
        print("  nothing acquired")
        return
    cands.sort(reverse=True)

    print("\n== track and decode ==")
    sats = []
    for _r, prn, d, ph in cands:
        oi, _oq, idx = track(raw, prn, fs, d, ph, total_ms - 5)
        sign = np.sign(oi)
        tr = np.nonzero(sign[1:] != sign[:-1])[0] + 1
        hist = np.bincount(tr % 20, minlength=20)
        off = int(hist.argmax())
        purity = hist[off] / max(1, hist.sum())
        n = (len(oi) - off) // 20
        bits = np.array([1 if oi[off + i * 20: off + i * 20 + 20].sum() > 0 else 0
                         for i in range(n)], dtype=np.int8)
        pol, start = frame_sync(bits)
        if start is None:
            print(f"  PRN {prn:2d}  bit-sync purity {purity:.2f}  -- no frame sync")
            continue
        eph, anchor_bit, anchor_tow = decode_sv(bits ^ pol, start)
        if eph is None:
            print(f"  PRN {prn:2d}  frame sync ok  -- subframes 1-3 incomplete")
            continue
        t_sub = (anchor_tow - 1) * 6.0  # the HOW names the *next* subframe
        sats.append(dict(prn=prn, e=eph, m0=off + anchor_bit * 20,
                         t_sub=t_sub, idx=idx))
        print(f"  PRN {prn:2d}  purity {purity:.2f}  health {eph['health']:2d}  "
              f"IODE {eph['iode']:3d}  toe {eph['toe']:.0f}  "
              f"a {eph['sqrta']**2/1000:.1f} km  subframe t_tx {t_sub:.1f} s")

    if len(sats) < 4:
        print(f"\nonly {len(sats)} satellites fully decoded -- cannot fix")
        return

    n0 = max(s["idx"][s["m0"]] for s in sats) + int(fs * 2.0)
    obs = []
    for s in sats:
        idx, m0 = s["idx"], s["m0"]
        m = int(np.searchsorted(idx, n0) - 1)
        if m < m0 or m + 1 >= len(idx):
            continue
        frac = (n0 - idx[m]) / (idx[m + 1] - idx[m])
        obs.append((s["prn"], s["e"], s["t_sub"] + (m - m0 + frac) * 1e-3))

    x, res = solve(obs)
    lat, lon, h = ecef_to_llh(x[:3])
    print(f"\n== position fix from {len(obs)} satellites ==")
    print(f"  {lat:.6f} N  {lon:.6f} E  height {h:.1f} m")
    print(f"  residual rms {np.sqrt((res**2).mean()):.1f} m")
    for (prn, _, _), r in zip(obs, res):
        print(f"    PRN {prn:2d}  {r:10.1f} m")

    if a.truth:
        err = x[:3] - llh_to_ecef(*a.truth)
        print(f"\n  error vs truth {a.truth[0]} N {a.truth[1]} E "
              f"{a.truth[2]} m: {np.linalg.norm(err):.1f} m")
        print("  (20-60 m is expected: no ionosphere or troposphere correction "
              "is applied here)")


if __name__ == "__main__":
    main()
