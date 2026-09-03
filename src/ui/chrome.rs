//! Application chrome: menu bar, navigation sidebar, modal dialogs, and the
//! central-panel dispatcher that routes to the active page.

use eframe::egui;

use crate::app::{AppPage, MyApp};
use crate::ui::home::show_home_page;
use crate::ui::library::show_routes_page;
use crate::ui::route::show_create_route_page;
use crate::ui::sim::show_sdr_gps_page;
use crate::ui::waypoints::show_waypoints_page;
use crate::waypoint::WaypointEntry;

pub(crate) fn show_menu_bar(app: &mut MyApp, ctx: &egui::Context) {
    egui::TopBottomPanel::top("menu_bar").show(ctx, |ui| {
        egui::MenuBar::new().ui(ui, |ui| {
            ui.menu_button("File", |ui| {
                if ui
                    .button("Set ORS API Key…")
                    .on_hover_text(
                        "Enter your OpenRouteService API key, required for the ORS API route source.",
                    )
                    .clicked()
                {
                    app.ors_key_input = app.ors_api_key.clone();
                    app.ors_key_show = false;
                    app.ors_key_dialog_open = true;
                    ui.close();
                }
                ui.separator();
                if ui.button("Quit").on_hover_text("Close the application.").clicked() {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
            });

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                egui::widgets::global_theme_preference_buttons(ui);
            });
        });
    });
}

pub(crate) fn show_api_key_dialog(app: &mut MyApp, ctx: &egui::Context) {
    if !app.ors_key_dialog_open {
        return;
    }

    let mut window_open = true;
    egui::Window::new("Set ORS API Key")
        .collapsible(false)
        .resizable(false)
        .open(&mut window_open)
        .show(ctx, |ui| {
            ui.label("OpenRouteService API Key:")
                .on_hover_text("Obtain a free key at openrouteservice.org/dev/#/signup");
            ui.horizontal(|ui| {
                ui.add(
                    egui::TextEdit::singleline(&mut app.ors_key_input)
                        .password(!app.ors_key_show)
                        .desired_width(300.0),
                )
                .on_hover_text("Paste your ORS API key here.");
                let eye = if app.ors_key_show { "🔒" } else { "👁" };
                if ui
                    .button(eye)
                    .on_hover_text(if app.ors_key_show {
                        "Hide the API key"
                    } else {
                        "Show the API key"
                    })
                    .clicked()
                {
                    app.ors_key_show = !app.ors_key_show;
                }
            });

            ui.add_space(4.0);
            ui.horizontal(|ui| {
                if ui
                    .button("Save")
                    .on_hover_text("Save the key and close this dialog.")
                    .clicked()
                {
                    app.ors_api_key = app.ors_key_input.trim().to_owned();
                    app.ors_key_dialog_open = false;
                }
                if ui
                    .button("Cancel")
                    .on_hover_text("Discard changes and close this dialog.")
                    .clicked()
                {
                    app.ors_key_dialog_open = false;
                }
            });
        });

    if !window_open {
        app.ors_key_dialog_open = false;
    }
}

