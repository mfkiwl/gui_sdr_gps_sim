//! Shared value types: GPS time, receiver location, ephemeris, ionospheric
//! corrections, and physical constants.
//!
//! All angle fields are in **radians** unless the name ends with `_deg`.
//! All times are in **seconds** unless stated otherwise.
//! All distances are in **metres**.

// ── Physical constants ────────────────────────────────────────────────────────

/// Physical and GPS signal constants referenced throughout the simulator.
pub mod consts {
    /// Speed of light in vacuum (m/s).
    pub const SPEED_OF_LIGHT: f64 = 299_792_458.0;

    /// GPS value of π (matches IS-GPS-200 exactly; slightly differs from
    /// `std::f64::consts::PI` but must be used for bit-accurate results).
    #[expect(
        clippy::approx_constant,
        reason = "IS-GPS-200 mandates this exact value; must not be replaced with std::f64::consts::PI"
    )]
    pub const GPS_PI: f64 = 3.141_592_653_589_8;

    /// Earth's rotation rate (rad/s), WGS-84.
    pub const OMEGA_EARTH: f64 = 7.292_115_146_7e-5;

    /// Earth's gravitational constant μ = GM (m³/s²), WGS-84.
    pub const GM_EARTH: f64 = 3.986_005e14;

    /// WGS-84 semi-major axis (m).
    pub const WGS84_A: f64 = 6_378_137.0;

    /// WGS-84 first eccentricity.
    pub const WGS84_E: f64 = 0.081_819_190_842_6;

    // ── L1 signal parameters ─────────────────────────────────────────────────

    /// GPS L1 carrier frequency (Hz).
    pub const FREQ_L1: f64 = 1_575_420_000.0;

    /// GPS L1 wavelength (m) = c / `f_L1`.
    pub const LAMBDA_L1: f64 = 0.190_293_672_798_365;

    /// C/A code chip rate (chips/s).
    pub const CODE_FREQ_CA: f64 = 1_023_000.0;

    /// Carrier-to-code frequency ratio = `FREQ_L1` / `CODE_FREQ_CA` = 1540.
    ///
    /// # Correctness note
    /// Doppler code rate: `f_code = CODE_FREQ_CA + f_carr / CARR_TO_CODE`
    /// (divide by 1540, **not** multiply).
    pub const CARR_TO_CODE: f64 = 1_540.0;

    // ── Simulation timing ────────────────────────────────────────────────────

    /// IQ sample rate fed to `HackRF` (Hz).
    pub const SAMPLE_RATE: f64 = 3_000_000.0;

    /// Duration of one IQ sample (s).
    pub const DT: f64 = 1.0 / SAMPLE_RATE;

    /// Duration of one GPS simulation step (s).
    /// Carrier/code frequencies and satellite positions are updated every step.
    pub const STEP_SECS: f64 = 0.1;

    /// Number of IQ samples generated per simulation step.
    /// = `STEP_SECS` / `DT` = 300 000.
    pub const SAMPLES_PER_STEP: usize = (STEP_SECS / DT) as usize;

    /// Maximum number of simultaneously tracked satellites.
    pub const MAX_CHANNELS: usize = 12;

    /// Total number of GPS PRNs (1–32).
    pub const MAX_SATS: usize = 32;

    /// Maximum number of ephemeris sets loaded from a single RINEX file.
    /// One set ≈ one hour of data.  13 sets = ~13 hours.
    pub const MAX_EPH_SETS: usize = 13;

    /// `HackRF` USB bulk transfer buffer size (bytes).
    /// Each buffer holds 262 144 interleaved signed 8-bit I/Q samples.
    pub const HACKRF_BUF_BYTES: usize = 262_144;
}

// ── GPS time ──────────────────────────────────────────────────────────────────

/// GPS time expressed as (week number, seconds-of-week).
///
/// The GPS epoch is 00:00:00 UTC on 6 January 1980.  The week number rolls
/// over every 1024 weeks (~19.7 years); we store it as `i32` so arithmetic
/// works correctly across rollovers.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct GpsTime {
    /// GPS week number (may be modulo-1024 as broadcast; treat as unambiguous
    /// here — RINEX files contain the unambiguous week).
    pub week: i32,
    /// Seconds within the week \[0, 604 800).
    pub sec: f64,
}

