//! Manage Waypoints page - the waypoint table, add form, and picker map.

use eframe::egui;
use egui_extras::Column;
use walkers::{HttpTiles, Map, lat_lon, sources::OpenStreetMap};

use crate::app::MyApp;
use crate::map_plugin::{ClickCapturePlugin, WaypointMarkerPlugin};
use crate::ui::widgets::{add_map_zoom_controls, page_heading, sortable_header_text};
use crate::waypoint::Waypoint;

/// Deferred mutations requested by the waypoint-manager page UI.
#[derive(Default)]
pub(crate) struct WaypointPageActions {
    pub(crate) edit_index: Option<usize>,
    pub(crate) delete_index: Option<usize>,
    /// Row that was clicked (to select and center map on).
    pub(crate) select_index: Option<usize>,
    pub(crate) save: bool,
}

pub(crate) fn show_waypoints_page(app: &mut MyApp, ui: &mut egui::Ui) -> WaypointPageActions {
    let mut actions = WaypointPageActions::default();

    page_heading(ui, "Waypoint Manager");

    ui.horizontal(|ui| {
        ui.label("Filter:")
            .on_hover_text("Type to filter the waypoint list by name, location, or category.");
        ui.add(
            egui::TextEdit::singleline(&mut app.filter_text)
                .hint_text("Search by name or location…")
                .desired_width(220.0),
        )
        .on_hover_text("Filter waypoints by name, location, or category (case-insensitive).");
    });

    ui.add_space(4.0);
    show_waypoint_table(app, ui, &mut actions);
    ui.add_space(6.0);

    // ── Waypoint map ─────────────────────────────────────────────────────────
    // Build marker list before borrowing map fields.
    let mut markers: Vec<(walkers::Position, egui::Color32)> = Vec::new();
    if let Some(idx) = app.wp_selected_row {
        if let Some(wp) = app.waypoints.get(idx) {
            markers.push((
                lat_lon(wp.lat, wp.lon),
                egui::Color32::from_rgb(70, 150, 255),
            ));
        }
    }
    if app.editing_index.is_some() {
        if let Ok(coords) = crate::geo::parse_coords(&app.new_waypoint_coords) {
            if let [lat, lon, ..] = coords.as_slice() {
                markers.push((lat_lon(*lat, *lon), egui::Color32::from_rgb(255, 140, 0)));
            }
        }
    }

    ensure_wp_map_tiles(app, ui.ctx());
    show_wp_map_widget(
        &mut app.wp_map_tiles,
        &mut app.wp_map_memory,
        &mut app.wp_map_clicked,
        &markers,
        ui,
    );

    // A click on the map fills the coordinates field.
    if let Some(click) = app.wp_map_clicked.take() {
        let lat = click.position.y();
        let lon = click.position.x();
        app.new_waypoint_coords = format!("{lat:.6}, {lon:.6}");
        app.new_waypoint_coord_error = None;
    }

    ui.add_space(8.0);

    ui.group(|ui| {
        let heading = if app.editing_index.is_some() {
            "Edit Waypoint"
        } else {
            "Add New Waypoint"
        };
        ui.heading(heading);
        ui.add_space(4.0);
        show_add_waypoint_form(app, ui);
    });

    ui.add_space(8.0);

    if ui
        .button("Save Changes")
        .on_hover_text("Persist all waypoints to disk (waypoint/ directory).")
        .clicked()
    {
        actions.save = true;
    }

    actions
}

/// Lazily initialises the HTTP tile fetcher for the waypoint-manager map.
pub(crate) fn ensure_wp_map_tiles(app: &mut MyApp, ctx: &egui::Context) {
    if app.wp_map_tiles.is_none() {
        app.wp_map_tiles = Some(HttpTiles::new(OpenStreetMap, ctx.clone()));
    }
}

/// Renders the waypoint-manager OSM map widget with optional markers.
pub(crate) fn show_wp_map_widget(
    map_tiles: &mut Option<HttpTiles>,
    map_memory: &mut walkers::MapMemory,
    map_clicked: &mut Option<crate::map_plugin::ClickResult>,
    markers: &[(walkers::Position, egui::Color32)],
    ui: &mut egui::Ui,
) {
    // Follow my_position initially; after center_at() is called it becomes Exact.
    let my_position = lat_lon(52.37308687621991, 4.893432625781817); // Amsterdam fallback

    let map = Map::new(
        map_tiles.as_mut().map(|t| t as &mut dyn walkers::Tiles),
        map_memory,
        my_position,
    )
    .with_plugin(ClickCapturePlugin { out: map_clicked })
    .with_plugin(WaypointMarkerPlugin { markers });

    let available_width = ui.available_width();
    let map_response = ui.add_sized([available_width, 250.0], map);
    add_map_zoom_controls(ui.ctx(), map_response.rect, "wp_map_zoom", map_memory);
}

