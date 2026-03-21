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
                let zoom_rect = egui::Rect::from_min_size(response.rect.min, ZOOM_WIDGET_EXCLUSION);
                if zoom_rect.contains(screen_pos) {
                    return;
                }
                let position = projector.unproject(screen_pos.to_vec2());
                *self.out = Some(ClickResult {
                    position,
                    screen_pos,
                });
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

/// Plugin that allows interactive editing of a route polyline.
///
/// - **Drag a vertex** to move it to a new geographic position.
/// - **Click near a segment** to insert a new point along that segment.
/// - **Click away from all segments** (empty map) to append a new point at
///   the end of the route.
pub struct EditableRoutePlugin<'a> {
    /// Mutable ordered list of route waypoints.  Modified in-place by drag
    /// and click interactions.
    pub points: &'a mut Vec<walkers::Position>,
}

impl Plugin for EditableRoutePlugin<'_> {
    fn run(
        self: Box<Self>,
        ui: &mut egui::Ui,
        response: &egui::Response,
        projector: &Projector,
        _map_memory: &MapMemory,
    ) {
        let painter = ui.painter_at(response.rect);

        // Collect screen-space positions first (immutable borrow only).
        let screen_pts: Vec<egui::Pos2> = self
            .points
            .iter()
            .map(|p| {
                let s = projector.project(*p);
                egui::pos2(s.x, s.y)
            })
            .collect();

        // Determine which segment (if any) the pointer is near, so we can
        // highlight it and use it for the insert-along-segment behaviour.
        const SEGMENT_HIT_DIST: f32 = 12.0;
        let hovered_segment: Option<usize> = ui
            .ctx()
            .pointer_latest_pos()
            .and_then(|ptr| nearest_segment_idx(&screen_pts, ptr, SEGMENT_HIT_DIST));

        // Draw polyline — highlight the hovered segment in a lighter colour.
        for (i, segment) in screen_pts.windows(2).enumerate() {
            if let [a, b] = segment {
                let color = if hovered_segment == Some(i) {
                    egui::Color32::from_rgb(255, 100, 100)
                } else {
                    egui::Color32::from_rgb(220, 50, 50)
                };
                painter.line_segment([*a, *b], egui::Stroke::new(3.0, color));
            }
        }

        const VERTEX_RADIUS: f32 = 8.0;
        const HIT_RADIUS: f32 = VERTEX_RADIUS * 1.8;

        // Process vertex interactions — collect the drag result without
        // mutating `points` yet (borrow checker requires one mutable use at a time).
        let mut drag_update: Option<(usize, walkers::Position)> = None;
        let mut pointer_on_vertex = false;

        for (i, &pos) in screen_pts.iter().enumerate() {
            let rect =
                egui::Rect::from_center_size(pos, egui::vec2(HIT_RADIUS * 2.0, HIT_RADIUS * 2.0));
            let id = response.id.with(("edit_v", i));
            let vr = ui.interact(rect, id, egui::Sense::drag());

            if vr.dragged() {
                if let Some(ptr) = vr.interact_pointer_pos() {
                    drag_update = Some((i, projector.unproject(ptr.to_vec2())));
                }
            }
            if vr.contains_pointer() || vr.dragged() {
                pointer_on_vertex = true;
            }

            let fill = if vr.hovered() || vr.dragged() {
                egui::Color32::from_rgb(255, 140, 0)
            } else {
                egui::Color32::from_rgb(220, 50, 50)
            };
            painter.circle_filled(pos, VERTEX_RADIUS, fill);
            painter.circle_stroke(
                pos,
                VERTEX_RADIUS,
                egui::Stroke::new(1.5, egui::Color32::WHITE),
            );
        }

        // Apply drag (mutable borrow of points — after the immutable loop above).
        if let Some((i, new_pos)) = drag_update {
            if let Some(pt) = self.points.get_mut(i) {
                *pt = new_pos;
            }
        }

        // Click: insert along the nearest segment, or append if no segment is near.
        if response.clicked_by(egui::PointerButton::Primary) && !pointer_on_vertex {
            if let Some(screen_pos) = response.interact_pointer_pos() {
                let zoom_rect = egui::Rect::from_min_size(response.rect.min, ZOOM_WIDGET_EXCLUSION);
                if !zoom_rect.contains(screen_pos) {
                    let geo_pos = projector.unproject(screen_pos.to_vec2());
                    match nearest_segment_idx(&screen_pts, screen_pos, SEGMENT_HIT_DIST) {
                        Some(seg_idx) => {
                            // Insert after the segment's start vertex.
                            self.points.insert(seg_idx + 1, geo_pos);
                        }
                        None => {
                            self.points.push(geo_pos);
                        }
                    }
                }
            }
        }
    }
}

/// Returns the index of the first vertex of the segment closest to `point`
/// whose perpendicular distance is within `threshold` pixels, or `None` if no
/// segment is close enough.
fn nearest_segment_idx(
    screen_pts: &[egui::Pos2],
    point: egui::Pos2,
    threshold: f32,
) -> Option<usize> {
    let mut best_idx: Option<usize> = None;
    let mut best_dist = threshold;

    for (i, segment) in screen_pts.windows(2).enumerate() {
        if let [a, b] = segment {
            let dist = point_to_segment_dist(point, *a, *b);
            if dist < best_dist {
                best_dist = dist;
                best_idx = Some(i);
            }
        }
    }

    best_idx
}

/// Minimum distance from point `p` to the finite line segment `[a, b]`.
fn point_to_segment_dist(p: egui::Pos2, a: egui::Pos2, b: egui::Pos2) -> f32 {
    let ab = b - a;
    let len_sq = ab.length_sq();
    if len_sq < f32::EPSILON {
        return (p - a).length();
    }
    let t = ((p - a).dot(ab) / len_sq).clamp(0.0, 1.0);
    (p - (a + ab * t)).length()
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
