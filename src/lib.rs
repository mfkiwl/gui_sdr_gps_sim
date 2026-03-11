#![warn(clippy::all, rust_2018_idioms)]

mod app;
mod geo;
mod library;
mod import;
mod map_plugin;
mod paths;
mod rinex;
mod route;
mod simulator;
mod ui;
mod waypoint;

pub use app::MyApp;
