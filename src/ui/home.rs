//! Home page - the landing view shown on startup.

use eframe::egui;

use crate::ui::widgets::page_heading;

pub(crate) fn show_home_page(ui: &mut egui::Ui) {
    page_heading(ui, "Gui SDR GPS Simulator");

    ui.label(
        "A desktop tool for generating and transmitting GPS L1 C/A signals \
         via a HackRF SDR, and for creating the UMF user-motion files they require.",
    );

    ui.add_space(12.0);

    home_card(
        ui,
        "GPS Simulator",
        "Transmit a GPS L1 C/A signal from a static position or \
         from a pre-recorded UMF motion file.",
    );
    ui.add_space(8.0);
    home_card(
        ui,
        "Create UMF Route",
        "Generate a UMF user-motion CSV from an ORS route, \
         a GeoJSON file, a hand-drawn polyline, or a GPX/KML import.",
    );
    ui.add_space(8.0);
    home_card(
        ui,
        "Manage Waypoints",
        "Store, filter, and organise named geographic coordinates. \
         Select them as route endpoints or static simulation positions.",
    );
    ui.add_space(8.0);
    home_card(
        ui,
        "Manage UMF Routes",
        "Browse, preview, edit, and delete the UMF route library. \
         Open any route in the route editor to tweak its geometry.",
    );
}

/// Renders a single info card for the home page.
pub(crate) fn home_card(ui: &mut egui::Ui, title: &str, body: &str) {
    ui.group(|ui| {
        ui.set_width(ui.available_width());
        ui.label(egui::RichText::new(title).strong().size(14.0));
        ui.add_space(4.0);
        ui.label(egui::RichText::new(body).weak());
    });
}