pub(crate) fn show_nav_panel(app: &mut MyApp, ctx: &egui::Context) {
    egui::SidePanel::left("nav_panel")
        .default_width(200.0)
        .show(ctx, |ui| {
            let logo_resp = ui
                .add(
                    egui::Image::new(egui::include_image!("../../assets/img/icon-1024.png"))
                        .max_width(200.0)
                        .maintain_aspect_ratio(true)
                        .shrink_to_fit()
                        .corner_radius(10)
                        .sense(egui::Sense::click()),
                )
                .on_hover_text("Go to the Home page.");
            if logo_resp.clicked() {
                navigate(app, AppPage::Home);
            }
            ui.add_space(4.0);

            if nav_image_active_with_tooltip(
                ui,
                egui::include_image!("../../assets/img/sdr_gps_simulator.png"),
                app.current_mode == AppPage::SdrGpsSimulator,
                "GPS Simulator — transmit GPS L1 C/A signals via HackRF \
                 from a dynamic route or a static position.",
            ) {
                navigate(app, AppPage::SdrGpsSimulator);
            }
            if nav_image_active_with_tooltip(
                ui,
                egui::include_image!("../../assets/img/create_umf_route.png"),
                app.current_mode == AppPage::CreateUmfRoute,
                "Create UMF Route — generate a GPS user-motion CSV file \
                 from an ORS route, GeoJSON, a drawn polyline, or a GPX/KML import.",
            ) {
                navigate(app, AppPage::CreateUmfRoute);
            }
            if nav_image_active_with_tooltip(
                ui,
                egui::include_image!("../../assets/img/manage_waypoints.png"),
                app.current_mode == AppPage::ManageWaypoints,
                "Manage Waypoints — store and organise named geographic coordinates \
                 to use as route endpoints or static simulation positions.",
            ) {
                navigate(app, AppPage::ManageWaypoints);
            }
            if nav_image_active_with_tooltip(
                ui,
                egui::include_image!("../../assets/img/manage_umf_routes.png"),
                app.current_mode == AppPage::ManageUmfRoutes,
                "Manage UMF Routes — browse, preview, edit, and delete \
                 saved UMF route CSV files.",
            ) {
                navigate(app, AppPage::ManageUmfRoutes);
            }

            ui.separator();
            ui.allocate_space(ui.available_size());
        });
}

/// Switches to a new page, auto-saving waypoints if leaving [`AppPage::ManageWaypoints`],
/// and loading them when entering it.
pub(crate) fn navigate(app: &mut MyApp, new_page: AppPage) {
    if app.current_mode == AppPage::ManageWaypoints && new_page != AppPage::ManageWaypoints {
        app.save_waypoints();
    }
    if new_page == AppPage::ManageWaypoints {
        app.load_waypoints();
    }
    if new_page == AppPage::ManageUmfRoutes {
        app.clear_and_rescan_library();
        // Restore the map preview for the previously selected row (if still valid).
        if let Some(i) = app.library_selected_row {
            if let Some(entry) = app.library.get(i) {
                let name = entry.name.clone();
                app.load_library_route(&name);
            } else {
                // Selected index no longer exists after rescan — clear stale state.
                app.library_selected_row = None;
                app.lib_route_points.clear();
            }
        }
    }
    app.current_mode = new_page;
}

/// Renders a nav image button with an optional hover tooltip.
///
/// Draws a highlighted left border when `active`. Pass an empty `tooltip` string
/// to skip adding the tooltip.
pub(crate) fn nav_image_active_with_tooltip(
    ui: &mut egui::Ui,
    src: egui::ImageSource<'_>,
    active: bool,
    tooltip: &str,
) -> bool {
    if active {
        let accent_color = egui::Color32::from_rgb(100, 160, 255);
        egui::Frame::new()
            .inner_margin(egui::Margin {
                left: 4,
                ..Default::default()
            })
            .stroke(egui::Stroke::NONE)
            .show(ui, |ui| {
                let resp = ui.add(
                    egui::Image::new(src)
                        .max_width(196.0)
                        .maintain_aspect_ratio(true)
                        .shrink_to_fit()
                        .corner_radius(10)
                        .sense(egui::Sense::click()),
                );
                // draw left accent bar
                let bar = egui::Rect::from_min_size(
                    resp.rect.min - egui::vec2(6.0, 0.0),
                    egui::vec2(3.0, resp.rect.height()),
                );
                ui.painter().rect_filled(bar, 0.0, accent_color);
                if !tooltip.is_empty() {
                    resp.on_hover_text(tooltip).clicked()
                } else {
                    resp.clicked()
                }
            })
            .inner
    } else {
        let resp = ui.add(
            egui::Image::new(src)
                .max_width(200.0)
                .maintain_aspect_ratio(true)
                .shrink_to_fit()
                .corner_radius(10)
                .sense(egui::Sense::click()),
        );
        if !tooltip.is_empty() {
            resp.on_hover_text(tooltip).clicked()
        } else {
            resp.clicked()
        }
    }
}