impl GpsTime {
    /// Seconds in one GPS week.
    pub const SECS_PER_WEEK: f64 = 604_800.0;

    /// Return `self − other` in seconds, handling week-boundary crossing.
    #[inline]
    #[expect(
        clippy::should_implement_trait,
        reason = "returns f64, not Self; cannot implement Sub<GpsTime> for GpsTime here"
    )]
    pub fn sub(self, other: Self) -> f64 {
        (self.week - other.week) as f64 * Self::SECS_PER_WEEK + (self.sec - other.sec)
    }

    /// Return `self + dt` seconds, rolling the week forward/backward as needed.
    #[inline]
    pub fn add_secs(self, dt: f64) -> Self {
        let mut s = self.sec + dt;
        let mut w = self.week;
        if s >= Self::SECS_PER_WEEK {
            s -= Self::SECS_PER_WEEK;
            w += 1;
        } else if s < 0.0 {
            s += Self::SECS_PER_WEEK;
            w -= 1;
        }
        Self { week: w, sec: s }
    }
}

// ── UTC calendar date ─────────────────────────────────────────────────────────

/// Gregorian calendar date/time in UTC.
///
/// Used only for RINEX epoch parsing and display — all internal simulation
/// time uses [`GpsTime`].
#[derive(Debug, Clone, Copy, Default)]
pub struct UtcDate {
    pub year: i32,
    pub month: u8,
    pub day: u8,
    pub hour: u8,
    pub min: u8,
    pub sec: f64,
}

impl UtcDate {
    /// Convert UTC calendar date to GPS time.
    ///
    /// GPS epoch = 6 January 1980 00:00:00 UTC.
    /// Two-digit years ≥ 80 are interpreted as 19xx; < 80 as 20xx (RINEX 2
    /// convention).
    pub fn to_gps(self) -> GpsTime {
        // Cumulative days before each month (non-leap year).
        const DOY: [i32; 12] = [0, 31, 59, 90, 120, 151, 181, 212, 243, 273, 304, 334];

        let yr = if self.year < 100 {
            if self.year >= 80 {
                self.year + 1900
            } else {
                self.year + 2000
            }
        } else {
            self.year
        };

        let is_leap = (yr % 4 == 0 && yr % 100 != 0) || yr % 400 == 0;
        let leap_bonus = i32::from(is_leap && self.month > 2);
        let month_idx = (self.month.saturating_sub(1)) as usize;
        let day_of_year = DOY.get(month_idx).copied().unwrap_or(0) + self.day as i32 + leap_bonus;

        // Days from GPS epoch (6 Jan 1980 = day 6 of 1980) to 1 Jan of `yr`.
        let y0 = yr - 1980;
        let leap_days = (y0 + 3) / 4 - (y0 + 99) / 100 + (y0 + 399) / 400;
        // Subtract 6 because GPS epoch is Jan 6 (day 6 of year), not Jan 1.
        let days_since_gps_epoch = y0 * 365 + leap_days + day_of_year - 6;

        let week = days_since_gps_epoch / 7;
        let day_of_week = days_since_gps_epoch % 7;
        let sec = day_of_week as f64 * 86_400.0
            + self.hour as f64 * 3_600.0
            + self.min as f64 * 60.0
            + self.sec;

        GpsTime { week, sec }
    }
}

// ── Receiver location ─────────────────────────────────────────────────────────

/// Geodetic position of the simulated receiver.
///
/// Internally stored in radians/metres so coordinate functions receive the
/// values they expect directly without unit conversion at every call site.
#[derive(Debug, Clone, Copy)]
pub struct Location {
    /// Geodetic latitude (rad).
    pub lat_rad: f64,
    /// Geodetic longitude (rad).
    pub lon_rad: f64,
    /// Ellipsoidal height above WGS-84 (m).
    pub height_m: f64,
}

impl Location {
    /// Construct from decimal degrees and metres.
    pub fn degrees(lat: f64, lon: f64, height_m: f64) -> Self {
        Self {
            lat_rad: lat.to_radians(),
            lon_rad: lon.to_radians(),
            height_m,
        }
    }

