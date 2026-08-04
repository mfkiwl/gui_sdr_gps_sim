//! Small presentation helpers shared by more than one page.
//!
//! Everything here is purely visual: it renders, and mutates nothing beyond
//! the widget-local values passed to it.

use eframe::egui;

/// Renders `+`/`-` zoom buttons overlaid in the top-left corner of a map widget.
///
/// The buttons are rendered inside a semi-transparent floating [`egui::Area`] so
/// they stay on top of the map tiles.  `id` must be unique per map instance.
pub(crate) fn add_map_zoom_controls(
    ctx: &egui::Context,
    map_rect: egui::Rect,
    id: &str,
    map_memory: &mut walkers::MapMemory,
) {
    egui::Area::new(egui::Id::new(id))
        .fixed_pos(map_rect.min + egui::vec2(8.0, 8.0))
        .order(egui::Order::Foreground)
        .interactable(true)
        .show(ctx, |ui| {
            egui::Frame::popup(ui.style()).show(ui, |ui| {
                ui.set_min_width(28.0);
                if ui.button(" + ").on_hover_text("Zoom in").clicked() {
                    map_memory.zoom_in().unwrap_or_default();
                }
                if ui.button(" − ").on_hover_text("Zoom out").clicked() {
                    map_memory.zoom_out().unwrap_or_default();
                }
            });
        });
}

/// Renders a page-level heading followed by a separator.
///
/// Use at the top of every page to give a uniform title appearance.
pub(crate) fn page_heading(ui: &mut egui::Ui, title: &str) {
    ui.add_space(2.0);
    ui.heading(title);
    ui.separator();
    ui.add_space(6.0);
}

/// Renders a bold section title inside a `ui.group()` block.
pub(crate) fn section_title(ui: &mut egui::Ui, title: &str) {
    ui.label(egui::RichText::new(title).strong().size(13.0));
    ui.add_space(3.0);
}

/// Renders a clickable bold text header for a sortable table column.
///
/// Appends a ▲ or ▼ to the label when this column is the active sort column.
/// Clicking toggles ascending/descending; clicking a new column resets to ascending.
pub(crate) fn sortable_header_text(
    ui: &mut egui::Ui,
    label: &str,
    col_idx: usize,
    sort_column: &mut Option<usize>,
    sort_ascending: &mut bool,
) {
    let arrow = if *sort_column == Some(col_idx) {
        if *sort_ascending { " ▲" } else { " ▼" }
    } else {
        ""
    };
    let text = egui::RichText::new(format!("{label}{arrow}")).strong();
    let resp = ui
        .add(egui::Label::new(text).sense(egui::Sense::click()))
        .on_hover_cursor(egui::CursorIcon::PointingHand)
        .on_hover_text(format!(
            "Click to sort by {label}. Click again to reverse order."
        ));
    if resp.clicked() {
        if *sort_column == Some(col_idx) {
            *sort_ascending = !*sort_ascending;
        } else {
            *sort_column = Some(col_idx);
            *sort_ascending = true;
        }
    }
}

/// Formats a duration in seconds as `H:MM:SS` (or `M:SS` when < 1 h).
pub(crate) fn format_duration(seconds: f64) -> String {
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "duration is always non-negative and well within u64 range"
    )]
    let total = seconds as u64;
    let h = total / 3600;
    let m = (total % 3600) / 60;
    let s = total % 60;
    if h > 0 {
        format!("{h}:{m:02}:{s:02}")
    } else {
        format!("{m}:{s:02}")
    }
}
