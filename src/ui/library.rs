//! Manage UMF Routes page - the saved-route library table.

use eframe::egui;
use walkers::{HttpTiles, Map, lat_lon, sources::OpenStreetMap};

use crate::app::MyApp;
use crate::map_plugin::{EditableRoutePlugin, RouteLinePlugin};
use crate::ui::widgets::{add_map_zoom_controls, format_duration, page_heading};

/// Deferred mutations requested by the route-manager page.
#[derive(Default)]
pub(crate) struct RouteLibraryActions {
    /// Row that was clicked (select for preview).
    pub(crate) select_row: Option<usize>,
    /// Row whose "Delete" button was pressed.
    pub(crate) delete_row: Option<usize>,
    /// Row whose "Edit" button was pressed.
    pub(crate) edit_row: Option<usize>,
    /// "Done" pressed in the route editor — dismiss editor.
    pub(crate) done_editing: bool,
    /// "Open in Draw Route" pressed — transfer edited route and navigate.
    pub(crate) open_in_draw: bool,
}

pub(crate) fn show_routes_page(app: &mut MyApp, ui: &mut egui::Ui) -> RouteLibraryActions {
    let mut actions = RouteLibraryActions::default();

    // ── Edit mode ─────────────────────────────────────────────────────────────
    if let Some(idx) = app.lib_edit_entry_idx {
        let route_name = app
            .library
            .get(idx)
            .map(|e| e.name.clone())
            .unwrap_or_default();

        page_heading(ui, &format!("Edit Route: {route_name}"));
        ui.label(
            egui::RichText::new(
                "Drag vertices to reposition them.  Click on the map to add a point at the end.",
            )
            .weak(),
        );

        let n = app.lib_edit_points.len();
        ui.label(format!("{n} point{}", if n == 1 { "" } else { "s" }));

        ui.add_space(4.0);
        ui.horizontal(|ui| {
            if ui
                .button("Done")
                .on_hover_text("Finish editing and return to the route library view.")
                .clicked()
            {
                actions.done_editing = true;
            }
            if ui
                .add_enabled(n >= 2, egui::Button::new("Open in Draw Route"))
                .on_hover_text(
                    "Transfer the edited route to Create UMF Route → Draw route \
                     so it can be re-segmentized and saved as a new CSV.",
                )
                .clicked()
            {
                actions.open_in_draw = true;
            }
        });

        ui.add_space(4.0);
        ui.separator();

        // ── Editable map ──────────────────────────────────────────────────
        if app.lib_edit_map_tiles.is_none() {
            app.lib_edit_map_tiles = Some(HttpTiles::new(OpenStreetMap, ui.ctx().clone()));
        }

        let center = app
            .lib_edit_points
            .first()
            .copied()
            .unwrap_or_else(|| lat_lon(52.37308687621991, 4.893432625781817));

        // Borrow three disjoint fields of `app` simultaneously.
        let map = Map::new(
            app.lib_edit_map_tiles
                .as_mut()
                .map(|t| t as &mut dyn walkers::Tiles),
            &mut app.lib_edit_map_memory,
            center,
        )
        .with_plugin(EditableRoutePlugin {
            points: &mut app.lib_edit_points,
        });

        let w = ui.available_width();
        let map_response = ui.add_sized([w, 420.0], map);
        add_map_zoom_controls(
            ui.ctx(),
            map_response.rect,
            "lib_edit_map_zoom",
            &mut app.lib_edit_map_memory,
        );

        return actions;
    }

    // ── Normal library view ───────────────────────────────────────────────────
    page_heading(ui, "Manage UMF Routes");

    ui.separator();

    show_library_table(app, ui, &mut actions);

    ui.separator();

    // ── Route preview map ─────────────────────────────────────────────────
    if app.lib_map_tiles.is_none() {
        app.lib_map_tiles = Some(HttpTiles::new(
            walkers::sources::OpenStreetMap,
            ui.ctx().clone(),
        ));
    }

    let points: Vec<walkers::Position> = app.lib_route_points.clone();
    let map = walkers::Map::new(
        app.lib_map_tiles
            .as_mut()
            .map(|t| t as &mut dyn walkers::Tiles),
        &mut app.lib_map_memory,
        lat_lon(52.37308687621991, 4.893432625781817),
    )
    .with_plugin(RouteLinePlugin { points: &points });

    let w = ui.available_width();
    let map_response = ui.add_sized([w, 300.0], map);
    add_map_zoom_controls(
        ui.ctx(),
        map_response.rect,
        "lib_map_zoom",
        &mut app.lib_map_memory,
    );

    if app.lib_route_points.is_empty() {
        ui.label(egui::RichText::new("Select a route above to preview it on the map.").weak());
    }

    actions
}

pub(crate) fn show_library_table(
    app: &MyApp,
    ui: &mut egui::Ui,
    actions: &mut RouteLibraryActions,
) {
    if app.library.is_empty() {
        ui.add_space(8.0);
        ui.label(
            egui::RichText::new("No routes in library. Press \"Rescan Library\" to populate.")
                .weak(),
        );
        return;
    }

    egui::ScrollArea::vertical()
        .max_height(280.0)
        .show(ui, |ui| {
            egui_extras::TableBuilder::new(ui)
                .column(egui_extras::Column::initial(200.0).at_least(120.0)) // Name
                .column(egui_extras::Column::initial(110.0).at_least(80.0)) // Distance
                .column(egui_extras::Column::initial(110.0).at_least(80.0)) // Duration
                .column(egui_extras::Column::initial(110.0).at_least(80.0)) // Velocity
                .column(egui_extras::Column::initial(60.0).at_least(50.0)) // Edit
                .column(egui_extras::Column::initial(60.0).at_least(50.0)) // Delete
                .sense(egui::Sense::click())
                .resizable(true)
                .striped(true)
                .header(24.0, |mut row| {
                    row.col(|ui| {
                        ui.strong("Route Name");
                    });
                    row.col(|ui| {
                        ui.strong("Distance");
                    });
                    row.col(|ui| {
                        ui.strong("Duration");
                    });
                    row.col(|ui| {
                        ui.strong("Velocity");
                    });
                    row.col(|ui| {
                        ui.strong("Edit");
                    });
                    row.col(|ui| {
                        ui.strong("Delete");
                    });
                })
                .body(|mut body| {
                    for (i, entry) in app.library.iter().enumerate() {
                        body.row(24.0, |mut row| {
                            row.set_selected(app.library_selected_row == Some(i));

                            row.col(|ui| {
                                ui.label(&entry.name);
                            });
                            row.col(|ui| {
                                ui.label(format!("{:.2} km", entry.distance_m / 1000.0));
                            });
                            row.col(|ui| {
                                ui.label(format_duration(entry.duration_s));
                            });
                            row.col(|ui| {
                                ui.label(format!("{:.1} km/h", entry.velocity_kmh));
                            });
                            row.col(|ui| {
                                if ui
                                    .small_button("Edit")
                                    .on_hover_text(
                                        "Open this route in the map editor to drag/add vertices.",
                                    )
                                    .clicked()
                                {
                                    actions.edit_row = Some(i);
                                }
                            });
                            row.col(|ui| {
                                if ui
                                    .small_button(
                                        egui::RichText::new("Delete")
                                            .color(egui::Color32::from_rgb(200, 60, 60)),
                                    )
                                    .on_hover_text(
                                        "Permanently delete this route's CSV and GeoJSON files.",
                                    )
                                    .clicked()
                                {
                                    actions.delete_row = Some(i);
                                }
                            });

                            if row.response().clicked() {
                                actions.select_row = Some(i);
                            }
                        });
                    }
                });
        });
}
