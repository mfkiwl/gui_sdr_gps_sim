//! Helpers for the `OpenRouteService` API key.
//!
//! The key is persisted by eframe in the OS app-data directory
//! (`%APPDATA%\gui_sdr_gps_sim\` on Windows) via the normal [`MyApp`] serde
//! field — it is **never written to any file inside the repository**.
