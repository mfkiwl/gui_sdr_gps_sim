//! Create UMF Route page - ORS directions, `GeoJSON` import, and the
//! draw-a-route map editor.

use eframe::egui;
use walkers::{HttpTiles, Map, lat_lon, sources::OpenStreetMap};

use crate::app::{AppStatus, MyApp, RouteSource};
use crate::map_plugin::{ClickCapturePlugin, PolylinePlugin, WaypointMarkerPlugin};
use crate::ui::widgets::{add_map_zoom_controls, page_heading};

/// Available `ORS` routing profiles as `(api_id, display_label)` pairs.
pub(crate) const ORS_PROFILES: &[(&str, &str)] = &[
    ("foot-walking", "Foot – Walking"),
    ("foot-hiking", "Foot – Hiking"),
    ("cycling-regular", "Cycling – Regular"),
    ("cycling-road", "Cycling – Road"),
    ("cycling-mountain", "Cycling – Mountain"),
    ("cycling-electric", "Cycling – Electric"),
    ("driving-car", "Driving – Car"),
    ("driving-hgv", "Driving – HGV"),
    ("wheelchair", "Wheelchair"),
];

/// Returns the display label for a given `ORS` profile id, or the raw id if
/// not found.
pub(crate) fn ors_profile_label(profile: &str) -> &str {
    ORS_PROFILES
        .iter()
        .find(|(id, _)| *id == profile)
        .map_or(profile, |(_, label)| label)
}

/// Deferred mutations requested by the route-creator page UI.
#[derive(Default)]
pub(crate) struct RoutePageActions {
    pub(crate) do_generate: bool,
    pub(crate) to_remove: Option<usize>,
    pub(crate) add_via: bool,
    pub(crate) set_start: Option<String>,
    pub(crate) set_end: Option<String>,
    pub(crate) add_via_with_pos: Option<String>,
    pub(crate) open_geojson_dialog: bool,
    /// Draw/import tab: remove the last polyline vertex.
    pub(crate) draw_undo_last: bool,
    /// Draw/import tab: remove all polyline vertices.
    pub(crate) draw_clear: bool,
    /// Draw/import tab: open a `GPX`/`KML` import file dialog.
    pub(crate) draw_open_import_dialog: bool,
}

/// Lazily initialises the HTTP tile fetcher the first time the map is shown.
pub(crate) fn ensure_map_tiles(app: &mut MyApp, ctx: &egui::Context) {
    if app.map_tiles.is_none() {
        app.map_tiles = Some(HttpTiles::new(OpenStreetMap, ctx.clone()));
    }
}

/// Renders the OSM map widget with waypoint markers and click capture.
pub(crate) fn show_map_widget(
    map_tiles: &mut Option<HttpTiles>,
    map_memory: &mut walkers::MapMemory,
    map_clicked: &mut Option<crate::map_plugin::ClickResult>,
    markers: &[(walkers::Position, egui::Color32)],
    ui: &mut egui::Ui,
) {
    let center = lat_lon(52.37308687621991, 4.893432625781817); // Amsterdam

    let map = Map::new(
        map_tiles.as_mut().map(|t| t as &mut dyn walkers::Tiles),
        map_memory,
        center,
    )
    .with_plugin(WaypointMarkerPlugin { markers })
    .with_plugin(ClickCapturePlugin { out: map_clicked });

    let available_width = ui.available_width();
    let map_response = ui.add_sized([available_width, 300.0], map);
    add_map_zoom_controls(ui.ctx(), map_response.rect, "route_map_zoom", map_memory);
}

/// Shows a popup anchored to the click position with coordinate assignment buttons.
/// Returns `true` when the popup should be dismissed.
pub(crate) fn show_map_click_popup(
    ui: &egui::Ui,
    click: &crate::map_plugin::ClickResult,
    actions: &mut RoutePageActions,
) -> bool {
    let lat = click.position.y();
    let lon = click.position.x();
    let coord = format!("{lat:.6}, {lon:.6}");
    let mut dismissed = false;

    egui::Area::new(egui::Id::new("map_click_popup"))
        .fixed_pos(click.screen_pos + egui::vec2(8.0, 8.0))
        .order(egui::Order::Foreground)
        .show(ui.ctx(), |ui| {
            egui::Frame::popup(ui.style()).show(ui, |ui| {
                ui.label(coord.clone())
                    .on_hover_text("Coordinates of the map click in decimal degrees.");
                ui.separator();
                if ui
                    .button("Set as Start")
                    .on_hover_text("Use this position as the route start point.")
                    .clicked()
                {
                    actions.set_start = Some(coord.clone());
                    dismissed = true;
                }
                if ui
                    .button("Add as Via Point")
                    .on_hover_text("Add this position as an intermediate via point.")
                    .clicked()
                {
                    actions.add_via_with_pos = Some(coord.clone());
                    dismissed = true;
                }
                if ui
                    .button("Set as End")
                    .on_hover_text("Use this position as the route end point.")
                    .clicked()
                {
                    actions.set_end = Some(coord.clone());
                    dismissed = true;
                }
                ui.separator();
                if ui
                    .button("Dismiss")
                    .on_hover_text("Close this popup.")
                    .clicked()
                {
                    dismissed = true;
                }
            });
        });

    dismissed
}