#[expect(
    clippy::too_many_lines,
    reason = "table with 7 columns, sortable headers, and filter/sort snapshot logic is inherently long"
)]
pub(crate) fn show_waypoint_table(
    app: &mut MyApp,
    ui: &mut egui::Ui,
    actions: &mut WaypointPageActions,
) {
    // Build a filtered + sorted snapshot for display. Cloning avoids borrow
    // conflicts when the edit/delete buttons resolve original indices.
    let filter = app.filter_text.to_lowercase();
    let mut display: Vec<Waypoint> = app
        .waypoints
        .iter()
        .filter(|w| {
            filter.is_empty()
                || w.name.to_lowercase().contains(&filter)
                || w.location.to_lowercase().contains(&filter)
                || w.category.to_lowercase().contains(&filter)
        })
        .cloned()
        .collect();

    if let Some(col) = app.sort_column {
        display.sort_by(|a, b| {
            let ord = match col {
                0 => a.name.cmp(&b.name),
                1 => a.location.cmp(&b.location),
                2 => a.category.cmp(&b.category),
                3 => a
                    .lat
                    .partial_cmp(&b.lat)
                    .unwrap_or(std::cmp::Ordering::Equal),
                4 => a
                    .lon
                    .partial_cmp(&b.lon)
                    .unwrap_or(std::cmp::Ordering::Equal),
                _ => std::cmp::Ordering::Equal,
            };
            if app.sort_ascending {
                ord
            } else {
                ord.reverse()
            }
        });
    }

    egui::ScrollArea::vertical()
        .max_height(280.0)
        .show(ui, |ui| {
            egui_extras::TableBuilder::new(ui)
                .column(Column::initial(160.0).at_least(100.0)) // Name
                .column(Column::initial(160.0).at_least(100.0)) // Location
                .column(Column::initial(120.0).at_least(80.0)) // Category
                .column(Column::initial(100.0).at_least(70.0)) // Latitude
                .column(Column::initial(100.0).at_least(70.0)) // Longitude
                .column(Column::initial(60.0).at_least(50.0)) // Edit
                .column(Column::initial(60.0).at_least(50.0)) // Delete
                .sense(egui::Sense::click())
                .resizable(true)
                .striped(true)
                .header(24.0, |mut row| {
                    row.col(|ui| {
                        sortable_header_text(
                            ui,
                            "Name",
                            0,
                            &mut app.sort_column,
                            &mut app.sort_ascending,
                        );
                    });
                    row.col(|ui| {
                        sortable_header_text(
                            ui,
                            "Location",
                            1,
                            &mut app.sort_column,
                            &mut app.sort_ascending,
                        );
                    });
                    row.col(|ui| {
                        sortable_header_text(
                            ui,
                            "Category",
                            2,
                            &mut app.sort_column,
                            &mut app.sort_ascending,
                        );
                    });
                    row.col(|ui| {
                        sortable_header_text(
                            ui,
                            "Latitude",
                            3,
                            &mut app.sort_column,
                            &mut app.sort_ascending,
                        );
                    });
                    row.col(|ui| {
                        sortable_header_text(
                            ui,
                            "Longitude",
                            4,
                            &mut app.sort_column,
                            &mut app.sort_ascending,
                        );
                    });
                    row.col(|ui| {
                        ui.strong("Edit");
                    });
                    row.col(|ui| {
                        ui.strong("Delete");
                    });
                })
                .body(|mut body| {
                    for waypoint in &display {
                        // Resolve to the original index (filter/sort may have reordered).
                        let orig_idx = app.waypoints.iter().position(|w| {
                            w.name == waypoint.name
                                && w.lat == waypoint.lat
                                && w.lon == waypoint.lon
                        });
                        body.row(28.0, |mut row| {
                            row.set_selected(app.wp_selected_row == orig_idx && orig_idx.is_some());

                            row.col(|ui| {
                                ui.label(&waypoint.name);
                            });
                            row.col(|ui| {
                                ui.label(&waypoint.location);
                            });
                            row.col(|ui| {
                                ui.label(&waypoint.category);
                            });
                            row.col(|ui| {
                                ui.label(format!("{:.6}", waypoint.lat));
                            });
                            row.col(|ui| {
                                ui.label(format!("{:.6}", waypoint.lon));
                            });

                            let mut action_clicked = false;
                            row.col(|ui| {
                                if ui
                                    .small_button("Edit")
                                    .on_hover_text("Load this waypoint into the edit form below.")
                                    .clicked()
                                {
                                    actions.edit_index = orig_idx;
                                    actions.select_index = orig_idx;
                                    action_clicked = true;
                                }
                            });
                            row.col(|ui| {
                                if ui
                                    .small_button(
                                        egui::RichText::new("Delete")
                                            .color(egui::Color32::from_rgb(200, 60, 60)),
                                    )
                                    .on_hover_text("Permanently delete this waypoint.")
                                    .clicked()
                                {
                                    actions.delete_index = orig_idx;
                                    action_clicked = true;
                                }
                            });

                            // Row click (on data cells) selects and centers map.
                            if !action_clicked && row.response().clicked() {
                                actions.select_index = orig_idx;
                            }
                        });
                    }
                });
        });
}

