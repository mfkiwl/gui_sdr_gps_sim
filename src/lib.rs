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
