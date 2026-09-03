//! Application directory helpers.
//!
//! Every well-known directory hangs off a single *data root*, resolved once per
//! process by [`data_root`] and then reused.  Each accessor returns the path to
//! its directory, creating it on first use if it does not already exist.
//!
//! The root is deliberately **not** just the process working directory.  A
//! desktop launcher that specifies no working directory leaves the process in
//! `C:\Windows\system32` (or `/`), where creating `Rinex_files/` fails with
//! "Access denied" — the app has no business writing there even when it is
//! running elevated and the call would succeed.

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

/// Environment variable that pins the data root explicitly, overriding the
/// search below.  Intended for tests, portable installs and packagers.
pub const DATA_DIR_ENV: &str = "GUI_SDR_GPS_SIM_DATA_DIR";

/// Application identifier used for the per-user fallback directory.  Must match
/// the title passed to `eframe::run_native`, so that our directories sit beside
/// the `app.ron` that eframe persists.
const APP_ID: &str = "Gui SDR GPS Sim";

/// Returns the directory all application data lives under, creating it if
/// needed.  Resolved once and cached for the lifetime of the process.
///
/// Candidates, in order — the first writable one wins:
///
/// 1. `$GUI_SDR_GPS_SIM_DATA_DIR`, if set.
/// 2. The current working directory, unless it is a system directory.
/// 3. The directory holding the executable (portable install).
/// 4. The per-user data directory (`%APPDATA%\Gui SDR GPS Sim\data`,
///    `~/.local/share/guisdrgpssim`, `~/Library/Application Support/…`).
///
/// # Errors
/// Returns a human-readable [`String`] listing what was tried if no candidate
/// can be written to.
pub fn data_root() -> Result<PathBuf, String> {
    static ROOT: OnceLock<Result<PathBuf, String>> = OnceLock::new();
    ROOT.get_or_init(|| {
        let root = resolve_data_root();
        match &root {
            Ok(dir) => log::info!("Data directory: {}", dir.display()),
            Err(e) => log::error!("{e}"),
        }
        root
    })
    .clone()
}

/// Returns the directory used for UMF motion files (`<data root>/umf`),
/// creating it if it does not already exist.
///
/// # Errors
/// Returns a human-readable [`String`] if the directory cannot be created.
pub fn umf_dir() -> Result<PathBuf, String> {
    subdir("umf")
}

/// Returns the directory used for waypoint data (`<data root>/waypoint`),
/// creating it if it does not already exist.
///
/// # Errors
/// Returns a human-readable [`String`] if the directory cannot be created.
pub fn waypoint_dir() -> Result<PathBuf, String> {
    subdir("waypoint")
}

/// Returns the directory used to store RINEX navigation files
/// (`<data root>/Rinex_files`), creating it if it does not already exist.
///
/// # Errors
/// Returns a human-readable [`String`] if the directory cannot be created.
pub fn rinex_dir() -> Result<PathBuf, String> {
    subdir("Rinex_files")
}

/// Creates and returns `<data root>/<name>`.
fn subdir(name: &str) -> Result<PathBuf, String> {
    let dir = data_root()?.join(name);
    if !dir.exists() {
        std::fs::create_dir_all(&dir)
            .map_err(|e| format!("Cannot create '{}': {e}", dir.display()))?;
        log::info!("Created directory: {}", dir.display());
    }
    Ok(dir)
}

/// Walks the candidate list and returns the first writable directory.
fn resolve_data_root() -> Result<PathBuf, String> {
    if let Some(dir) = std::env::var_os(DATA_DIR_ENV) {
        let dir = PathBuf::from(dir);
        return if is_writable(&dir) {
            Ok(dir)
        } else {
            Err(format!(
                "{DATA_DIR_ENV} is set to '{}', which cannot be created or written to",
                dir.display()
            ))
        };
    }

    let mut tried: Vec<PathBuf> = Vec::new();
    for candidate in candidates() {
        if is_writable(&candidate) {
            return Ok(candidate);
        }
        tried.push(candidate);
    }

    let list = tried
        .iter()
        .map(|p| format!("'{}'", p.display()))
        .collect::<Vec<_>>()
        .join(", ");
    Err(format!(
        "No writable data directory found (tried {list}). \
         Set {DATA_DIR_ENV} to a directory you can write to."
    ))
}

/// The candidate roots, in preference order.
fn candidates() -> Vec<PathBuf> {
    let mut out = Vec::new();
    if let Ok(cwd) = std::env::current_dir()
        && !is_system_dir(&cwd)
    {
        out.push(cwd);
    }
    if let Some(dir) = exe_dir() {
        out.push(dir);
    }
    if let Some(dir) = eframe::storage_dir(APP_ID) {
        out.push(dir);
    }
    out
}

/// The directory holding the running executable, skipped when it sits inside a
/// macOS application bundle — writing into a bundle breaks code signing and the
/// data would vanish with the next drag-and-drop install.
fn exe_dir() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let dir = exe.parent()?;
    if dir
        .components()
        .any(|c| c.as_os_str().to_string_lossy().ends_with(".app"))
    {
        return None;
    }
    Some(dir.to_path_buf())
}

/// True for directories no application should scatter data into: a filesystem
/// root, or anything under the Windows installation directory.
fn is_system_dir(dir: &Path) -> bool {
    if dir.parent().is_none() {
        return true;
    }
    #[cfg(windows)]
    if let Some(root) = std::env::var_os("SystemRoot")
        && dir.starts_with(PathBuf::from(root))
    {
        return true;
    }
    false
}

/// Creates `dir` if needed and probes it with a real file — on Windows a
/// directory can be listable and still reject writes, so metadata alone lies.
fn is_writable(dir: &Path) -> bool {
    if std::fs::create_dir_all(dir).is_err() {
        return false;
    }
    let probe = dir.join(format!(".write-probe-{}", std::process::id()));
    match std::fs::File::create(&probe) {
        Ok(file) => {
            drop(file);
            if let Err(e) = std::fs::remove_file(&probe) {
                log::debug!("Could not remove write probe {}: {e}", probe.display());
            }
            true
        }
        Err(_) => false,
    }
}

#[cfg(test)]
mod tests {
    use super::{candidates, is_system_dir, is_writable, umf_dir};
    use std::path::Path;

    #[test]
    fn no_candidate_root_is_a_system_directory() {
        for dir in candidates() {
            assert!(
                !is_system_dir(&dir),
                "{} must never be offered as a data root",
                dir.display()
            );
        }
    }

    #[test]
    fn subdirectories_resolve_and_are_created() {
        let dir = umf_dir().expect("a writable data root");
        assert!(dir.is_dir());
        assert!(dir.ends_with("umf"));
    }

    #[test]
    fn a_writable_directory_probes_true() {
        let dir = tempfile::tempdir().expect("temp dir");
        assert!(is_writable(dir.path()));
        // The probe file must not be left behind.
        let leftovers = std::fs::read_dir(dir.path())
            .expect("read temp dir")
            .count();
        assert_eq!(leftovers, 0);
    }

    #[test]
    fn a_filesystem_root_is_a_system_dir() {
        assert!(is_system_dir(Path::new(std::path::MAIN_SEPARATOR_STR)));
    }

    #[cfg(windows)]
    #[test]
    fn the_windows_system_directory_is_rejected() {
        // The exact case reported by users: a launcher with no working
        // directory leaves the process in C:\Windows\system32.
        assert!(is_system_dir(Path::new(r"C:\Windows\system32")));
        assert!(!is_system_dir(Path::new(r"C:\Users\someone\Downloads")));
    }
}
