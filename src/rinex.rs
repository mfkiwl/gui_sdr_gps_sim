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
    let dir = PathBuf::from("./Rinex_files");
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
fn today_doy_year() -> (u32, u32) {
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

/// Blocking inner function — runs inside `spawn_blocking`.
///
/// Connects via explicit FTPS, downloads the `.gz` file, decompresses it in
/// memory, and writes the RINEX nav file to [`rinex_dir`].
fn blocking_download(doy: u32, year: u32) -> Result<PathBuf, String> {
    use std::io::Read as _;

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

    // Establish a plain FTP connection, then upgrade to explicit FTPS (AUTH TLS).
    let plain = NativeTlsFtpStream::connect(format!("{CDDIS_HOST}:21"))
        .map_err(|e| format!("FTP connect: {e}"))?;
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

/// Downloads and decompresses today's RINEX navigation file from CDDIS via
/// anonymous FTPS.
///
/// The blocking transfer is offloaded to a thread-pool thread so the async
/// caller remains responsive.  On success, returns the local [`PathBuf`] of
/// the decompressed `.n` file inside [`rinex_dir`].
///
/// # Errors
///
/// Returns a human-readable [`String`] if the directory cannot be created,
/// the FTP connection fails, the TLS handshake fails, the file transfer
/// fails, or gzip decompression fails.
pub async fn download_today_rinex() -> Result<PathBuf, String> {
    let (doy, year) = today_doy_year();
    tokio::task::spawn_blocking(move || blocking_download(doy, year))
        .await
        .map_err(|e| format!("Spawn error: {e}"))?
}
