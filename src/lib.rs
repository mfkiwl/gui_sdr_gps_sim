//! GUI SDR GPS Simulator — a desktop application for generating and
//! transmitting realistic GNSS signals through a `HackRF` One.
//!
//! The binary is a cross-platform [egui](https://github.com/emilk/egui) app; this
//! library crate holds everything behind it. Only two things are public:
//!
//! - [`MyApp`] — the eframe application, constructed by `src/main.rs`.
//! - [`gps_sim`] — the signal generator, usable on its own without any UI. See
//!   its module documentation for a worked example.
//!
//! Everything else (route building, the map widgets, RINEX download, waypoint
//! storage) is an implementation detail of the application and stays private.
//!
//! # Signal generation in one paragraph
//!
//! [`gps_sim`] reads a RINEX navigation file, picks the ephemeris a receiver
//! would hold at the scenario time, allocates a channel per visible satellite,
//! and accumulates their spreading codes and navigation messages into a single
//! interleaved 8-bit IQ stream at 1575.42 MHz. GPS L1 C/A, Galileo E1-B and
//! `BeiDou` B1C share that carrier and can be combined into one stream. The
//! result goes to a `HackRF` One, an IQ file, or a network socket.
//!
//! # A warning worth repeating
//!
//! Transmitting GNSS signals is regulated or prohibited in most jurisdictions
//! and can interfere with safety-critical systems. Use a shielded enclosure or
//! hold the appropriate licence.

#![warn(clippy::all, rust_2018_idioms)]

pub mod gps_sim;

mod app;
mod geo;
mod import;
mod library;
mod map_plugin;
mod paths;
mod rinex;
mod route;
mod simulator;
mod ui;
mod waypoint;

pub use app::MyApp;
