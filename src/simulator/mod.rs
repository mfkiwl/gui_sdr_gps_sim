//! GPS signal simulation module.
//!
//! Wraps the `gps` and `libhackrf` crates into a self-contained module
//! that exposes types and functions consumed by the UI layer.

mod state;
mod worker;

pub use state::{SimSettings, SimState, SimStatus};
pub use worker::{GPS_L1_HZ, run};

use std::{path::PathBuf, sync::mpsc, thread};

/// Opens a native file-picker dialog in a background thread so the UI stays
/// responsive. Returns a [`mpsc::Receiver`] that yields `Some(path)` when the
/// user picks a file, or `None` if they cancel.
///
/// `start_dir` sets the initial directory; if `None` the OS default is used.
pub fn open_file_dialog(
    title: impl Into<String>,
    filters: &[(&'static str, &'static [&'static str])],
    start_dir: Option<PathBuf>,
) -> mpsc::Receiver<Option<PathBuf>> {
    let (tx, rx) = mpsc::channel();
    let title = title.into();
    let filters: Vec<(&'static str, &'static [&'static str])> = filters.to_vec();

    thread::spawn(move || {
        let mut dialog = rfd::FileDialog::new().set_title(&title);
        if let Some(dir) = start_dir {
            dialog = dialog.set_directory(&dir);
        }
        for (name, exts) in filters {
            dialog = dialog.add_filter(name, exts);
        }
        tx.send(dialog.pick_file()).ok();
    });

    rx
}
