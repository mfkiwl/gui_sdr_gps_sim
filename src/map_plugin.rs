//! Map plugins: click capture and waypoint markers.

use walkers::{MapMemory, Plugin, Position, Projector};

/// Rectangular size reserved for the zoom control widget in the top-left corner
/// of the map.
///
/// `ClickCapturePlugin` skips clicks that fall inside this area so that pressing
/// the zoom `+`/`-` buttons does not also register as a map coordinate click.
pub const ZOOM_WIDGET_EXCLUSION: egui::Vec2 = egui::vec2(52.0, 76.0);

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
                // Don't capture clicks inside the zoom-button overlay area (top-left corner).
                let zoom_rect =
                    egui::Rect::from_min_size(response.rect.min, ZOOM_WIDGET_EXCLUSION);
                if zoom_rect.contains(screen_pos) {
                    return;
                }
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
        response: &egui::Response,
        projector: &Projector,
        _map_memory: &MapMemory,
    ) {
        let painter = ui.painter_at(response.rect);
        for (position, color) in self.markers {
            let screen = projector.project(*position);
            let pos = egui::pos2(screen.x, screen.y);
            painter.circle_filled(pos, 7.0, *color);
            painter.circle_stroke(pos, 7.0, egui::Stroke::new(2.0, egui::Color32::WHITE));
        }
    }
}

// ---------------------------------------------------------------------------

/// Plugin that draws a route as a plain polyline without vertex markers.
///
/// Use this for pre-computed routes with many points where individual
/// vertex numbering would be visually overwhelming.
pub struct RouteLinePlugin<'a> {
    /// Ordered points that define the route.
    pub points: &'a [walkers::Position],
}

impl Plugin for RouteLinePlugin<'_> {
    fn run(
        self: Box<Self>,
        ui: &mut egui::Ui,
        response: &egui::Response,
        projector: &Projector,
        _map_memory: &MapMemory,
    ) {
        if self.points.is_empty() {
            return;
        }
        let painter = ui.painter_at(response.rect);
        let stroke = egui::Stroke::new(3.0, egui::Color32::from_rgb(220, 50, 50));
        let screen_pts: Vec<egui::Pos2> = self
            .points
            .iter()
            .map(|p| {
                let s = projector.project(*p);
                egui::pos2(s.x, s.y)
            })
            .collect();
        for segment in screen_pts.windows(2) {
            if let [a, b] = segment {
                painter.line_segment([*a, *b], stroke);
            }
        }
    }
}

// ---------------------------------------------------------------------------

/// Plugin that draws a polyline through an ordered sequence of geographic
/// positions.
///
/// Segments are rendered as solid blue lines; each vertex gets a numbered
/// circle so the route order is immediately visible.
pub struct PolylinePlugin<'a> {
    /// Ordered points that define the polyline.
    pub points: &'a [walkers::Position],
}

impl Plugin for PolylinePlugin<'_> {
    fn run(
        self: Box<Self>,
        ui: &mut egui::Ui,
        response: &egui::Response,
        projector: &Projector,
        _map_memory: &MapMemory,
    ) {
        if self.points.is_empty() {
            return;
        }

        let painter = ui.painter_at(response.rect);
        let stroke = egui::Stroke::new(3.0, egui::Color32::from_rgb(30, 120, 255));

        let screen_pts: Vec<egui::Pos2> = self
            .points
            .iter()
            .map(|p| {
                let s = projector.project(*p);
                egui::pos2(s.x, s.y)
            })
            .collect();

        for segment in screen_pts.windows(2) {
            if let [a, b] = segment {
                painter.line_segment([*a, *b], stroke);
            }
        }

        for (i, &pos) in screen_pts.iter().enumerate() {
            painter.circle_filled(pos, 8.0, egui::Color32::from_rgb(30, 120, 255));
            painter.circle_stroke(pos, 8.0, egui::Stroke::new(1.5, egui::Color32::WHITE));
            painter.text(
                pos,
                egui::Align2::CENTER_CENTER,
                (i + 1).to_string(),
                egui::FontId::proportional(9.0),
                egui::Color32::WHITE,
            );
        }
    }
}
