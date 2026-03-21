//! RINEX navigation file parser.
//!
//! Supports RINEX 2.x (`.n`) and RINEX 3.x (`.rnx`) formats, with optional
//! transparent gzip decompression for `.gz` files.
//!
//! # What is parsed
//! - **Header**: ionospheric correction coefficients (α, β), UTC parameters,
//!   leap seconds.
//! - **Data records**: GPS satellite ephemeris (one record = 8 lines per SV).
//!
//! # Ephemeris grouping
//! Records are grouped into *sets* by GPS time.  A new set is started when
//! the epoch of the next record differs by more than one hour from the current
//! set.  This lets the simulator pick the set whose reference time is closest
//! to the desired start time.

use flate2::read::GzDecoder;
use std::io::{self, BufRead};

use super::error::SimError;
use super::types::{Ephemeris, GpsTime, IonoUtc, UtcDate, consts::MAX_EPH_SETS};

// ── Public data types ─────────────────────────────────────────────────────────

/// Parsed navigation data from a RINEX file.
///
/// `eph` is indexed as `eph[set][prn - 1]`, where `set` is one of up to
/// [`MAX_EPH_SETS`] hourly groups.  Only slots with `.valid == true` are usable.
#[derive(Debug, Clone)]
pub struct NavData {
    /// Ephemeris sets, each covering approximately one hour.
    pub eph: Vec<[Ephemeris; 32]>,
    /// Ionospheric and UTC parameters from the header.
    pub iono: IonoUtc,
    /// Number of loaded sets (= `eph.len()`).
    pub count: usize,
}

// ── Entry point ───────────────────────────────────────────────────────────────

/// Load a RINEX 2 or 3 GPS navigation file.
///
/// If `path` ends in `.gz`, the file is transparently decompressed with
/// `flate2`.
///
/// # Errors
/// - [`SimError::Io`] — file not found or read failure.
/// - [`SimError::Rinex`] — malformed content.
/// - [`SimError::NoEphemeris`] — file contained no usable GPS records.
pub fn load(path: &str) -> Result<NavData, SimError> {
    let file = std::fs::File::open(path)?;
    let reader: Box<dyn BufRead> = if path.ends_with(".gz") {
        Box::new(io::BufReader::new(GzDecoder::new(file)))
    } else {
        Box::new(io::BufReader::new(file))
    };
    parse(reader)
}

// ── Parser ────────────────────────────────────────────────────────────────────

