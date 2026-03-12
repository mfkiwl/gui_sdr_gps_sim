//! Downloads the current-day GPS broadcast RINEX navigation file from CDDIS.
//!
//! The remote server is `gdc.cddis.eosdis.nasa.gov` (anonymous FTPS, port 21).
//! The synchronous FTP transfer runs inside [`tokio::task::spawn_blocking`] so
//! the UI thread stays responsive.  The compressed payload is decompressed with
//! gzip and written to `./Rinex_files/`.

use std::path::PathBuf;

const CDDIS_HOST: &str = "gdc.cddis.eosdis.nasa.gov";

/// Returns the directory used to store RINEX navigation files (`../Rinex_files`),
/// creating it if it does not already exist.
///
/// # Errors
///
/// Returns a human-readable [`String`] if the directory cannot be created.
pub fn rinex_dir() -> Result<PathBuf, String> {
    let dir = std::env::current_dir()
        .map_err(|e| format!("Cannot determine working directory: {e}"))?
        .join("Rinex_files");
    if !dir.exists() {
        std::fs::create_dir_all(&dir)
            .map_err(|e| format!("Cannot create '{}': {e}", dir.display()))?;
        log::info!("Created RINEX directory: {}", dir.display());
    }
    Ok(dir)
}

/// Returns `(day_of_year, full_year)` for today in UTC.
///
/// Both values are `u32`; GPS data years are always positive so the chrono
/// `i32` year is cast safely.
pub(crate) fn today_doy_year() -> (u32, u32) {
    use chrono::{Datelike as _, Utc};
    let now = Utc::now();
    // year() returns i32 but GPS data years are always well within u32 range.
    #[expect(
        clippy::cast_sign_loss,
        reason = "GPS years are always positive; i32→u32 is safe here"
    )]
    let year = now.year() as u32;
    (now.ordinal(), year)
}

/// Returns the RINEX 2 navigation filename for today's broadcast ephemeris.
///
/// Format: `brdc{doy:03}0.{yr:02}n`  e.g. `brdc0670.25n`
pub fn today_rinex_filename() -> String {
    let (doy, year) = today_doy_year();
    let yr = year % 100;
    format!("brdc{doy:03}0.{yr:02}n")
}

/// Returns the full path to today's RINEX file inside [`rinex_dir`], or `None`
/// if the directory cannot be resolved.
pub fn today_rinex_path() -> Option<PathBuf> {
    rinex_dir().ok().map(|d| d.join(today_rinex_filename()))
}

/// Blocking download — called directly from a `std::thread::spawn` thread.
///
/// Connects via explicit FTPS, downloads the `.gz` file, decompresses it in
/// memory, and writes the RINEX nav file to [`rinex_dir`].
///
/// A 15-second TCP connect timeout and 30-second socket read/write timeouts
/// are set so every network operation fails cleanly instead of hanging.
pub(crate) fn blocking_download(doy: u32, year: u32) -> Result<PathBuf, String> {
    use std::{io::Read as _, net::ToSocketAddrs as _, time::Duration};

    use flate2::read::GzDecoder;
    use suppaftp::{NativeTlsConnector, NativeTlsFtpStream};

    let yr = year % 100;
    let filename = format!("brdc{doy:03}0.{yr:02}n");
    let gz_name = format!("{filename}.gz");
    let remote = format!("/pub/gps/data/daily/{year}/brdc/{gz_name}");

    let out_dir = rinex_dir()?;
    let out_path = out_dir.join(&filename);

    log::info!("Connecting to CDDIS FTPS for {gz_name}");

    // Build the TLS connector — `native_tls` is re-exported by `suppaftp`.
    let native = suppaftp::native_tls::TlsConnector::new()
        .map_err(|e| format!("TLS init: {e}"))?;
    let connector = NativeTlsConnector::from(native);

    // Resolve DNS before connecting so we can use connect_timeout.
    let addr = format!("{CDDIS_HOST}:21")
        .to_socket_addrs()
        .map_err(|e| format!("DNS resolution: {e}"))?
        .next()
        .ok_or_else(|| "DNS returned no addresses for CDDIS host".to_owned())?;

    // Establish a plain FTP connection with a 15-second timeout.
    // Using connect_timeout (not connect) prevents indefinite hangs when the
    // server drops SYN packets.
    let plain = NativeTlsFtpStream::connect_timeout(addr, Duration::from_secs(15))
        .map_err(|e| format!("FTP connect: {e}"))?;

    // Set 30-second read/write timeouts at the OS socket level.  These apply
    // to both the TLS handshake and all subsequent FTP transfers.  Without
    // this, the SChannel TLS handshake on Windows can hang indefinitely while
    // performing CRL/OCSP certificate verification via WinHTTP.
    plain
        .get_ref()
        .set_read_timeout(Some(Duration::from_secs(30)))
        .map_err(|e| format!("Set read timeout: {e}"))?;
    plain
        .get_ref()
        .set_write_timeout(Some(Duration::from_secs(30)))
        .map_err(|e| format!("Set write timeout: {e}"))?;

    let mut ftp = plain
        .into_secure(connector, CDDIS_HOST)
        .map_err(|e| format!("FTPS handshake: {e}"))?;

    // CDDIS accepts anonymous FTPS; password must be a valid-looking e-mail.
    ftp.login("anonymous", "anonymous@example.com")
        .map_err(|e| format!("FTP login: {e}"))?;

    let cursor = ftp
        .retr_as_buffer(&remote)
        .map_err(|e| format!("FTP download '{remote}': {e}"))?;

    ftp.quit().ok();

    // Decompress the .gz payload in memory.
    let mut decoder = GzDecoder::new(cursor);
    let mut content = Vec::new();
    decoder
        .read_to_end(&mut content)
        .map_err(|e| format!("Gzip decompress: {e}"))?;

    std::fs::write(&out_path, &content)
        .map_err(|e| format!("Write '{}': {e}", out_path.display()))?;

    log::info!("RINEX written to {}", out_path.display());
    Ok(out_path)
}