/// Lazily initialises the HTTP tile fetcher for the draw-route map.
pub(crate) fn ensure_draw_map_tiles(app: &mut MyApp, ctx: &egui::Context) {
    if app.draw_map_tiles.is_none() {
        app.draw_map_tiles = Some(HttpTiles::new(OpenStreetMap, ctx.clone()));
    }
}

/// Renders the draw-route OSM map with the polyline overlay and click capture.
pub(crate) fn show_draw_map_widget(
    map_tiles: &mut Option<HttpTiles>,
    map_memory: &mut walkers::MapMemory,
    map_clicked: &mut Option<crate::map_plugin::ClickResult>,
    points: &[walkers::Position],
    ui: &mut egui::Ui,
) {
    let center = lat_lon(52.37308687621991, 4.893432625781817);
    let map = Map::new(
        map_tiles.as_mut().map(|t| t as &mut dyn walkers::Tiles),
        map_memory,
        center,
    )
    .with_plugin(ClickCapturePlugin { out: map_clicked })
    .with_plugin(PolylinePlugin { points });

    let available_width = ui.available_width();
    let map_response = ui.add_sized([available_width, 400.0], map);
    add_map_zoom_controls(ui.ctx(), map_response.rect, "draw_map_zoom", map_memory);
}

#[expect(
    clippy::too_many_lines,
    reason = "three source modes (ORS API / GeoJSON file / Draw+Import) with their own sub-sections make this inherently long"
)]
pub(crate) fn show_create_route_page(app: &mut MyApp, ui: &mut egui::Ui) -> RoutePageActions {
    let mut actions = RoutePageActions::default();

    page_heading(ui, "UMF Route Creator");

    ui.horizontal(|ui| {
        ui.label("Route name:")
            .on_hover_text("Name used for the output files: {name}.csv and {name}.geojson.");
        ui.text_edit_singleline(&mut app.route_name).on_hover_text(
            "Enter a filename-safe name for the route (no spaces or special characters).",
        );
    });

    ui.add_space(4.0);

    // ── Route source selector ─────────────────────────────────────────────────
    ui.horizontal(|ui| {
        ui.label("Route source:")
            .on_hover_text("Choose how the route geometry is obtained.");
        ui.selectable_value(&mut app.route_source, RouteSource::OrsApi, "ORS API")
            .on_hover_text(
                "Fetch a turn-by-turn route from the OpenRouteService API \
                 between start, optional via points, and end.",
            );
        ui.selectable_value(
            &mut app.route_source,
            RouteSource::GeoJsonFile,
            "Load GeoJSON file",
        )
        .on_hover_text("Load a pre-existing GeoJSON route file from disk.");
        ui.selectable_value(
            &mut app.route_source,
            RouteSource::ImportKmlGpx,
            "Import KML / GPX",
        )
        .on_hover_text("Import track points from a GPX or KML file.");
        ui.selectable_value(&mut app.route_source, RouteSource::DrawImport, "Draw route")
            .on_hover_text("Click on the map to place waypoints and build a custom polyline.");
    });

    ui.separator();

    match app.route_source {
        RouteSource::OrsApi => {
            // ── ORS settings ──────────────────────────────────────────────────
            ui.horizontal(|ui| {
                ui.label("Profile:").on_hover_text(
                    "Routing profile determines which roads/paths are used \
                         and how the route is calculated.",
                );
                egui::ComboBox::from_id_salt("ors_profile")
                    .selected_text(ors_profile_label(&app.ors_profile))
                    .show_ui(ui, |ui| {
                        for &(id, label) in ORS_PROFILES {
                            ui.selectable_value(&mut app.ors_profile, id.to_owned(), label);
                        }
                    })
                    .response
                    .on_hover_text(
                        "Select the ORS routing profile that matches your simulation scenario.",
                    );
            });

            ui.separator();

            // ── ORS: start / via / end coordinate inputs ──────────────────────
            ui.horizontal(|ui| {
                ui.label("Start:").on_hover_text("Route start point.");
                ui.text_edit_singleline(&mut app.start.text).on_hover_text(
                    "Enter coordinates as \"lat, lon\" in decimal degrees, \
                         e.g. 52.3731, 4.8934. You can also click on the map.",
                );
            });

            ui.add_space(4.0);
            ui.label("Via points:")
                .on_hover_text("Optional intermediate stops the route must pass through.");

            egui::ScrollArea::vertical()
                .max_height(100.0)
                .show(ui, |ui| {
                    for (i, via) in app.viapoints.iter_mut().enumerate() {
                        ui.horizontal(|ui| {
                            ui.label(format!("Via {}:", i + 1));
                            ui.text_edit_singleline(&mut via.text).on_hover_text(
                                "Intermediate waypoint as \"lat, lon\" in decimal degrees.",
                            );
                            if ui
                                .button("X")
                                .on_hover_text("Remove this via point.")
                                .clicked()
                            {
                                actions.to_remove = Some(i);
                            }
                        });
                    }
                });

            if ui
                .button("+ Add Via Point")
                .on_hover_text("Add another intermediate waypoint to the route.")
                .clicked()
            {
                actions.add_via = true;
            }

            ui.add_space(4.0);

            ui.horizontal(|ui| {
                ui.label("End:").on_hover_text("Route end point.");
                ui.text_edit_singleline(&mut app.end.text).on_hover_text(
                    "Enter coordinates as \"lat, lon\" in decimal degrees, \
                         e.g. 52.3731, 4.8934. You can also click on the map.",
                );
            });

            ui.separator();

            ui.horizontal(|ui| {
                ui.label("Velocity:").on_hover_text(
                    "Simulated movement speed used to compute the transmit-point spacing.",
                );
                ui.add(egui::TextEdit::singleline(&mut app.velocity).desired_width(60.0))
                    .on_hover_text("Speed in km/h, e.g. 50.");
                ui.label("km/h");
            });

            ui.separator();

            // ── Map widget ────────────────────────────────────────────────────
            // Build markers from the current text fields before taking mutable
            // borrows on map_tiles / map_memory.
            // Start = green, via = orange, end = red.
            let mut markers: Vec<(walkers::Position, egui::Color32)> = Vec::new();
            if let Ok(c) = crate::geo::parse_coords(&app.start.text) {
                if let [lat, lon, ..] = c.as_slice() {
                    markers.push((lat_lon(*lat, *lon), egui::Color32::from_rgb(50, 200, 50)));
                }
            }
            for via in &app.viapoints {
                if let Ok(c) = crate::geo::parse_coords(&via.text) {
                    if let [lat, lon, ..] = c.as_slice() {
                        markers.push((lat_lon(*lat, *lon), egui::Color32::from_rgb(255, 140, 0)));
                    }
                }
            }
            if let Ok(c) = crate::geo::parse_coords(&app.end.text) {
                if let [lat, lon, ..] = c.as_slice() {
                    markers.push((lat_lon(*lat, *lon), egui::Color32::from_rgb(220, 50, 50)));
                }
            }

            ensure_map_tiles(app, ui.ctx());
            show_map_widget(
                &mut app.map_tiles,
                &mut app.map_memory,
                &mut app.map_clicked,
                &markers,
                ui,
            );
            if app.map_clicked.is_some() {
                if let Some(click) = app.map_clicked.take() {
                    let dismissed = show_map_click_popup(ui, &click, &mut actions);
                    if !dismissed {
                        app.map_clicked = Some(click);
                    }
                }
            }
        }

        RouteSource::GeoJsonFile => {
            // ── GeoJSON file picker ───────────────────────────────────────────
            ui.horizontal(|ui| {
                ui.label("GeoJSON file:");
                let display = app
                    .route_geojson_path
                    .as_deref()
                    .and_then(|p| p.file_name())
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_else(|| "None selected".to_owned());
                ui.label(egui::RichText::new(display).monospace().weak());
                let btn_label = if app.route_geojson_dialog.is_some() {
                    "…"
                } else {
                    "Browse…"
                };
                if ui
                    .add_enabled(
                        app.route_geojson_dialog.is_none(),
                        egui::Button::new(btn_label),
                    )
                    .on_hover_text(
                        "Select a GeoJSON file whose LineString geometry will be used as the route.",
                    )
                    .clicked()
                {
                    actions.open_geojson_dialog = true;
                }
            });

            ui.separator();

            ui.horizontal(|ui| {
                ui.label("Velocity:").on_hover_text(
                    "Simulated movement speed used to compute the transmit-point spacing.",
                );
                ui.add(egui::TextEdit::singleline(&mut app.velocity).desired_width(60.0))
                    .on_hover_text("Speed in km/h, e.g. 50.");
                ui.label("km/h");
            });
        }

        RouteSource::DrawImport => {
            // ── Draw route ────────────────────────────────────────────────────
            ui.label("Click on the map to place waypoints one by one.");
            ui.add_space(4.0);

            let n = app.draw_route_points.len();
            ui.horizontal(|ui| {
                ui.label(format!("{n} point{}", if n == 1 { "" } else { "s" }));
                if ui
                    .add_enabled(n > 0, egui::Button::new("Undo"))
                    .on_hover_text("Remove the last point")
                    .clicked()
                {
                    actions.draw_undo_last = true;
                }
                if ui
                    .add_enabled(n > 0, egui::Button::new("Clear"))
                    .on_hover_text("Remove all points")
                    .clicked()
                {
                    actions.draw_clear = true;
                }
            });

            if let Some(err) = &app.draw_route_status {
                ui.colored_label(egui::Color32::RED, err);
            }

            ui.separator();

            ensure_draw_map_tiles(app, ui.ctx());

            // Clone points so we can borrow map tile/memory fields separately.
            let points: Vec<walkers::Position> = app.draw_route_points.clone();

            show_draw_map_widget(
                &mut app.draw_map_tiles,
                &mut app.draw_map_memory,
                &mut app.draw_map_clicked,
                &points,
                ui,
            );

            // Appending a clicked position is safe here: the map widget's mutable
            // borrows have already been released.
            if let Some(click) = app.draw_map_clicked.take() {
                app.draw_route_points.push(click.position);
                app.draw_route_status = None;
            }

            ui.separator();

            ui.horizontal(|ui| {
                ui.label("Velocity:").on_hover_text(
                    "Simulated movement speed used to compute the transmit-point spacing.",
                );
                ui.add(egui::TextEdit::singleline(&mut app.velocity).desired_width(60.0))
                    .on_hover_text("Speed in km/h, e.g. 50.");
                ui.label("km/h");
            });
        }

        RouteSource::ImportKmlGpx => {
            // ── Import KML / GPX ──────────────────────────────────────────────
            ui.label("Select a GPX or KML file to use as the route.");
            ui.add_space(4.0);

            ui.horizontal(|ui| {
                let file_label = app
                    .draw_import_path
                    .as_deref()
                    .and_then(|p| p.file_name())
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_else(|| "No file selected".to_owned());
                ui.label(egui::RichText::new(file_label).monospace().weak())
                    .on_hover_text("Currently imported GPX or KML file.");
                let importing = app.draw_import_dialog.is_some();
                let btn_label = if importing { "…" } else { "Browse…" };
                if ui
                    .add_enabled(!importing, egui::Button::new(btn_label))
                    .on_hover_text("Select a .gpx or .kml file to import its track as the route.")
                    .clicked()
                {
                    actions.draw_open_import_dialog = true;
                }
            });

            let n = app.draw_route_points.len();
            if n > 0 {
                ui.label(format!("{n} point{} loaded", if n == 1 { "" } else { "s" }));
            }

            if let Some(err) = &app.draw_route_status {
                ui.colored_label(egui::Color32::RED, err);
            }

            ui.separator();

            ui.horizontal(|ui| {
                ui.label("Velocity:").on_hover_text(
                    "Simulated movement speed used to compute the transmit-point spacing.",
                );
                ui.add(egui::TextEdit::singleline(&mut app.velocity).desired_width(60.0))
                    .on_hover_text("Speed in km/h, e.g. 50.");
                ui.label("km/h");
            });
        }
    }

    ui.separator();

    let working = matches!(app.status, AppStatus::Working);
    let can_generate = !working
        && match app.route_source {
            RouteSource::DrawImport | RouteSource::ImportKmlGpx => app.draw_route_points.len() >= 2,
            _ => true,
        };
    if ui
        .add_enabled(can_generate, egui::Button::new("Generate User Motion File"))
        .on_hover_text(
            "Fetch the route, segmentize it at the given velocity, \
             and write the ECEF transmit points to {route_name}.csv \
             and the GeoJSON to {route_name}.geojson.",
        )
        .clicked()
    {
        actions.do_generate = true;
    }

    ui.separator();

    match &app.status {
        AppStatus::Idle => {
            ui.label("Ready.");
        }
        AppStatus::Working => {
            ui.label("Working…");
        }
        AppStatus::Done(count) => {
            let name = app.route_name.trim();
            ui.colored_label(
                egui::Color32::GREEN,
                format!("Done — {count} transmit points written to {name}.csv / {name}.geojson"),
            );
        }
        AppStatus::Error(msg) => {
            ui.colored_label(egui::Color32::RED, format!("Error: {msg}"));
        }
    }

    actions
}