#[expect(
    clippy::too_many_lines,
    reason = "RINEX parser necessarily handles many format variants in one pass"
)]
#[expect(
    clippy::while_let_loop,
    reason = "named break 'records inside inner loop prevents conversion to while-let"
)]
#[expect(
    clippy::manual_let_else,
    reason = "break/continue targets prevent let-else refactoring here"
)]
#[expect(
    clippy::field_reassign_with_default,
    reason = "Ephemeris is populated field-by-field from parsed arrays; struct literal would be unwieldy"
)]
#[expect(
    clippy::indexing_slicing,
    reason = "fields[n] guarded by nf<28; current_set[prn-1] guarded by prn 1..=32; nums indexed after len check"
)]
fn parse(reader: Box<dyn BufRead>) -> Result<NavData, SimError> {
    let mut lines = reader.lines();
    let mut iono = IonoUtc::default();
    let mut version = 2.0_f64;
    // Bitmask tracking which header groups have been found:
    //   0x1 = ION ALPHA  0x2 = ION BETA  0x4 = UTC params
    let mut flags = 0u8;

    // ── Header section ───────────────────────────────────────────────────────
    loop {
        let line = read_line(&mut lines)?;
        let label = line.get(60..).map(str::trim).unwrap_or("");

        if label.starts_with("RINEX VERSION / TYPE") {
            version = line[0..9].trim().parse::<f64>().unwrap_or(2.0);
        } else if label.starts_with("ION ALPHA")
            || (label.starts_with("IONOSPHERIC CORR") && line.contains("GPSA"))
        {
            let start = if version >= 3.0 { 5 } else { 2 };
            iono.alpha = parse_iono_floats(&line, start);
            flags |= 0x1;
        } else if label.starts_with("ION BETA")
            || (label.starts_with("IONOSPHERIC CORR") && line.contains("GPSB"))
        {
            let start = if version >= 3.0 { 5 } else { 2 };
            iono.beta = parse_iono_floats(&line, start);
            flags |= 0x2;
        } else if label.starts_with("DELTA-UTC")
            || (label.starts_with("TIME SYSTEM CORR") && line.contains("GPUT"))
        {
            // RINEX 2: fields start at field index 3; RINEX 3: at 5.
            let off = if version >= 3.0 { 5 * 19 } else { 3 * 19 };
            iono.a0 = parse_f64_field(&line, off, 19);
            iono.a1 = parse_f64_field(&line, off + 19, 19);
            iono.tot = line
                .get(off + 38..off + 57)
                .unwrap_or("")
                .trim()
                .parse()
                .unwrap_or(0);
            iono.wnt = line
                .get(off + 57..)
                .unwrap_or("")
                .trim()
                .parse()
                .unwrap_or(0);
            flags |= 0x4;
        } else if label.starts_with("LEAP SECONDS") {
            iono.dtls = line[0..6].trim().parse().unwrap_or(0);
        } else if label.starts_with("END OF HEADER") {
            break;
        }
    }
    // All three groups must be present for iono to be valid.
    if flags == 0x7 {
        iono.valid = true;
    }

    // ── Data section ─────────────────────────────────────────────────────────
    let mut eph_sets: Vec<[Ephemeris; 32]> = Vec::new();
    let mut current_set = [Ephemeris::default(); 32];
    // GPS time of the epoch that started the current set (-1 week = none yet).
    let mut g0 = GpsTime { week: -1, sec: 0.0 };
    let mut set_count = 0usize;

    'records: loop {
        // Read the first line of the next satellite record.
        let line = match read_line(&mut lines) {
            Ok(l) => l,
            Err(_) => break,
        };
        if line.trim().is_empty() {
            continue;
        }

        // Determine PRN and data-column offset (RINEX 2 vs 3 differ by 1 col).
        let (prn, col0): (usize, usize) = if version >= 3.0 {
            // RINEX 3 prefixes non-GPS constellations with E, R, C, …; skip them.
            if !line.starts_with('G') {
                for _ in 0..7 {
                    let _skip: io::Result<String> = read_line(&mut lines);
                }
                continue;
            }
            let p = line[1..3].trim().parse::<usize>().unwrap_or(0);
            (p, 4)
        } else {
            let p = line[0..2].trim().parse::<usize>().unwrap_or(0);
            (p, 3)
        };
        if prn == 0 || prn > 32 {
            continue;
        }

        // Parse epoch and SV clock polynomial from line 1.
        let date = match parse_epoch(&line, col0) {
            Ok(d) => d,
            Err(_) => continue,
        };
        let t = date.to_gps();
        let af0 = parse_f64_field(&line, col0 + 19, 19);
        let af1 = parse_f64_field(&line, col0 + 38, 19);
        let af2 = parse_f64_field(&line, col0 + 57, 19);

        // Lines 2–8: 7 broadcast orbit lines, 4 fields × 19 chars each.
        let dc = if version >= 3.0 { 4 } else { 3 }; // data-column start
        let mut fields = [0.0_f64; 28];
        let mut nf = 0usize;
        for _ in 0..7 {
            let l = match read_line(&mut lines) {
                Ok(l) => l,
                Err(_) => break 'records,
            };
            for j in 0..4 {
                if nf < 28 {
                    fields[nf] = parse_f64_field(&l, dc + j * 19, 19);
                    nf += 1;
                }
            }
        }
        if nf < 24 {
            continue;
        } // incomplete record — skip

        // Build ephemeris struct.
        let mut e = Ephemeris::default();
        e.valid = true;
        e.iode = fields[0] as i32;
        e.crs = fields[1];
        e.deltan = fields[2];
        e.m0 = fields[3];
        e.cuc = fields[4];
        e.ecc = fields[5];
        e.cus = fields[6];
        e.sqrta = fields[7];
        // TOE: the seconds field is fields[8]; week comes from fields[18].
        e.toe = GpsTime {
            week: fields[18] as i32,
            sec: fields[8],
        };
        e.cic = fields[9];
        e.omg0 = fields[10];
        e.cis = fields[11];
        e.inc0 = fields[12];
        e.crc = fields[13];
        e.aop = fields[14];
        e.omgdot = fields[15];
        e.idot = fields[16];
        e.sva = fields[20] as i32;
        e.svh = fields[21] as i32;
        // IS-GPS-200 §20.3.3.3.1.4: SVH ∈ (0, 32) must have bit 5 set.
        if e.svh > 0 && e.svh < 32 {
            e.svh += 32;
        }
        e.tgd = fields[22];
        e.iodc = fields[23] as i32;
        e.fit = if nf > 25 { fields[25] } else { 0.0 };
        e.af0 = af0;
        e.af1 = af1;
        e.af2 = af2;
        e.toc = t;

        // Start a new ephemeris set when the epoch gap exceeds one hour.
        let dt = if g0.week < 0 { 7201.0 } else { t.sub(g0).abs() };
        if dt > 3_600.0 {
            if g0.week >= 0 {
                eph_sets.push(current_set);
                set_count += 1;
                if set_count >= MAX_EPH_SETS {
                    break;
                }
            }
            g0 = t;
            current_set = [Ephemeris::default(); 32];
        }
        current_set[prn - 1] = e;
    }

    // Push the final (possibly partial) set.
    if g0.week >= 0 {
        eph_sets.push(current_set);
        set_count += 1;
    }
    if set_count == 0 {
        return Err(SimError::NoEphemeris);
    }
    Ok(NavData {
        count: set_count,
        eph: eph_sets,
        iono,
    })
}

