//! Map plugins: click capture and waypoint markers.

use walkers::{MapMemory, Plugin, Position, Projector};

/// The result of a click on the map widget.
pub struct ClickResult {
    /// Geographic position of the click (`x` = lon, `y` = lat).
    pub position: Position,
    /// Absolute screen-space position, used to anchor the popup.
    pub screen_pos: egui::Pos2,
}

/// [`Plugin`] that writes the geographic position of a primary-button click
/// into a shared `Option<ClickResult>`.
pub struct ClickCapturePlugin<'a> {
    pub out: &'a mut Option<ClickResult>,
}

impl Plugin for ClickCapturePlugin<'_> {
    fn run(
        self: Box<Self>,
        _ui: &mut egui::Ui,
        response: &egui::Response,
        projector: &Projector,
        _map_memory: &MapMemory,
    ) {
        if response.clicked_by(egui::PointerButton::Primary) {
            if let Some(screen_pos) = response.interact_pointer_pos() {
                let position = projector.unproject(screen_pos.to_vec2());
                *self.out = Some(ClickResult { position, screen_pos });
            }
        }
    }
}

// ---------------------------------------------------------------------------

/// Plugin that draws filled circle markers at given geographic positions.
///
/// Each marker is a filled circle with a thin white outline so it stands out
/// against both light and dark map tiles.
pub struct WaypointMarkerPlugin<'a> {
    /// Positions to mark together with their fill colour.
    pub markers: &'a [(walkers::Position, egui::Color32)],
}

impl Plugin for WaypointMarkerPlugin<'_> {
    fn run(
        self: Box<Self>,
        ui: &mut egui::Ui,
        _response: &egui::Response,
        projector: &Projector,
        _map_memory: &MapMemory,
    ) {
        let painter = ui.painter();
        for (position, color) in self.markers {
            let screen = projector.project(*position);
            let pos = egui::pos2(screen.x, screen.y);
            painter.circle_filled(pos, 7.0, *color);
            painter.circle_stroke(pos, 7.0, egui::Stroke::new(2.0, egui::Color32::WHITE));
        }
    }
}