#[expect(
    clippy::too_many_lines,
    reason = "central panel dispatches to all pages and applies deferred actions for each — splitting further would obscure the control flow"
)]
pub(crate) fn show_central_panel(app: &mut MyApp, ctx: &egui::Context) {
    // Copy current_mode (it's Copy) before entering the closure so we can
    // still borrow `app` mutably inside it.
    let current_mode = app.current_mode;

    egui::CentralPanel::default().show(ctx, |ui| {
        egui::ScrollArea::vertical()
            .auto_shrink([false; 2])
            .show(ui, |ui| {
                match current_mode {
                    AppPage::Home => show_home_page(ui),
                    AppPage::SdrGpsSimulator => show_sdr_gps_page(app, ui, ctx),
                    AppPage::CreateUmfRoute => {
                        // Collect deferred mutations to apply after the UI is rendered,
                        // avoiding conflicts with borrows held inside the egui closures.
                        let actions = show_create_route_page(app, ui);
                        if let Some(i) = actions.to_remove {
                            app.viapoints.remove(i);
                        }
                        if actions.add_via {
                            app.viapoints.push(WaypointEntry::default());
                        }
                        if actions.do_generate {
                            app.generate();
                        }
                        if let Some(pos) = actions.set_start {
                            app.start.text = pos;
                        }
                        if let Some(pos) = actions.set_end {
                            app.end.text = pos;
                        }
                        if let Some(pos) = actions.add_via_with_pos {
                            app.viapoints.push(WaypointEntry { text: pos });
                        }
                        if actions.open_geojson_dialog {
                            app.route_geojson_dialog = Some(crate::simulator::open_file_dialog(
                                "Select GeoJSON Route File",
                                &[("GeoJSON", &["geojson", "json"])],
                                crate::paths::umf_dir().ok(),
                            ));
                        }
                        if actions.draw_undo_last {
                            app.draw_route_points.pop();
                        }
                        if actions.draw_clear {
                            app.draw_route_points.clear();
                            app.draw_route_status = None;
                        }
                        if actions.draw_open_import_dialog {
                            app.draw_import_dialog = Some(crate::simulator::open_file_dialog(
                                "Import GPX or KML Route File",
                                &[("Route files", &["gpx", "kml"])],
                                None,
                            ));
                        }
                    }
                    AppPage::ManageWaypoints => {
                        let actions = show_waypoints_page(app, ui);
                        if let Some(i) = actions.edit_index {
                            app.wp_selected_row = Some(i);
                            if let Some(wp) = app.waypoints.get(i) {
                                app.wp_map_memory
                                    .center_at(walkers::lat_lon(wp.lat, wp.lon));
                            }
                            app.edit_waypoint(i);
                        }
                        if let Some(i) = actions.delete_index {
                            if app.wp_selected_row == Some(i) {
                                app.wp_selected_row = None;
                            }
                            app.delete_waypoint(i);
                        }
                        if let Some(i) = actions.select_index {
                            app.wp_selected_row = Some(i);
                            if let Some(wp) = app.waypoints.get(i) {
                                app.wp_map_memory
                                    .center_at(walkers::lat_lon(wp.lat, wp.lon));
                            }
                        }
                        if actions.save {
                            app.save_waypoints();
                        }
                    }
                    AppPage::ManageUmfRoutes => {
                        let actions = show_routes_page(app, ui);
                        if let Some(i) = actions.select_row {
                            app.library_selected_row = Some(i);
                            if let Some(entry) = app.library.get(i) {
                                let name = entry.name.clone();
                                app.load_library_route(&name);
                            }
                        }
                        if let Some(i) = actions.delete_row {
                            app.delete_library_route(i);
                            app.clear_and_rescan_library();
                            // Clear selection/map if the deleted row was selected.
                            if app.library_selected_row == Some(i) {
                                app.library_selected_row = None;
                                app.lib_route_points.clear();
                            }
                        }
                        if let Some(i) = actions.edit_row {
                            app.load_lib_edit_route(i);
                        }
                        if actions.done_editing {
                            app.lib_edit_entry_idx = None;
                        }
                        if actions.open_in_draw {
                            app.open_lib_edit_in_draw_route();
                            app.current_mode = AppPage::CreateUmfRoute;
                        }
                    }
                }
            }); // ScrollArea
    });
}