// ── Low-level helpers ─────────────────────────────────────────────────────────

/// Read the next line from the iterator, returning `UnexpectedEof` at end.
fn read_line(lines: &mut impl Iterator<Item = io::Result<String>>) -> io::Result<String> {
    match lines.next() {
        Some(Ok(l)) => Ok(l),
        Some(Err(e)) => Err(e),
        None => Err(io::Error::new(io::ErrorKind::UnexpectedEof, "EOF")),
    }
}

/// Extract and parse a Fortran `f64` field at byte offset `off` with width `w`.
///
/// RINEX uses Fortran exponential notation (`D` exponent marker); replace with
/// `E` before standard Rust `f64` parsing.
fn parse_f64_field(line: &str, off: usize, w: usize) -> f64 {
    line.get(off..off + w)
        .map(|s| {
            s.trim()
                .replace(['D', 'd'], "E")
                .parse::<f64>()
                .unwrap_or(0.0)
        })
        .unwrap_or(0.0)
}

/// Parse four Klobuchar iono float fields from a header line.
///
/// `start_field` is the zero-based field index; each field is 12 characters.
fn parse_iono_floats(line: &str, start_field: usize) -> [f64; 4] {
    std::array::from_fn(|i| parse_f64_field(line, start_field + i * 12, 12))
}

/// Parse the satellite epoch date/time from the first line of a data record.
#[expect(
    clippy::indexing_slicing,
    reason = "nums indexed 0..5; guarded by len() >= 6 check above"
)]
fn parse_epoch(line: &str, col0: usize) -> Result<UtcDate, SimError> {
    let s = line.get(col0..).unwrap_or("");
    let nums: Vec<f64> = s
        .split_whitespace()
        .take(6)
        .map(|x| x.parse().unwrap_or(0.0))
        .collect();
    if nums.len() < 6 {
        return Err(SimError::Rinex(format!(
            "bad epoch in: '{}'",
            &line[..line.len().min(40)]
        )));
    }
    // Two-digit year: ≥80 → 19xx, <80 → 20xx (RINEX 2 convention).
    let year = {
        let y = nums[0] as i32;
        if y >= 80 {
            y + 1900
        } else if y < 100 {
            y + 2000
        } else {
            y
        }
    };
    Ok(UtcDate {
        year,
        month: nums[1] as u8,
        day: nums[2] as u8,
        hour: nums[3] as u8,
        min: nums[4] as u8,
        sec: nums[5],
    })
}