/// Renders the add / edit waypoint form and applies changes immediately.
pub(crate) fn show_add_waypoint_form(app: &mut MyApp, ui: &mut egui::Ui) {
    egui::Grid::new("add_waypoint_grid")
        .num_columns(2)
        .spacing([8.0, 6.0])
        .show(ui, |ui| {
            ui.label("Coordinates (lat, lon):").on_hover_text(
                "WGS-84 latitude and longitude in decimal degrees. \
                     You can also click on the map to fill this field automatically.",
            );
            ui.add(
                egui::TextEdit::singleline(&mut app.new_waypoint_coords)
                    .hint_text("e.g. 52.3731, 4.8934")
                    .desired_width(220.0),
            )
            .on_hover_text(
                "Enter as \"lat, lon\" in decimal degrees, e.g. 52.3731, 4.8934. \
                 Or click on the map above.",
            );
            ui.end_row();

            ui.label("Name:")
                .on_hover_text("Short identifying name for the waypoint.");
            ui.text_edit_singleline(&mut app.new_waypoint.name)
                .on_hover_text("A short, unique name for this waypoint.");
            ui.end_row();

            ui.label("Location:")
                .on_hover_text("City, area, or place description for the waypoint.");
            ui.text_edit_singleline(&mut app.new_waypoint.location)
                .on_hover_text(
                    "Human-readable description of the location, e.g. \"Amsterdam, NL\".",
                );
            ui.end_row();

            ui.label("Category:").on_hover_text(
                "Tag used to group waypoints, e.g. \"Airport\", \"Home\", \"Test\".",
            );
            ui.text_edit_singleline(&mut app.new_waypoint.category)
                .on_hover_text(
                    "Category label for filtering and grouping, e.g. \"Airport\" or \"City\".",
                );
            ui.end_row();
        });

    if let Some(err) = &app.new_waypoint_coord_error.clone() {
        ui.label(egui::RichText::new(err).color(egui::Color32::RED).small());
    }

    ui.add_space(4.0);

    let btn_label = if app.editing_index.is_some() {
        "Update Waypoint"
    } else {
        "Add Waypoint"
    };

    if ui
        .button(btn_label)
        .on_hover_text(if app.editing_index.is_some() {
            "Save changes to the selected waypoint."
        } else {
            "Add a new waypoint to the list using the fields above."
        })
        .clicked()
    {
        let wp = &app.new_waypoint;
        let all_fields_filled =
            !wp.name.is_empty() && !wp.location.is_empty() && !wp.category.is_empty();

        match crate::geo::parse_coords(&app.new_waypoint_coords) {
            Ok(coords) => {
                if let [lat, lon, ..] = coords.as_slice() {
                    if all_fields_filled {
                        app.new_waypoint.lat = *lat;
                        app.new_waypoint.lon = *lon;
                        app.new_waypoint_coord_error = None;
                        app.waypoints.push(app.new_waypoint.clone());
                        if let Some(index) = app.editing_index.take() {
                            app.delete_waypoint(index);
                        }
                        app.new_waypoint = Waypoint::default();
                        app.new_waypoint_coords = String::new();
                    }
                } else {
                    app.new_waypoint_coord_error = Some("Enter lat, lon".to_owned());
                }
            }
            Err(e) => {
                app.new_waypoint_coord_error = Some(format!("{e}"));
            }
        }
    }

    if app.editing_index.is_some()
        && ui
            .button("Cancel Edit")
            .on_hover_text("Discard edits and return to the Add New Waypoint form.")
            .clicked()
    {
        app.editing_index = None;
        app.new_waypoint = Waypoint::default();
        app.new_waypoint_coords = String::new();
        app.new_waypoint_coord_error = None;
    }
}