    /// Construct from radians and metres.
    pub fn radians(lat_rad: f64, lon_rad: f64, height_m: f64) -> Self {
        Self {
            lat_rad,
            lon_rad,
            height_m,
        }
    }
}

// ── Simulation start time ─────────────────────────────────────────────────────

/// When the simulated signal begins.
#[derive(Debug, Clone, Copy)]
pub enum StartTime {
    /// Use current system clock converted to GPS time.
    Now,
    /// Explicit UTC date/time (converted to GPS time at build).
    DateTime(UtcDate),
    /// Explicit GPS week + seconds-of-week.
    Gps(GpsTime),
}

// ── Satellite ephemeris ───────────────────────────────────────────────────────

/// Broadcast satellite ephemeris decoded from a RINEX navigation file.
///
/// All orbit parameters follow IS-GPS-200 Table 20-III field definitions.
/// Angles are in radians (not semi-circles — the RINEX parser converts).
#[derive(Debug, Clone, Copy, Default)]
pub struct Ephemeris {
    /// `true` if this slot contains valid data parsed from RINEX.
    pub valid: bool,

    /// SV health word (0 = healthy; >31 = unhealthy per IS-GPS-200 §20.3.3.3).
    /// Values in (0, 32) have 32 added during parsing to set the MSB.
    pub svh: i32,

    /// Signal accuracy index (URA index).
    pub sva: i32,

    /// Issue of data, ephemeris.
    pub iode: i32,
    /// Issue of data, clock.
    pub iodc: i32,

    /// Clock reference time (GPS time).
    pub toc: GpsTime,
    /// Ephemeris reference time (GPS time).
    pub toe: GpsTime,

    // ── Clock correction ─────────────────────────────────────────────────────
    /// Group delay differential (s).
    pub tgd: f64,
    /// Clock bias at TOC (s).
    pub af0: f64,
    /// Clock drift (s/s).
    pub af1: f64,
    /// Clock drift rate (s/s²).
    pub af2: f64,

    // ── Keplerian elements ───────────────────────────────────────────────────
    /// Eccentricity (dimensionless).
    pub ecc: f64,
    /// Square root of semi-major axis (m^½).
    pub sqrta: f64,
    /// Mean anomaly at TOE (rad).
    pub m0: f64,
    /// Right ascension of ascending node at GPS week start (rad).
    pub omg0: f64,
    /// Inclination at TOE (rad).
    pub inc0: f64,
    /// Argument of perigee (rad).
    pub aop: f64,
    /// Rate of change of right ascension (rad/s).
    pub omgdot: f64,
    /// Rate of change of inclination (rad/s).
    pub idot: f64,
    /// Mean motion correction (rad/s).
    pub deltan: f64,

    // ── Second-harmonic orbit perturbation corrections ────────────────────────
    /// Cosine correction to argument of latitude (rad).
    pub cuc: f64,
    /// Sine correction to argument of latitude (rad).
    pub cus: f64,
    /// Cosine correction to inclination angle (rad).
    pub cic: f64,
    /// Sine correction to inclination angle (rad).
    pub cis: f64,
    /// Cosine correction to orbital radius (m).
    pub crc: f64,
    /// Sine correction to orbital radius (m).
    pub crs: f64,

    /// Curve-fit interval flag.
    pub fit: f64,
}

// ── Ionospheric / UTC parameters ──────────────────────────────────────────────

/// Klobuchar ionospheric model and UTC correction parameters.
///
/// These are broadcast on GPS subframe 4, page 18, and parsed from the RINEX
/// navigation file header.
#[derive(Debug, Clone, Copy, Default)]
pub struct IonoUtc {
    /// `true` when all three parameter groups (α, β, UTC) were found in the
    /// RINEX header.
    pub valid: bool,

    /// Klobuchar amplitude polynomial coefficients α₀–α₃.
    pub alpha: [f64; 4],
    /// Klobuchar period polynomial coefficients β₀–β₃.
    pub beta: [f64; 4],

    /// UTC correction polynomial constant term A₀ (s).
    pub a0: f64,
    /// UTC correction polynomial linear term A₁ (s/s).
    pub a1: f64,
    /// Current GPS–UTC leap-second offset (s).
    pub dtls: i32,
    /// UTC reference time (s of week).
    pub tot: i32,
    /// UTC reference week number.
    pub wnt: i32,
}
