//! Application directory helpers.
//!
//! Each function returns the path to a well-known directory, creating it on
//! first use if it does not already exist.

use std::path::PathBuf;

/// Returns the directory used for UMF motion files (`./umf/`),
/// creating it if it does not already exist.
///
/// # Errors
/// Returns a human-readable [`String`] if the directory cannot be created.
pub fn umf_dir() -> Result<PathBuf, String> {
    let dir = std::env::current_dir()
        .map_err(|e| format!("Cannot determine working directory: {e}"))?
        .join("umf");
    if !dir.exists() {
        std::fs::create_dir_all(&dir)
            .map_err(|e| format!("Cannot create '{}': {e}", dir.display()))?;
        log::info!("Created UMF directory: {}", dir.display());
    }
    Ok(dir)
}

/// Returns the directory used for waypoint data (`./waypoint/`),
/// creating it if it does not already exist.
///
/// # Errors
/// Returns a human-readable [`String`] if the directory cannot be created.
pub fn waypoint_dir() -> Result<PathBuf, String> {
    let dir = std::env::current_dir()
        .map_err(|e| format!("Cannot determine working directory: {e}"))?
        .join("waypoint");
    if !dir.exists() {
        std::fs::create_dir_all(&dir)
            .map_err(|e| format!("Cannot create '{}': {e}", dir.display()))?;
        log::info!("Created waypoint directory: {}", dir.display());
    }
    Ok(dir)
}
