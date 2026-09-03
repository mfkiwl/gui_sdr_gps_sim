//! SDR GPS simulator page and its four tabs.

use eframe::egui;

use crate::app::{MyApp, SimTab};
use crate::ui::sim::dynamic::show_sim_dynamic_tab;
use crate::ui::sim::fixed::show_sim_static_tab;
use crate::ui::sim::interactive::show_sim_interactive_tab;
use crate::ui::sim::settings::show_sim_settings_tab;
use crate::ui::widgets::page_heading;

pub(crate) mod dynamic;
pub(crate) mod fixed;
pub(crate) mod interactive;
pub(crate) mod settings;

pub(crate) fn show_sdr_gps_page(app: &mut MyApp, ui: &mut egui::Ui, ctx: &egui::Context) {
    page_heading(ui, "GPS L1 C/A Simulator");

    ui.add_space(4.0);
    ui.horizontal(|ui| {
        ui.selectable_value(&mut app.sim_tab, SimTab::Dynamic, "Dynamic Mode")
            .on_hover_text(
                "Simulate a moving receiver following a pre-recorded UMF motion CSV route.",
            );
        ui.selectable_value(&mut app.sim_tab, SimTab::Static, "Static Mode")
            .on_hover_text(
                "Simulate a stationary receiver at a fixed WGS-84 position, looping indefinitely.",
            );
        ui.selectable_value(&mut app.sim_tab, SimTab::Interactive, "Interactive Mode")
            .on_hover_text(
                "Steer the simulated receiver in real time using keyboard controls \
                 (W/S/A/D/E/Q) — no motion file required.",
            );
        ui.selectable_value(&mut app.sim_tab, SimTab::Settings, "Settings")
            .on_hover_text(
                "Configure simulation parameters and HackRF hardware settings \
                 shared by both Dynamic and Static modes.",
            );
    });
    ui.separator();

    match app.sim_tab {
        SimTab::Dynamic => show_sim_dynamic_tab(app, ui),
        SimTab::Static => show_sim_static_tab(app, ui),
        SimTab::Interactive => show_sim_interactive_tab(app, ui, ctx),
        SimTab::Settings => show_sim_settings_tab(app, ui),
    }
}
