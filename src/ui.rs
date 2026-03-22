//! UI rendering: menu bar, navigation sidebar, and all page views.
//!
//! The public entry point is [`update`], called every frame by
//! [`crate::app::MyApp`]'s `eframe::App` implementation.

use eframe::egui;
use egui_extras::Column;
use walkers::{HttpTiles, Map, lat_lon, sources::OpenStreetMap};

use crate::{
    app::{AppPage, AppStatus, MyApp, RouteSource, SimTab},
    map_plugin::{
        ClickCapturePlugin, EditableRoutePlugin, PolylinePlugin, RouteLinePlugin,
        WaypointMarkerPlugin,
    },
    waypoint::{Waypoint, WaypointEntry},
};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Renders `+`/`-` zoom buttons overlaid in the top-left corner of a map widget.
///
/// The buttons are rendered inside a semi-transparent floating [`egui::Area`] so
/// they stay on top of the map tiles.  `id` must be unique per map instance.
fn add_map_zoom_controls(
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
fn page_heading(ui: &mut egui::Ui, title: &str) {
    ui.add_space(2.0);
    ui.heading(title);
    ui.separator();
    ui.add_space(6.0);
}

/// Renders a bold section title inside a `ui.group()` block.
fn section_title(ui: &mut egui::Ui, title: &str) {
    ui.label(egui::RichText::new(title).strong().size(13.0));
    ui.add_space(3.0);
}

// ---------------------------------------------------------------------------
// Public entry point
// ---------------------------------------------------------------------------

/// Main render entry point — called every frame from `eframe::App::update`.
#[expect(
    clippy::too_many_lines,
    reason = "top-level update polls multiple independent background tasks and then delegates to page renderers"
)]
pub fn update(app: &mut MyApp, ctx: &egui::Context) {
    // Poll the background pipeline task for a finished result.
    if let Ok(result) = app.result_rx.try_recv() {
        app.status = match result {
            Ok(count) => {
                // A new CSV was written — refresh the route library so the new
                // entry appears immediately on the ManageUmfRoutes page.
                app.library_loaded = false;
                app.load_library();
                app.scan_library();
                AppStatus::Done(count)
            }
            Err(msg) => AppStatus::Error(msg),
        };
    }

    // Keep repainting while the pipeline is running so the spinner stays live.
    if matches!(app.status, AppStatus::Working) {
        ctx.request_repaint_after(std::time::Duration::from_millis(100));
    }

    // Poll the GeoJSON file-dialog for the route creator page.
    if let Some(rx) = &app.route_geojson_dialog {
        if let Ok(path) = rx.try_recv() {
            app.route_geojson_path = path;
            app.route_geojson_dialog = None;
        }
    }

    // ── GPS Simulator bookkeeping ────────────────────────────────────────────

    // Poll pending file-dialog results.
    if let Some(rx) = &app.sim_rinex_dialog {
        if let Ok(path) = rx.try_recv() {
            app.sim_rinex_path = path;
            app.sim_rinex_dialog = None;
        }
    }
    if let Some(rx) = &app.sim_motion_dialog {
        if let Ok(path) = rx.try_recv() {
            app.sim_motion_path = path;
            app.sim_motion_dialog = None;
        }
    }

    // Poll the GPX/KML import dialog.
    if let Some(rx) = &app.draw_import_dialog {
        if let Ok(maybe_path) = rx.try_recv() {
            app.draw_import_dialog = None;
            if let Some(path) = maybe_path {
                match crate::import::load_route_file(&path) {
                    Ok(points) => {
                        app.draw_route_points = points
                            .into_iter()
                            .map(|[lat, lon]| walkers::lat_lon(lat, lon))
                            .collect();
                        if let Some(first) = app.draw_route_points.first() {
                            app.draw_map_memory.center_at(*first);
                        }
                        app.draw_import_path = Some(path);
                        app.draw_route_status = None;
                    }
                    Err(e) => {
                        app.draw_route_status = Some(e);
                    }
                }
            }
        }
    }

    // Poll static simulator file-dialog result.
    if let Some(rx) = &app.sim_static_rinex_dialog {
        if let Ok(path) = rx.try_recv() {
            app.sim_static_rinex_path = path;
            app.sim_static_rinex_dialog = None;
        }
    }

    // Poll interactive simulator file-dialog result.
    if let Some(rx) = &app.sim_interactive_rinex_dialog {
        if let Ok(path) = rx.try_recv() {
            app.sim_interactive_rinex_path = path;
            app.sim_interactive_rinex_dialog = None;
        }
    }

    // Keep repainting while any file-dialog is open so the result is picked
    // up immediately when the OS dialog closes (egui receives no input events
    // while a native dialog has focus).
    if app.sim_rinex_dialog.is_some()
        || app.sim_motion_dialog.is_some()
        || app.route_geojson_dialog.is_some()
        || app.draw_import_dialog.is_some()
        || app.sim_static_rinex_dialog.is_some()
        || app.sim_interactive_rinex_dialog.is_some()
    {
        ctx.request_repaint_after(std::time::Duration::from_millis(50));
    }

    // Poll a pending RINEX download task.
    if let Some(rx) = &app.sim_rinex_download {
        if let Ok(result) = rx.try_recv() {
            match result {
                Ok(path) => {
                    app.sim_rinex_path = Some(path);
                    app.sim_rinex_dl_error = None;
                }
                Err(e) => {
                    app.sim_rinex_dl_error = Some(e);
                }
            }
            app.sim_rinex_download = None;
        }
    }
    if app.sim_rinex_download.is_some() {
        ctx.request_repaint_after(std::time::Duration::from_millis(200));
    }

    // Clean up a finished simulation thread.
    if app
        .sim_thread
        .as_ref()
        .map(|h| h.is_finished())
        .unwrap_or(false)
    {
        if let Some(h) = app.sim_thread.take() {
            h.join().ok();
        }
    }

    // Keep repainting while the simulation thread is alive.
    // Using thread existence (not status) so the final cleanup repaint still
    // fires after the worker sets status=Stopped/Done but before it returns.
    if app.sim_thread.is_some() {
        ctx.request_repaint_after(std::time::Duration::from_millis(150));
    }

    // ── Static GPS Simulator bookkeeping ─────────────────────────────────────

    // Poll a pending RINEX download for the static simulator.
    if let Some(rx) = &app.sim_static_rinex_download {
        if let Ok(result) = rx.try_recv() {
            match result {
                Ok(path) => {
                    app.sim_static_rinex_path = Some(path);
                    app.sim_static_rinex_dl_error = None;
                }
                Err(e) => {
                    app.sim_static_rinex_dl_error = Some(e);
                }
            }
            app.sim_static_rinex_download = None;
        }
    }
    if app.sim_static_rinex_download.is_some() {
        ctx.request_repaint_after(std::time::Duration::from_millis(200));
    }

    // Clean up a finished static simulation thread.
    if app
        .sim_static_thread
        .as_ref()
        .map(|h| h.is_finished())
        .unwrap_or(false)
    {
        if let Some(h) = app.sim_static_thread.take() {
            h.join().ok();
        }
    }

    // Keep repainting while the static simulation thread is alive.
    if app.sim_static_thread.is_some() {
        ctx.request_repaint_after(std::time::Duration::from_millis(150));
    }

    // ── Interactive GPS Simulator bookkeeping ─────────────────────────────────

    // Poll a pending RINEX download for the interactive simulator.
    if let Some(rx) = &app.sim_interactive_rinex_download {
        if let Ok(result) = rx.try_recv() {
            match result {
                Ok(path) => {
                    app.sim_interactive_rinex_path = Some(path);
                    app.sim_interactive_rinex_dl_error = None;
                }
                Err(e) => {
                    app.sim_interactive_rinex_dl_error = Some(e);
                }
            }
            app.sim_interactive_rinex_download = None;
        }
    }
    if app.sim_interactive_rinex_download.is_some() {
        ctx.request_repaint_after(std::time::Duration::from_millis(200));
    }

    // Clean up a finished interactive simulation thread.
    if app
        .sim_interactive_thread
        .as_ref()
        .map(|h| h.is_finished())
        .unwrap_or(false)
    {
        if let Some(h) = app.sim_interactive_thread.take() {
            h.join().ok();
        }
    }

    // Keep repainting while the interactive simulation thread is alive.
    // Using thread existence (not status) ensures the cleanup repaint fires
    // even after the worker sets status=Stopped but before the thread returns.
    if app.sim_interactive_thread.is_some() {
        ctx.request_repaint_after(std::time::Duration::from_millis(100));
    }

    show_menu_bar(app, ctx);
    show_nav_panel(app, ctx);
    show_central_panel(app, ctx);
    show_api_key_dialog(app, ctx);
}

// ---------------------------------------------------------------------------
// Menu bar
// ---------------------------------------------------------------------------

fn show_menu_bar(app: &mut MyApp, ctx: &egui::Context) {
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

// ---------------------------------------------------------------------------
// ORS API key dialog
// ---------------------------------------------------------------------------

fn show_api_key_dialog(app: &mut MyApp, ctx: &egui::Context) {
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

// ---------------------------------------------------------------------------
// Navigation sidebar
// ---------------------------------------------------------------------------

fn show_nav_panel(app: &mut MyApp, ctx: &egui::Context) {
    egui::SidePanel::left("nav_panel")
        .default_width(200.0)
        .show(ctx, |ui| {
            let logo_resp = ui
                .add(
                    egui::Image::new(egui::include_image!("../assets/img/icon-1024.png"))
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
                egui::include_image!("../assets/img/sdr_gps_simulator.png"),
                app.current_mode == AppPage::SdrGpsSimulator,
                "GPS Simulator — transmit GPS L1 C/A signals via HackRF \
                 from a dynamic route or a static position.",
            ) {
                navigate(app, AppPage::SdrGpsSimulator);
            }
            if nav_image_active_with_tooltip(
                ui,
                egui::include_image!("../assets/img/create_umf_route.png"),
                app.current_mode == AppPage::CreateUmfRoute,
                "Create UMF Route — generate a GPS user-motion CSV file \
                 from an ORS route, GeoJSON, a drawn polyline, or a GPX/KML import.",
            ) {
                navigate(app, AppPage::CreateUmfRoute);
            }
            if nav_image_active_with_tooltip(
                ui,
                egui::include_image!("../assets/img/manage_waypoints.png"),
                app.current_mode == AppPage::ManageWaypoints,
                "Manage Waypoints — store and organise named geographic coordinates \
                 to use as route endpoints or static simulation positions.",
            ) {
                navigate(app, AppPage::ManageWaypoints);
            }
            if nav_image_active_with_tooltip(
                ui,
                egui::include_image!("../assets/img/manage_umf_routes.png"),
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
fn navigate(app: &mut MyApp, new_page: AppPage) {
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
fn nav_image_active_with_tooltip(
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

// ---------------------------------------------------------------------------
// Central panel — dispatches to the active page
// ---------------------------------------------------------------------------

#[expect(
    clippy::too_many_lines,
    reason = "central panel dispatches to all pages and applies deferred actions for each — splitting further would obscure the control flow"
)]
fn show_central_panel(app: &mut MyApp, ctx: &egui::Context) {
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

// ---------------------------------------------------------------------------
// Page: Home
// ---------------------------------------------------------------------------

fn show_home_page(ui: &mut egui::Ui) {
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
fn home_card(ui: &mut egui::Ui, title: &str, body: &str) {
    ui.group(|ui| {
        ui.set_width(ui.available_width());
        ui.label(egui::RichText::new(title).strong().size(14.0));
        ui.add_space(4.0);
        ui.label(egui::RichText::new(body).weak());
    });
}

// ---------------------------------------------------------------------------
// Page: GPS Simulator
// ---------------------------------------------------------------------------

fn show_sdr_gps_page(app: &mut MyApp, ui: &mut egui::Ui, ctx: &egui::Context) {
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

#[expect(
    clippy::too_many_lines,
    reason = "dynamic tab: RINEX file group, route library table, map preview with live position, control buttons, and status panel"
)]
fn show_sim_dynamic_tab(app: &mut MyApp, ui: &mut egui::Ui) {
    use std::sync::atomic::Ordering;

    // Ensure the library is loaded (no-op after first call).
    app.load_library();

    ui.add_space(4.0);

    // ── Input files ──────────────────────────────────────────────────────────
    ui.group(|ui| {
        section_title(ui, "Input Files");

        // RINEX Nav File — browse + download buttons.
        let downloading = app.sim_rinex_download.is_some();
        let mut open_browse = false;
        let mut start_download = false;
        ui.horizontal(|ui| {
            ui.label("RINEX Nav File:");
            let display = app
                .sim_rinex_path
                .as_deref()
                .and_then(|p| p.file_name())
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| "None selected".to_owned());
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let browse_label = if app.sim_rinex_dialog.is_some() {
                    "…"
                } else {
                    "Browse…"
                };
                if ui
                    .add_enabled(
                        app.sim_rinex_dialog.is_none(),
                        egui::Button::new(browse_label),
                    )
                    .on_hover_text(
                        "Select a RINEX navigation file (.nav / .23n / .24n …) \
                         containing GPS satellite ephemeris data.",
                    )
                    .clicked()
                {
                    open_browse = true;
                }
                let dl_label = if downloading {
                    "⏳"
                } else {
                    "⬇ Download Latest"
                };
                if ui
                    .add_enabled(!downloading, egui::Button::new(dl_label))
                    .on_hover_text(crate::rinex::today_rinex_filename())
                    .clicked()
                {
                    start_download = true;
                }
                ui.label(egui::RichText::new(display).monospace().weak())
                    .on_hover_text("Currently selected RINEX navigation file.");
            });
        });
        if open_browse {
            app.sim_rinex_dialog = Some(crate::simulator::open_file_dialog(
                "Select RINEX Navigation File",
                &[(
                    "RINEX Navigation",
                    &["nav", "n", "22n", "23n", "24n", "25n", "26n", "27n"],
                )],
                crate::rinex::rinex_dir().ok(),
            ));
        }
        if start_download {
            app.download_rinex();
        }
        if let Some(err) = &app.sim_rinex_dl_error.clone() {
            ui.label(egui::RichText::new(err).color(egui::Color32::RED).small());
        }

        ui.add_space(2.0);

        if sim_file_row(
            ui,
            "Motion CSV (ECEF)",
            &app.sim_motion_path,
            app.sim_motion_dialog.is_some(),
        ) {
            app.sim_motion_dialog = Some(crate::simulator::open_file_dialog(
                "Select User Motion File (ECEF x,y,z CSV)",
                &[("CSV files", &["csv"])],
                crate::paths::umf_dir().ok(),
            ));
        }
    });

    ui.add_space(6.0);

    // ── Route library ─────────────────────────────────────────────────────────
    let running = app.sim_thread.is_some();
    ui.add_enabled_ui(!running, |ui| {
        ui.group(|ui| {
            section_title(ui, "Route Library");

            if app.library.is_empty() {
                ui.label(
                    egui::RichText::new("No routes in library. Go to Manage UMF Routes to scan.")
                        .weak(),
                );
            } else {
                let mut route_to_load: Option<usize> = None;

                egui::ScrollArea::vertical()
                    .id_salt("sim_dyn_lib_scroll")
                    .max_height(160.0)
                    .show(ui, |ui| {
                        egui_extras::TableBuilder::new(ui)
                            .column(Column::initial(160.0).at_least(100.0)) // Name
                            .column(Column::initial(90.0).at_least(70.0)) // Distance
                            .column(Column::initial(90.0).at_least(70.0)) // Duration
                            .column(Column::initial(90.0).at_least(70.0)) // Velocity
                            .sense(egui::Sense::click())
                            .resizable(true)
                            .striped(true)
                            .header(22.0, |mut row| {
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
                            })
                            .body(|mut body| {
                                for (i, entry) in app.library.iter().enumerate() {
                                    body.row(22.0, |mut row| {
                                        row.set_selected(app.sim_lib_selected_row == Some(i));
                                        row.col(|ui| {
                                            ui.label(&entry.name);
                                        });
                                        row.col(|ui| {
                                            ui.label(format!(
                                                "{:.2} km",
                                                entry.distance_m / 1000.0
                                            ));
                                        });
                                        row.col(|ui| {
                                            ui.label(format_duration(entry.duration_s));
                                        });
                                        row.col(|ui| {
                                            ui.label(format!("{:.1} km/h", entry.velocity_kmh));
                                        });
                                        if row.response().clicked() {
                                            route_to_load = Some(i);
                                        }
                                    });
                                }
                            });
                    });

                if let Some(i) = route_to_load {
                    app.sim_lib_selected_row = Some(i);
                    app.load_sim_lib_route(i);
                }
            }
        })
    });

    ui.add_space(6.0);

    // ── Route preview / live-tracking map ─────────────────────────────────────
    if !app.sim_lib_route_points.is_empty() {
        let state = match app.sim_state.lock() {
            Ok(g) => g.clone(),
            Err(_) => crate::simulator::SimState::default(),
        };
        let running =
            app.sim_thread.is_some() || state.status == crate::simulator::SimStatus::Running;

        // Compute the current geographic position from simulation progress.
        let current_pos: Option<walkers::Position> = if running || state.total_steps > 0 {
            interpolate_route_pos(
                &app.sim_lib_route_points,
                state.current_step,
                state.total_steps,
            )
        } else {
            None
        };

        // While running, keep the map centred on the moving marker.
        if running {
            if let Some(pos) = current_pos {
                app.sim_lib_map_memory.center_at(pos);
            }
        }

        if app.sim_lib_map_tiles.is_none() {
            app.sim_lib_map_tiles = Some(HttpTiles::new(OpenStreetMap, ui.ctx().clone()));
        }

        let route_pts: Vec<walkers::Position> = app.sim_lib_route_points.clone();
        let marker_pts: Vec<(walkers::Position, egui::Color32)> = current_pos
            .map(|p| vec![(p, egui::Color32::from_rgb(0, 180, 255))])
            .unwrap_or_default();

        let map = Map::new(
            app.sim_lib_map_tiles
                .as_mut()
                .map(|t| t as &mut dyn walkers::Tiles),
            &mut app.sim_lib_map_memory,
            app.sim_lib_route_points
                .first()
                .copied()
                .unwrap_or_else(|| lat_lon(52.37308687621991, 4.893432625781817)),
        )
        .with_plugin(RouteLinePlugin { points: &route_pts })
        .with_plugin(WaypointMarkerPlugin {
            markers: &marker_pts,
        });

        let w = ui.available_width();
        let map_response = ui.add_sized([w, 260.0], map);
        add_map_zoom_controls(
            ui.ctx(),
            map_response.rect,
            "sim_dyn_map_zoom",
            &mut app.sim_lib_map_memory,
        );
    }

    ui.add_space(6.0);

    // ── Control buttons ──────────────────────────────────────────────────────
    let ready = app.sim_rinex_path.is_some() && app.sim_motion_path.is_some() && !running;

    ui.horizontal(|ui| {
        ui.add_enabled_ui(ready, |ui| {
            if ui
                .button(egui::RichText::new("  ▶  Start Simulation  ").size(15.0))
                .on_hover_text(
                    "Begin transmitting the GPS route on the HackRF. \
                     Requires a RINEX nav file and a Motion CSV to be selected.",
                )
                .clicked()
            {
                app.start_simulation();
            }
        });

        if running {
            let paused = app.sim_pause_flag.load(Ordering::Relaxed);
            let pause_label = if paused {
                egui::RichText::new("  ▶  Resume  ").size(15.0)
            } else {
                egui::RichText::new("  ⏸  Pause  ").size(15.0)
            };
            if ui
                .button(pause_label)
                .on_hover_text(if paused {
                    "Resume route playback from the current position."
                } else {
                    "Pause the route: hold position and keep transmitting GPS signal."
                })
                .clicked()
            {
                app.sim_pause_flag.store(!paused, Ordering::Relaxed);
            }

            if ui
                .button(egui::RichText::new("  ■  Stop  ").size(15.0))
                .on_hover_text("Stop the running simulation and release the HackRF device.")
                .clicked()
            {
                app.sim_stop_flag.store(true, Ordering::Relaxed);
            }
        }
    });

    ui.add_space(8.0);

    // ── Status panel ─────────────────────────────────────────────────────────
    ui.group(|ui| {
        section_title(ui, "Status");

        let state = match app.sim_state.lock() {
            Ok(guard) => guard.clone(),
            Err(_) => crate::simulator::SimState::default(),
        };

        let paused = app.sim_pause_flag.load(Ordering::Relaxed);
        let (status_text, status_colour) = match &state.status {
            crate::simulator::SimStatus::Idle => ("Idle", egui::Color32::GRAY),
            crate::simulator::SimStatus::Running if paused => {
                ("Paused at current position", egui::Color32::GOLD)
            }
            crate::simulator::SimStatus::Running => ("Running…", egui::Color32::GREEN),
            crate::simulator::SimStatus::Done => ("Done", egui::Color32::LIGHT_BLUE),
            crate::simulator::SimStatus::Stopped => ("Stopped by user", egui::Color32::GOLD),
            crate::simulator::SimStatus::Error => ("Error", egui::Color32::RED),
        };
        ui.label(egui::RichText::new(status_text).color(status_colour));

        if let Some(err) = &state.error {
            ui.horizontal(|ui| {
                ui.colored_label(egui::Color32::RED, err);
                if ui
                    .small_button("Copy")
                    .on_hover_text("Copy error message to clipboard.")
                    .clicked()
                {
                    ui.output_mut(|o| o.commands.push(egui::OutputCommand::CopyText(err.clone())));
                }
            });
        }

        let progress = if state.total_steps > 0 {
            state.current_step as f32 / state.total_steps as f32
        } else {
            0.0
        };
        ui.add(
            egui::ProgressBar::new(progress)
                .text(format!(
                    "{:.0}%  ({:.1} s / {:.1} s)",
                    progress * 100.0,
                    state.current_step as f64 / 10.0,
                    state.total_steps as f64 / 10.0,
                ))
                .desired_width(500.0),
        )
        .on_hover_text("Simulation progress: elapsed time / total route duration.");

        ui.label(format!(
            "Bytes transmitted: {:.2} MB",
            state.bytes_sent as f64 / 1_000_000.0
        ))
        .on_hover_text("Total IQ data sent to the HackRF USB bulk endpoint.");

        if state.status == crate::simulator::SimStatus::Running
            || state.status == crate::simulator::SimStatus::Done
        {
            if state.lat_deg != 0.0 || state.lon_deg != 0.0 {
                ui.label(format!(
                    "Position: {:.6}°, {:.6}°  alt {:.1} m",
                    state.lat_deg, state.lon_deg, state.height_m
                ))
                .on_hover_text("Most recent simulated receiver position (lat, lon, height).");
            }

            if !state.satellites.is_empty() {
                ui.add_space(4.0);
                ui.label(format!("Satellites in view: {}", state.satellites.len()));
                egui::Grid::new("dyn_sat_table")
                    .striped(true)
                    .min_col_width(60.0)
                    .show(ui, |ui| {
                        ui.label(egui::RichText::new("PRN").strong());
                        ui.label(egui::RichText::new("Azimuth").strong());
                        ui.label(egui::RichText::new("Elevation").strong());
                        ui.end_row();
                        for sat in &state.satellites {
                            ui.label(format!("G{:02}", sat.prn));
                            ui.label(format!("{:.1}°", sat.az_deg));
                            ui.label(format!("{:.1}°", sat.el_deg));
                            ui.end_row();
                        }
                    });
            }
        }
    });
}

/// Interpolates along `points` by arc length based on `current_step / total_steps`.
///
/// Builds a cumulative distance table so the progress fraction maps to actual
/// distance along the route rather than to array index. This is required because
/// ORS waypoints are not evenly spaced: dense urban areas have many short
/// segments close together, so a naive index-based mapping makes the marker
/// appear to crawl there and race through sparse rural sections.
///
/// Returns `None` when `points` is empty or `total_steps` is zero.
fn interpolate_route_pos(
    points: &[walkers::Position],
    current_step: usize,
    total_steps: usize,
) -> Option<walkers::Position> {
    if points.is_empty() || total_steps == 0 {
        return None;
    }
    if points.len() == 1 {
        return points.first().copied();
    }

    // Cumulative arc-length table (one entry per waypoint).
    // Uses an equirectangular approximation — accurate enough for a map marker.
    let mut cum: Vec<f64> = Vec::with_capacity(points.len());
    cum.push(0.0);
    for w in points.windows(2) {
        if let [a, b] = w {
            let dlat = b.y() - a.y();
            // Longitude degrees shrink with latitude → correct with cos(mid_lat).
            let dlon = (b.x() - a.x()) * ((a.y() + b.y()) * 0.5).to_radians().cos();
            let prev = cum.last().copied().unwrap_or(0.0);
            cum.push(prev + dlat.hypot(dlon));
        }
    }

    let total = cum.last().copied().unwrap_or(0.0);
    if total == 0.0 {
        return points.first().copied();
    }

    #[expect(
        clippy::cast_precision_loss,
        reason = "step counts fit in f64 without meaningful precision loss at realistic route lengths"
    )]
    let target = (current_step as f64 / total_steps as f64).clamp(0.0, 1.0) * total;

    // Binary-search for the segment that straddles `target`.
    let i = cum
        .partition_point(|&d| d <= target)
        .saturating_sub(1)
        .min(points.len() - 2);

    let (Some(a), Some(b)) = (points.get(i), points.get(i + 1)) else {
        return points.last().copied();
    };
    let seg_len = cum.get(i + 1).copied().unwrap_or(0.0) - cum.get(i).copied().unwrap_or(0.0);
    let frac = if seg_len > 0.0 {
        (target - cum.get(i).copied().unwrap_or(0.0)) / seg_len
    } else {
        0.0
    };

    Some(lat_lon(
        a.y() + (b.y() - a.y()) * frac,
        a.x() + (b.x() - a.x()) * frac,
    ))
}

#[expect(
    clippy::too_many_lines,
    reason = "static tab: RINEX file group, waypoint picker, map, position group, control buttons, and status panel"
)]
fn show_sim_static_tab(app: &mut MyApp, ui: &mut egui::Ui) {
    use std::sync::atomic::Ordering;

    ui.add_space(4.0);

    // ── RINEX nav file ────────────────────────────────────────────────────────
    ui.group(|ui| {
        section_title(ui, "Input File");

        let downloading = app.sim_static_rinex_download.is_some();
        let mut open_browse = false;
        let mut start_download = false;

        ui.horizontal(|ui| {
            ui.label("RINEX Nav File:");
            let display = app
                .sim_static_rinex_path
                .as_deref()
                .and_then(|p| p.file_name())
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| "None selected".to_owned());
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let browse_label = if app.sim_static_rinex_dialog.is_some() {
                    "…"
                } else {
                    "Browse…"
                };
                if ui
                    .add_enabled(
                        app.sim_static_rinex_dialog.is_none(),
                        egui::Button::new(browse_label),
                    )
                    .on_hover_text(
                        "Select a RINEX navigation file (.nav / .23n / .24n …) \
                         containing GPS satellite ephemeris data.",
                    )
                    .clicked()
                {
                    open_browse = true;
                }
                let dl_label = if downloading {
                    "⏳"
                } else {
                    "⬇ Download Latest"
                };
                if ui
                    .add_enabled(!downloading, egui::Button::new(dl_label))
                    .on_hover_text(crate::rinex::today_rinex_filename())
                    .clicked()
                {
                    start_download = true;
                }
                ui.label(egui::RichText::new(display).monospace().weak())
                    .on_hover_text("Currently selected RINEX navigation file.");
            });
        });
        if open_browse {
            app.sim_static_rinex_dialog = Some(crate::simulator::open_file_dialog(
                "Select RINEX Navigation File",
                &[(
                    "RINEX Navigation",
                    &["nav", "n", "22n", "23n", "24n", "25n", "26n", "27n"],
                )],
                crate::rinex::rinex_dir().ok(),
            ));
        }
        if start_download {
            app.download_rinex_static();
        }
        if let Some(err) = &app.sim_static_rinex_dl_error.clone() {
            ui.label(egui::RichText::new(err).color(egui::Color32::RED).small());
        }
    });

    ui.add_space(8.0);

    // ── Waypoint picker ───────────────────────────────────────────────────────
    // Lazily load waypoints (safe to call repeatedly; guard is inside load_waypoints).
    if !app.waypoints_loaded {
        app.load_waypoints();
    }

    // Snapshot to avoid borrow conflicts inside egui closures.
    let waypoints_snap: Vec<crate::waypoint::Waypoint> = app.waypoints.clone();
    let current_selected = app.sim_static_wp_selected_row;
    let mut new_selected: Option<usize> = None;

    ui.group(|ui| {
        section_title(ui, "Select from Waypoints");

        egui::ScrollArea::vertical()
            .id_salt("sim_static_wp_scroll")
            .max_height(180.0)
            .show(ui, |ui| {
                egui_extras::TableBuilder::new(ui)
                    .column(Column::initial(160.0).at_least(80.0)) // Name
                    .column(Column::initial(160.0).at_least(80.0)) // Location
                    .column(Column::initial(130.0).at_least(80.0)) // Category
                    .sense(egui::Sense::click())
                    .resizable(true)
                    .striped(true)
                    .header(24.0, |mut row| {
                        row.col(|ui| {
                            ui.strong("Name");
                        });
                        row.col(|ui| {
                            ui.strong("Location");
                        });
                        row.col(|ui| {
                            ui.strong("Category");
                        });
                    })
                    .body(|mut body| {
                        for (idx, wp) in waypoints_snap.iter().enumerate() {
                            body.row(22.0, |mut row| {
                                row.set_selected(current_selected == Some(idx));
                                row.col(|ui| {
                                    ui.label(&wp.name);
                                });
                                row.col(|ui| {
                                    ui.label(&wp.location);
                                });
                                row.col(|ui| {
                                    ui.label(&wp.category);
                                });
                                if row.response().clicked() {
                                    new_selected = Some(idx);
                                }
                            });
                        }
                    });
            });
    });

    // Apply row selection: fill position fields and centre the map.
    if let Some(idx) = new_selected {
        if let Some(wp) = waypoints_snap.get(idx) {
            app.sim_static_wp_selected_row = Some(idx);
            app.sim_static_lat = format!("{:.6}", wp.lat);
            app.sim_static_lon = format!("{:.6}", wp.lon);
            app.sim_static_map_memory.center_at(lat_lon(wp.lat, wp.lon));
        }
    }

    ui.add_space(4.0);

    // ── Waypoint map ──────────────────────────────────────────────────────────
    if app.sim_static_map_tiles.is_none() {
        app.sim_static_map_tiles = Some(HttpTiles::new(OpenStreetMap, ui.ctx().clone()));
    }

    let marker: Vec<(walkers::Position, egui::Color32)> = app
        .sim_static_wp_selected_row
        .and_then(|i| waypoints_snap.get(i))
        .map(|wp| {
            vec![(
                lat_lon(wp.lat, wp.lon),
                egui::Color32::from_rgb(70, 150, 255),
            )]
        })
        .unwrap_or_default();

    let my_pos = lat_lon(52.373_086_876_219_91, 4.893_432_625_781_817); // Amsterdam fallback
    let sim_static_map = Map::new(
        app.sim_static_map_tiles
            .as_mut()
            .map(|t| t as &mut dyn walkers::Tiles),
        &mut app.sim_static_map_memory,
        my_pos,
    )
    .with_plugin(ClickCapturePlugin {
        out: &mut app.sim_static_map_clicked,
    })
    .with_plugin(WaypointMarkerPlugin { markers: &marker });

    let available_width = ui.available_width();
    let map_resp = ui.add_sized([available_width, 250.0], sim_static_map);
    add_map_zoom_controls(
        ui.ctx(),
        map_resp.rect,
        "sim_static_map_zoom",
        &mut app.sim_static_map_memory,
    );

    // A click on the map fills the position fields (deselects table row).
    if let Some(click) = app.sim_static_map_clicked.take() {
        app.sim_static_lat = format!("{:.6}", click.position.y());
        app.sim_static_lon = format!("{:.6}", click.position.x());
        app.sim_static_wp_selected_row = None;
    }

    ui.add_space(8.0);

    // ── Static position ───────────────────────────────────────────────────────
    let running = app.sim_static_thread.is_some();
    ui.add_enabled_ui(!running, |ui| {
        ui.group(|ui| {
            section_title(ui, "Static Position");

            ui.horizontal(|ui| {
                ui.label("Latitude (°): ");
                ui.text_edit_singleline(&mut app.sim_static_lat)
                    .on_hover_text("WGS-84 latitude in decimal degrees, e.g. 52.3702");
            });
            ui.horizontal(|ui| {
                ui.label("Longitude (°):");
                ui.text_edit_singleline(&mut app.sim_static_lon)
                    .on_hover_text("WGS-84 longitude in decimal degrees, e.g. 4.8952");
            });
            ui.horizontal(|ui| {
                ui.label("Altitude (m): ");
                ui.text_edit_singleline(&mut app.sim_static_alt)
                    .on_hover_text("Height above WGS-84 ellipsoid in metres");
            });
            ui.horizontal(|ui| {
                ui.label("Loop duration:");
                ui.add(
                    egui::DragValue::new(&mut app.sim_static_loop_duration)
                        .range(30.0..=3600.0)
                        .speed(10.0)
                        .suffix(" s"),
                )
                .on_hover_text(
                    "Duration of each simulation pass before the loop restarts.\n\
                     GPS receivers need ≥ 30 s to acquire a signal.\n\
                     Recommended: ≥ 300 s.",
                );
            });
        });
    });

    ui.add_space(8.0);

    // ── Control buttons ───────────────────────────────────────────────────────
    let lat_ok =
        !app.sim_static_lat.trim().is_empty() && app.sim_static_lat.trim().parse::<f64>().is_ok();
    let lon_ok =
        !app.sim_static_lon.trim().is_empty() && app.sim_static_lon.trim().parse::<f64>().is_ok();
    let ready = app.sim_static_rinex_path.is_some() && lat_ok && lon_ok && !running;

    ui.horizontal(|ui| {
        ui.add_enabled_ui(ready, |ui| {
            if ui
                .button(egui::RichText::new("  ▶  Start Loop  ").size(15.0))
                .on_hover_text(
                    "Streams the static position indefinitely, restarting every loop pass.",
                )
                .clicked()
            {
                app.start_static_simulation();
            }
        });

        if running
            && ui
                .button(egui::RichText::new("  ■  Stop  ").size(15.0))
                .on_hover_text("Stop the looping simulation and release the HackRF device.")
                .clicked()
        {
            app.sim_static_stop_flag.store(true, Ordering::Relaxed);
        }
    });

    if !lat_ok || !lon_ok {
        ui.label(
            egui::RichText::new("Enter a valid latitude and longitude to enable start.")
                .small()
                .color(egui::Color32::YELLOW),
        );
    }

    ui.add_space(8.0);

    // ── Status panel ──────────────────────────────────────────────────────────
    ui.group(|ui| {
        section_title(ui, "Status");

        let state = match app.sim_static_state.lock() {
            Ok(guard) => guard.clone(),
            Err(_) => crate::simulator::SimState::default(),
        };

        let (status_text, status_colour) = match &state.status {
            crate::simulator::SimStatus::Idle => ("Idle", egui::Color32::GRAY),
            crate::simulator::SimStatus::Running => ("Running (looping)…", egui::Color32::GREEN),
            crate::simulator::SimStatus::Done => ("Done", egui::Color32::LIGHT_BLUE),
            crate::simulator::SimStatus::Stopped => ("Stopped by user", egui::Color32::GOLD),
            crate::simulator::SimStatus::Error => ("Error", egui::Color32::RED),
        };
        ui.label(egui::RichText::new(status_text).color(status_colour));

        if state.loop_count > 0 {
            ui.label(format!("Loop pass: {}", state.loop_count))
                .on_hover_text("Number of completed loop passes since the simulation started.");
        }

        if let Some(err) = &state.error {
            ui.horizontal(|ui| {
                ui.colored_label(egui::Color32::RED, err);
                if ui
                    .small_button("Copy")
                    .on_hover_text("Copy error message to clipboard.")
                    .clicked()
                {
                    ui.output_mut(|o| o.commands.push(egui::OutputCommand::CopyText(err.clone())));
                }
            });
        }

        let progress = if state.total_steps > 0 {
            state.current_step as f32 / state.total_steps as f32
        } else {
            0.0
        };
        ui.add(
            egui::ProgressBar::new(progress)
                .text(format!(
                    "{:.0}%  ({:.1} s / {:.1} s)",
                    progress * 100.0,
                    state.current_step as f64 / 10.0,
                    state.total_steps as f64 / 10.0,
                ))
                .desired_width(500.0),
        )
        .on_hover_text("Progress through the current loop pass: elapsed / loop duration.");

        ui.label(format!(
            "Bytes transmitted: {:.2} MB",
            state.bytes_sent as f64 / 1_000_000.0
        ))
        .on_hover_text("Total IQ data sent to the HackRF USB bulk endpoint.");

        if state.status == crate::simulator::SimStatus::Running
            || state.status == crate::simulator::SimStatus::Done
        {
            if state.lat_deg != 0.0 || state.lon_deg != 0.0 {
                ui.label(format!(
                    "Position: {:.6}°, {:.6}°  alt {:.1} m",
                    state.lat_deg, state.lon_deg, state.height_m
                ))
                .on_hover_text("Most recent simulated receiver position (lat, lon, height).");
            }

            if !state.satellites.is_empty() {
                ui.add_space(4.0);
                ui.label(format!("Satellites in view: {}", state.satellites.len()));
                egui::Grid::new("static_sat_table")
                    .striped(true)
                    .min_col_width(60.0)
                    .show(ui, |ui| {
                        ui.label(egui::RichText::new("PRN").strong());
                        ui.label(egui::RichText::new("Azimuth").strong());
                        ui.label(egui::RichText::new("Elevation").strong());
                        ui.end_row();
                        for sat in &state.satellites {
                            ui.label(format!("G{:02}", sat.prn));
                            ui.label(format!("{:.1}°", sat.az_deg));
                            ui.label(format!("{:.1}°", sat.el_deg));
                            ui.end_row();
                        }
                    });
            }
        }
    });
}

// ── Interactive simulator tab ──────────────────────────────────────────────────

/// Compute the initial bearing (degrees, 0–360) from one geographic point to another.
fn geodetic_bearing(from_lat: f64, from_lon: f64, to_lat: f64, to_lon: f64) -> f64 {
    let lat1 = from_lat.to_radians();
    let lat2 = to_lat.to_radians();
    let dlon = (to_lon - from_lon).to_radians();
    let y = dlon.sin() * lat2.cos();
    let x = lat1.cos() * lat2.sin() - lat1.sin() * lat2.cos() * dlon.cos();
    y.atan2(x).to_degrees().rem_euclid(360.0)
}

/// Draws an interactive compass-rose bearing dial.
///
/// The filled dot on the circle perimeter represents the current heading; the
/// user can drag it (or click anywhere on the circle) to set a new bearing.
/// The current bearing value is painted in the centre.
///
/// Returns `true` when the user has changed the bearing this frame.
fn bearing_dial(ui: &mut egui::Ui, bearing_deg: &mut f64, enabled: bool) -> bool {
    let sense = if enabled {
        egui::Sense::click_and_drag()
    } else {
        egui::Sense::hover()
    };
    let (rect, response) = ui.allocate_exact_size(egui::Vec2::splat(140.0), sense);

    if ui.is_rect_visible(rect) {
        let painter = ui.painter_at(rect);
        let center = rect.center();
        let r = rect.width().min(rect.height()) * 0.45;

        let (ring_col, dot_col, text_col, card_col) = if enabled {
            (
                egui::Color32::from_gray(140),
                egui::Color32::from_rgb(70, 150, 255),
                egui::Color32::WHITE,
                egui::Color32::from_gray(190),
            )
        } else {
            (
                egui::Color32::from_gray(70),
                egui::Color32::from_gray(100),
                egui::Color32::from_gray(120),
                egui::Color32::from_gray(100),
            )
        };

        // Background fill and outer ring.
        painter.circle_filled(center, r + 2.0, egui::Color32::from_gray(28));
        painter.circle_stroke(center, r, egui::Stroke::new(2.0, ring_col));

        // Cardinal tick marks and labels (N / E / S / W).
        for (label, deg) in [("N", 0.0_f64), ("E", 90.0), ("S", 180.0), ("W", 270.0)] {
            let rad = deg.to_radians();
            let sdx = rad.sin() as f32;
            let sdy = (-rad.cos()) as f32;
            painter.line_segment(
                [
                    center + egui::vec2(sdx * (r - 8.0), sdy * (r - 8.0)),
                    center + egui::vec2(sdx * r, sdy * r),
                ],
                egui::Stroke::new(1.5, ring_col),
            );
            painter.text(
                center + egui::vec2(sdx * (r - 19.0), sdy * (r - 19.0)),
                egui::Align2::CENTER_CENTER,
                label,
                egui::FontId::proportional(10.0),
                card_col,
            );
        }

        // Bearing indicator: line from centre to dot on perimeter.
        let b_rad = bearing_deg.to_radians();
        let dot = center + egui::vec2((b_rad.sin() as f32) * r, ((-b_rad.cos()) as f32) * r);
        painter.line_segment([center, dot], egui::Stroke::new(2.0, dot_col));
        painter.circle_filled(dot, 7.0, dot_col);

        // Bearing value in the centre.
        painter.text(
            center,
            egui::Align2::CENTER_CENTER,
            format!("{:.1}°", *bearing_deg),
            egui::FontId::monospace(13.0),
            text_col,
        );
    }

    // Interaction: drag or click anywhere on the dial → recompute bearing.
    let mut changed = false;
    if enabled && (response.dragged() || response.clicked()) {
        if let Some(pos) = response.interact_pointer_pos() {
            let delta = pos - rect.center();
            if delta.length() > 4.0 {
                *bearing_deg = f64::from(delta.x)
                    .atan2(f64::from(-delta.y))
                    .to_degrees()
                    .rem_euclid(360.0);
                changed = true;
            }
        }
    }
    changed
}

#[expect(
    clippy::too_many_lines,
    reason = "interactive tab: RINEX picker, position, motion buttons, map widget, controls, status, satellite table"
)]
fn show_sim_interactive_tab(app: &mut MyApp, ui: &mut egui::Ui, ctx: &egui::Context) {
    let running = app.sim_interactive_thread.is_some();

    // ── Process key events while running ─────────────────────────────────────
    if running {
        #[expect(
            clippy::unwrap_used,
            reason = "mutex poison means a prior panic; best-effort key update"
        )]
        let mut ist = app.sim_interactive_istate.lock().unwrap();
        ctx.input(|i| {
            // Bearing (A = left / D = right), continuous while key is held.
            if i.key_down(egui::Key::A) {
                ist.bearing_deg -= 1.0;
            }
            if i.key_down(egui::Key::D) {
                ist.bearing_deg += 1.0;
            }
            ist.bearing_deg = ist.bearing_deg.rem_euclid(360.0);

            // Speed (E = faster / Q = slower), per key-press.
            if i.key_pressed(egui::Key::E) {
                ist.speed_ms += 1.0;
            }
            if i.key_pressed(egui::Key::Q) {
                ist.speed_ms = (ist.speed_ms - 1.0).max(0.0);
            }

            // Vertical speed (W = up / S = down), per key-press.
            if i.key_pressed(egui::Key::W) {
                ist.vert_speed_ms += 1.0;
            }
            if i.key_pressed(egui::Key::S) {
                ist.vert_speed_ms -= 1.0;
            }

            // Stop (X key).
            if i.key_pressed(egui::Key::X) {
                app.sim_interactive_stop_flag
                    .store(true, std::sync::atomic::Ordering::Relaxed);
            }
        });
    }

    ui.add_space(4.0);

    // ── RINEX file ────────────────────────────────────────────────────────────
    let downloading = app.sim_interactive_rinex_download.is_some();
    ui.group(|ui| {
        section_title(ui, "Input File");

        let mut open_browse = false;
        let mut start_download = false;

        ui.horizontal(|ui| {
            ui.label("RINEX Nav File:");
            let display = app
                .sim_interactive_rinex_path
                .as_deref()
                .and_then(|p| p.file_name())
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| "None selected".to_owned());
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let browse_label = if app.sim_interactive_rinex_dialog.is_some() {
                    "…"
                } else {
                    "Browse…"
                };
                if ui
                    .add_enabled(
                        app.sim_interactive_rinex_dialog.is_none(),
                        egui::Button::new(browse_label),
                    )
                    .on_hover_text(
                        "Select a RINEX navigation file (.nav / .23n / .24n …) \
                         containing GPS satellite ephemeris data.",
                    )
                    .clicked()
                {
                    open_browse = true;
                }
                let dl_label = if downloading {
                    "⏳"
                } else {
                    "⬇ Download Latest"
                };
                if ui
                    .add_enabled(!downloading, egui::Button::new(dl_label))
                    .on_hover_text(crate::rinex::today_rinex_filename())
                    .clicked()
                {
                    start_download = true;
                }
                ui.label(egui::RichText::new(display).monospace().weak())
                    .on_hover_text("Currently selected RINEX navigation file.");
            });
        });
        if open_browse {
            app.sim_interactive_rinex_dialog = Some(crate::simulator::open_file_dialog(
                "Select RINEX Navigation File",
                &[(
                    "RINEX Navigation",
                    &["nav", "n", "22n", "23n", "24n", "25n", "26n", "27n"],
                )],
                crate::rinex::rinex_dir().ok(),
            ));
        }
        if start_download {
            app.download_rinex_interactive();
        }
        if let Some(err) = &app.sim_interactive_rinex_dl_error.clone() {
            ui.label(egui::RichText::new(err).color(egui::Color32::RED).small());
        }
    });

    ui.add_space(8.0);

    // ── Starting position ─────────────────────────────────────────────────────
    ui.add_enabled_ui(!running, |ui| {
        ui.group(|ui| {
            section_title(ui, "Starting Position");

            ui.horizontal(|ui| {
                ui.label("Latitude (°): ");
                ui.text_edit_singleline(&mut app.sim_interactive_lat)
                    .on_hover_text("WGS-84 latitude in decimal degrees, e.g. 52.3702");
            });
            ui.horizontal(|ui| {
                ui.label("Longitude (°):");
                ui.text_edit_singleline(&mut app.sim_interactive_lon)
                    .on_hover_text("WGS-84 longitude in decimal degrees, e.g. 4.8952");
            });
            ui.horizontal(|ui| {
                ui.label("Altitude (m): ");
                ui.text_edit_singleline(&mut app.sim_interactive_alt)
                    .on_hover_text("Height above WGS-84 ellipsoid in metres");
            });
        });
    });

    ui.add_space(8.0);

    // ── Motion controls ───────────────────────────────────────────────────────
    {
        // Snapshot taken after keyboard processing so the widgets show
        // keyboard-updated values each frame.
        #[expect(
            clippy::unwrap_used,
            reason = "mutex poison means a prior panic; best-effort display"
        )]
        let ist_snap = app.sim_interactive_istate.lock().unwrap().clone();

        let mut bearing_deg = ist_snap.bearing_deg;
        let mut speed_kmh = ist_snap.speed_ms * 3.6;
        let mut vert_speed_ms = ist_snap.vert_speed_ms;
        let mut stop_motion = false;

        let mut bearing_changed = false;
        let mut speed_changed = false;
        let mut vert_changed = false;

        ui.group(|ui| {
            section_title(ui, "Motion Controls");

            ui.horizontal(|ui| {
                // ── Bearing dial (left) ───────────────────────────────────────
                ui.vertical(|ui| {
                    ui.label("Bearing");
                    if bearing_dial(ui, &mut bearing_deg, running) {
                        bearing_changed = true;
                    }
                });

                ui.add_space(16.0);

                // ── Speed + vertical sliders (right) ──────────────────────────
                ui.vertical(|ui| {
                    ui.label("Speed:");
                    if ui
                        .add_enabled(
                            running,
                            egui::Slider::new(&mut speed_kmh, 0.0..=300.0).suffix(" km/h"),
                        )
                        .changed()
                    {
                        speed_changed = true;
                    }

                    ui.add_space(8.0);

                    ui.label("Vertical speed:");
                    if ui
                        .add_enabled(
                            running,
                            egui::Slider::new(&mut vert_speed_ms, -50.0..=50.0).suffix(" m/s"),
                        )
                        .changed()
                    {
                        vert_changed = true;
                    }

                    ui.add_space(12.0);

                    if ui
                        .add_enabled(running, egui::Button::new("■  Stop all motion"))
                        .on_hover_text("Set speed and vertical speed to zero.")
                        .clicked()
                    {
                        stop_motion = true;
                    }
                });
            });

            ui.add_space(4.0);
            ui.label(
                egui::RichText::new(
                    "Keyboard: A/D = bearing ±1°, E/Q = speed ±1 m/s, W/S = vertical ±1 m/s",
                )
                .small()
                .weak(),
            );
        });

        // Apply widget-driven changes to the shared motion state.
        if running && (bearing_changed || speed_changed || vert_changed || stop_motion) {
            #[expect(
                clippy::unwrap_used,
                reason = "mutex poison means a prior panic; best-effort motion update"
            )]
            let mut ist = app.sim_interactive_istate.lock().unwrap();
            if bearing_changed {
                ist.bearing_deg = bearing_deg;
            }
            if speed_changed {
                ist.speed_ms = speed_kmh / 3.6;
            }
            if vert_changed {
                ist.vert_speed_ms = vert_speed_ms;
            }
            if stop_motion {
                ist.speed_ms = 0.0;
                ist.vert_speed_ms = 0.0;
            }
        }
    }

    ui.add_space(8.0);

    // ── Control buttons ───────────────────────────────────────────────────────
    let lat_ok = !app.sim_interactive_lat.trim().is_empty()
        && app.sim_interactive_lat.trim().parse::<f64>().is_ok();
    let lon_ok = !app.sim_interactive_lon.trim().is_empty()
        && app.sim_interactive_lon.trim().parse::<f64>().is_ok();
    let ready = app.sim_interactive_rinex_path.is_some() && lat_ok && lon_ok && !running;

    ui.horizontal(|ui| {
        ui.add_enabled_ui(ready, |ui| {
            if ui
                .button(egui::RichText::new("  ▶  Start Interactive  ").size(15.0))
                .on_hover_text(
                    "Start the interactive GPS simulation. \
                     Use the buttons above or keyboard to steer the receiver.",
                )
                .clicked()
            {
                app.start_interactive_simulation();
            }
        });

        if running
            && ui
                .button(egui::RichText::new("  ■  Stop  ").size(15.0))
                .on_hover_text("Stop the interactive simulation and release the HackRF device.")
                .clicked()
        {
            app.sim_interactive_stop_flag
                .store(true, std::sync::atomic::Ordering::Relaxed);
        }
    });

    if !lat_ok || !lon_ok {
        ui.label(
            egui::RichText::new("Enter a valid latitude and longitude to enable start.")
                .small()
                .color(egui::Color32::YELLOW),
        );
    }

    ui.add_space(8.0);

    // ── Live map ──────────────────────────────────────────────────────────────
    {
        let map_state = match app.sim_interactive_state.lock() {
            Ok(guard) => guard.clone(),
            Err(_) => crate::simulator::SimState::default(),
        };

        let live_pos: Option<walkers::Position> =
            if map_state.lat_deg != 0.0 || map_state.lon_deg != 0.0 {
                Some(lat_lon(map_state.lat_deg, map_state.lon_deg))
            } else {
                None
            };

        let center_pos = live_pos.unwrap_or_else(|| {
            let lat = app
                .sim_interactive_lat
                .trim()
                .parse::<f64>()
                .unwrap_or(52.373_086);
            let lon = app
                .sim_interactive_lon
                .trim()
                .parse::<f64>()
                .unwrap_or(4.893_433);
            lat_lon(lat, lon)
        });

        // Follow live position while running.
        if running {
            if let Some(pos) = live_pos {
                app.sim_interactive_map_memory.center_at(pos);
            }
        }

        let markers: Vec<(walkers::Position, egui::Color32)> = live_pos
            .map(|p| vec![(p, egui::Color32::from_rgb(70, 150, 255))])
            .unwrap_or_default();

        if app.sim_interactive_map_tiles.is_none() {
            app.sim_interactive_map_tiles = Some(HttpTiles::new(OpenStreetMap, ui.ctx().clone()));
        }

        let map = Map::new(
            app.sim_interactive_map_tiles
                .as_mut()
                .map(|t| t as &mut dyn walkers::Tiles),
            &mut app.sim_interactive_map_memory,
            center_pos,
        )
        .with_plugin(ClickCapturePlugin {
            out: &mut app.sim_interactive_map_clicked,
        })
        .with_plugin(WaypointMarkerPlugin { markers: &markers });

        let available_width = ui.available_width();
        let map_resp = ui.add_sized([available_width, 320.0], map);
        add_map_zoom_controls(
            ui.ctx(),
            map_resp.rect,
            "sim_interactive_map_zoom",
            &mut app.sim_interactive_map_memory,
        );

        ui.label(
            egui::RichText::new(
                "Click on the map to steer toward that position (auto-sets speed to 5 m/s if stopped).",
            )
            .small()
            .weak(),
        );

        // Handle map click.
        if let Some(click) = app.sim_interactive_map_clicked.take() {
            let to_lat = click.position.y();
            let to_lon = click.position.x();
            if running {
                // While running: steer toward the clicked point.
                let from_lat = if map_state.lat_deg != 0.0 {
                    map_state.lat_deg
                } else {
                    app.sim_interactive_lat.trim().parse().unwrap_or(0.0)
                };
                let from_lon = if map_state.lon_deg != 0.0 {
                    map_state.lon_deg
                } else {
                    app.sim_interactive_lon.trim().parse().unwrap_or(0.0)
                };
                #[expect(
                    clippy::unwrap_used,
                    reason = "mutex poison means a prior panic; best-effort bearing update"
                )]
                let mut ist = app.sim_interactive_istate.lock().unwrap();
                ist.bearing_deg = geodetic_bearing(from_lat, from_lon, to_lat, to_lon);
                if ist.speed_ms < 0.5 {
                    ist.speed_ms = 5.0;
                }
            } else {
                // Not running: always fill starting position with the clicked coordinates.
                app.sim_interactive_lat = format!("{to_lat:.6}");
                app.sim_interactive_lon = format!("{to_lon:.6}");
            }
        }
    }

    ui.add_space(8.0);

    // ── Status panel ─────────────────────────────────────────────────────────
    ui.group(|ui| {
        section_title(ui, "Status");

        let state = match app.sim_interactive_state.lock() {
            Ok(guard) => guard.clone(),
            Err(_) => crate::simulator::SimState::default(),
        };

        let (status_text, status_colour) = match &state.status {
            crate::simulator::SimStatus::Idle => ("Idle", egui::Color32::GRAY),
            crate::simulator::SimStatus::Running => ("Running…", egui::Color32::GREEN),
            crate::simulator::SimStatus::Done => ("Done", egui::Color32::LIGHT_BLUE),
            crate::simulator::SimStatus::Stopped => ("Stopped by user", egui::Color32::GOLD),
            crate::simulator::SimStatus::Error => ("Error", egui::Color32::RED),
        };
        ui.label(egui::RichText::new(status_text).color(status_colour));

        if let Some(err) = &state.error {
            ui.horizontal(|ui| {
                ui.colored_label(egui::Color32::RED, err);
                if ui
                    .small_button("Copy")
                    .on_hover_text("Copy error message to clipboard.")
                    .clicked()
                {
                    ui.ctx().copy_text(err.clone());
                }
            });
        }

        let progress = if state.total_steps > 0 {
            state.current_step as f32 / state.total_steps as f32
        } else {
            0.0
        };
        ui.add(
            egui::ProgressBar::new(progress)
                .text(format!("{:.1} s elapsed", state.current_step as f64 / 10.0,))
                .desired_width(500.0),
        )
        .on_hover_text("Simulation time elapsed since start.");

        ui.label(format!(
            "Bytes transmitted: {:.2} MB",
            state.bytes_sent as f64 / 1_000_000.0,
        ));

        if state.lat_deg != 0.0 || state.lon_deg != 0.0 {
            ui.label(format!(
                "Position: {:.6}°, {:.6}°  alt {:.1} m",
                state.lat_deg, state.lon_deg, state.height_m,
            ))
            .on_hover_text("Most recent simulated receiver position (lat, lon, height).");
        }

        if !state.satellites.is_empty() {
            ui.add_space(4.0);
            ui.label(format!("Satellites in view: {}", state.satellites.len()));
            egui::Grid::new("interactive_sat_table")
                .striped(true)
                .min_col_width(60.0)
                .show(ui, |ui| {
                    ui.label(egui::RichText::new("PRN").strong());
                    ui.label(egui::RichText::new("Azimuth").strong());
                    ui.label(egui::RichText::new("Elevation").strong());
                    ui.end_row();
                    for sat in &state.satellites {
                        ui.label(format!("G{:02}", sat.prn));
                        ui.label(format!("{:.1}°", sat.az_deg));
                        ui.label(format!("{:.1}°", sat.el_deg));
                        ui.end_row();
                    }
                });
        }
    });
}

#[expect(
    clippy::too_many_lines,
    reason = "settings tab: simulation-settings group and HackRF-settings group shared by both simulators"
)]
fn show_sim_settings_tab(app: &mut MyApp, ui: &mut egui::Ui) {
    // Settings are locked while either simulator is running.
    let either_running = app.sim_thread.is_some() || app.sim_static_thread.is_some();

    ui.add_space(4.0);
    ui.label(
        egui::RichText::new("Shared by Dynamic Mode and Static Mode.")
            .weak()
            .italics(),
    );
    ui.add_space(6.0);

    // ── Simulation settings ───────────────────────────────────────────────────
    ui.add_enabled_ui(!either_running, |ui| {
        ui.group(|ui| {
            section_title(ui, "Simulation Settings");

            ui.horizontal(|ui| {
                ui.label("Start time:")
                    .on_hover_text(
                        "Scenario start time. Format: YYYY/MM/DD,hh:mm:ss  \
                         or \"now\" for current UTC, or leave empty for ephemeris start.",
                    );
                ui.text_edit_singleline(&mut app.sim_start_time).on_hover_text(
                    "YYYY/MM/DD,hh:mm:ss  ·  \"now\"  ·  leave empty for ephemeris start",
                );
                if ui
                    .small_button("Now")
                    .on_hover_text("Set start time to the current UTC time.")
                    .clicked()
                {
                    app.sim_start_time = "now".to_owned();
                }
                if ui
                    .small_button("Clear")
                    .on_hover_text("Clear the start time field; the ephemeris reference time will be used.")
                    .clicked()
                {
                    app.sim_start_time = String::new();
                }
            });

            ui.checkbox(&mut app.sim_time_override, "Overwrite TOC/TOE to start time")
                .on_hover_text(
                    "Shifts all ephemeris TOC/TOE values to match the scenario \
                     start time. Allows using any RINEX file at an arbitrary time.",
                );

            ui.checkbox(
                &mut app.sim_ionospheric_disable,
                "Disable ionospheric delay correction",
            )
            .on_hover_text(
                "Disables the Klobuchar ionospheric model. \
                 Useful for spacecraft scenarios above the ionosphere.",
            );

            ui.horizontal(|ui| {
                ui.checkbox(&mut app.sim_fixed_gain_enable, "Fixed gain (disable path loss):")
                    .on_hover_text(
                        "Hold all satellite signals at a constant power level \
                         instead of computing gain from satellite distance.",
                    );
                ui.add_enabled(
                    app.sim_fixed_gain_enable,
                    egui::DragValue::new(&mut app.sim_fixed_gain)
                        .range(1..=10_000)
                        .speed(10.0),
                );
            });

            ui.horizontal(|ui| {
                ui.checkbox(&mut app.sim_leap_enable, "Override leap seconds:")
                    .on_hover_text(
                        "Override the GPS leap second parameters from the RINEX file.",
                    );
                ui.add_enabled(
                    app.sim_leap_enable,
                    egui::DragValue::new(&mut app.sim_leap_week)
                        .range(0_i32..=9999_i32)
                        .prefix("week "),
                )
                .on_hover_text("GPS week number when the leap second is effective.");
                ui.add_enabled(
                    app.sim_leap_enable,
                    egui::DragValue::new(&mut app.sim_leap_day)
                        .range(1_i32..=7_i32)
                        .prefix("day "),
                )
                .on_hover_text("Day of week (1 = Sunday … 7 = Saturday).");
                ui.add_enabled(
                    app.sim_leap_enable,
                    egui::DragValue::new(&mut app.sim_leap_delta)
                        .range(-128_i32..=127_i32)
                        .suffix(" s"),
                )
                .on_hover_text("Delta leap seconds: current GPS − UTC offset in whole seconds.");
            });

            ui.horizontal(|ui| {
                ui.label("PPB correction:")
                    .on_hover_text("Oscillator offset in parts-per-billion. Positive = runs fast → shifts signal frequency down.");
                ui.add(
                    egui::DragValue::new(&mut app.sim_ppb)
                        .range(-500_i32..=500_i32)
                        .suffix(" ppb"),
                );
            });

            ui.horizontal(|ui| {
                ui.label("Elevation mask:")
                    .on_hover_text("Minimum satellite elevation angle in degrees. Satellites below this angle are ignored.");
                ui.add(
                    egui::Slider::new(&mut app.sim_elevation_mask, 0.0_f64..=45.0)
                        .suffix("°")
                        .step_by(1.0),
                );
            });

            ui.horizontal(|ui| {
                ui.label("Block PRNs:")
                    .on_hover_text("Comma-separated PRN numbers (1–32) to exclude from simulation, e.g. \"5,12,23\".");
                ui.text_edit_singleline(&mut app.sim_blocked_prns)
                    .on_hover_text("e.g. 5,12,23  — leave empty to include all satellites.");
            });

            ui.add_space(4.0);
            section_title(ui, "Constellations");
            ui.label(
                egui::RichText::new(
                    "All signals share the 1575.42 MHz carrier and are combined in the same IQ output. \
                     BeiDou / Galileo require a RINEX 3 multi-GNSS file (e.g. BRDM*.rnx).",
                )
                .weak()
                .small(),
            );
            ui.horizontal(|ui| {
                // GPS L1 C/A is always enabled.
                let mut gps_always = true;
                ui.add_enabled(false, egui::Checkbox::new(&mut gps_always, "GPS L1 C/A"))
                    .on_hover_text("GPS L1 C/A is always enabled and cannot be disabled.");
                ui.checkbox(&mut app.sim_use_beidou, "BeiDou B1C  (1575.42 MHz)")
                    .on_hover_text(
                        "Include BeiDou B1C signals. Requires BeiDou ephemeris in the RINEX file.",
                    );
                ui.checkbox(&mut app.sim_use_galileo, "Galileo E1  (1575.42 MHz)")
                    .on_hover_text(
                        "Include Galileo E1-B signals. Requires Galileo ephemeris in the RINEX file.",
                    );
            });

            ui.horizontal(|ui| {
                ui.checkbox(&mut app.sim_log_enable, "Position log:")
                    .on_hover_text("Write a CSV position log (time_s,lat_deg,lon_deg,height_m) during the simulation.");
                ui.add_enabled(
                    app.sim_log_enable,
                    egui::TextEdit::singleline(&mut app.sim_log_path)
                        .hint_text("sim_position_log.csv"),
                )
                .on_hover_text("Output path for the position log CSV file.");
            });
        });
    });

    ui.add_space(8.0);

    // ── Output Sink ───────────────────────────────────────────────────────────
    ui.add_enabled_ui(!either_running, |ui| {
        ui.group(|ui| {
            section_title(ui, "Output Sink");

            ui.horizontal(|ui| {
                ui.selectable_value(
                    &mut app.sim_output_type,
                    crate::simulator::SimOutputType::HackRf,
                    "HackRF",
                )
                .on_hover_text("Transmit via HackRF One USB device.");
                ui.selectable_value(
                    &mut app.sim_output_type,
                    crate::simulator::SimOutputType::IqFile,
                    "IQ File",
                )
                .on_hover_text("Write raw 8-bit IQ samples to a file.");
                ui.selectable_value(
                    &mut app.sim_output_type,
                    crate::simulator::SimOutputType::Udp,
                    "UDP",
                )
                .on_hover_text("Stream IQ samples over UDP.");
                ui.selectable_value(
                    &mut app.sim_output_type,
                    crate::simulator::SimOutputType::Tcp,
                    "TCP Server",
                )
                .on_hover_text("Stream IQ samples over TCP (waits for one client connection).");
                ui.selectable_value(
                    &mut app.sim_output_type,
                    crate::simulator::SimOutputType::Null,
                    "Null",
                )
                .on_hover_text("Discard output — useful for testing.");
            });

            match app.sim_output_type {
                crate::simulator::SimOutputType::IqFile => {
                    ui.horizontal(|ui| {
                        ui.label("File path:");
                        ui.text_edit_singleline(&mut app.sim_iq_file_path);
                    });
                }
                crate::simulator::SimOutputType::Udp => {
                    ui.horizontal(|ui| {
                        ui.label("Destination (host:port):");
                        ui.text_edit_singleline(&mut app.sim_udp_addr)
                            .on_hover_text("e.g. 127.0.0.1:4567");
                    });
                }
                crate::simulator::SimOutputType::Tcp => {
                    ui.horizontal(|ui| {
                        ui.label("Listen port:");
                        ui.add(
                            egui::DragValue::new(&mut app.sim_tcp_port).range(1024_u16..=65535_u16),
                        );
                    });
                }
                _ => {}
            }
        });
    });

    ui.add_space(8.0);

    // ── HackRF settings ───────────────────────────────────────────────────────
    ui.add_enabled_ui(!either_running, |ui| {
        ui.group(|ui| {
            section_title(ui, "HackRF Settings");

            ui.horizontal(|ui| {
                ui.label("TX VGA Gain:").on_hover_text(
                    "Transmit Variable Gain Amplifier level (0–47 dB). \
                         Higher values increase the transmitted signal power.",
                );
                ui.add(egui::Slider::new(&mut app.sim_txvga_gain, 0..=47).suffix(" dB"))
                    .on_hover_text(
                        "HackRF TX VGA gain in dB (0–47). \
                         Increase carefully; strong signals can interfere with nearby receivers.",
                    );
            });
            ui.horizontal(|ui| {
                ui.label("Sample Rate:").on_hover_text(
                    "Baseband IQ sample rate sent to the HackRF. \
                         Must be at least 2.046 MHz for GPS L1 C/A.",
                );
                ui.add(
                    egui::Slider::new(&mut app.sim_frequency, 1_000_000..=20_000_000)
                        .suffix(" Hz")
                        .step_by(100_000.0),
                )
                .on_hover_text("Baseband sample rate in Hz (1 – 20 MHz).");
            });
            ui.horizontal(|ui| {
                ui.label("Centre frequency:");
                ui.add(
                    egui::DragValue::new(&mut app.sim_center_freq)
                        .range(1_u64..=6_000_000_000_u64)
                        .speed(100_000.0)
                        .suffix(" Hz"),
                )
                .on_hover_text(
                    "RF centre frequency transmitted by the HackRF. \
                     Default: 1 575 420 000 Hz (GPS L1 C/A).",
                );
                if ui
                    .small_button("L1")
                    .on_hover_text("Reset to the GPS L1 C/A centre frequency (1 575 420 000 Hz).")
                    .clicked()
                {
                    app.sim_center_freq = crate::simulator::GPS_L1_HZ;
                }
            });
            ui.horizontal(|ui| {
                ui.checkbox(&mut app.sim_baseband_filter_enable, "Baseband filter:")
                    .on_hover_text(
                        "Override the baseband filter bandwidth. \
                         When unchecked, set_sample_rate_auto sets this automatically.",
                    );
                ui.add_enabled(
                    app.sim_baseband_filter_enable,
                    egui::DragValue::new(&mut app.sim_baseband_filter)
                        .range(1_750_000_u32..=28_000_000_u32)
                        .speed(250_000.0)
                        .suffix(" Hz"),
                );
            });
            ui.checkbox(&mut app.sim_amp_enable, "Enable RF Amplifier")
                .on_hover_text(
                    "Enable the HackRF on-board RF amplifier (+11 dB). \
                     Use only when the antenna is connected and in a shielded enclosure.",
                );
            ui.label(
                egui::RichText::new(
                    "⚠ Transmitting GPS signals may be illegal. \
                     Use only in a shielded environment.",
                )
                .small()
                .color(egui::Color32::YELLOW),
            );
        });
    });

    if either_running {
        ui.add_space(4.0);
        ui.label(
            egui::RichText::new("Settings are locked while a simulation is running.")
                .small()
                .color(egui::Color32::GOLD),
        );
    }
}

/// Renders a file-selection row with a label, the selected filename, and a
/// Browse button. Returns `true` when Browse is clicked.
fn sim_file_row(
    ui: &mut egui::Ui,
    label: &str,
    current: &Option<std::path::PathBuf>,
    dialog_open: bool,
) -> bool {
    let mut browse_clicked = false;
    ui.horizontal(|ui| {
        ui.label(format!("{label}:"));
        let display = current
            .as_deref()
            .and_then(|p| p.file_name())
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| "None selected".to_owned());
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let btn_text = if dialog_open { "…" } else { "Browse…" };
            if ui
                .add_enabled(!dialog_open, egui::Button::new(btn_text))
                .on_hover_text(format!("Select the {label} file."))
                .clicked()
            {
                browse_clicked = true;
            }
            ui.label(egui::RichText::new(display).monospace().weak())
                .on_hover_text("Currently selected file.");
        });
    });
    browse_clicked
}

// ---------------------------------------------------------------------------
// Page: UMF Route Creator
// ---------------------------------------------------------------------------

/// Available `ORS` routing profiles as `(api_id, display_label)` pairs.
const ORS_PROFILES: &[(&str, &str)] = &[
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
fn ors_profile_label(profile: &str) -> &str {
    ORS_PROFILES
        .iter()
        .find(|(id, _)| *id == profile)
        .map_or(profile, |(_, label)| label)
}

/// Deferred mutations requested by the route-creator page UI.
#[derive(Default)]
struct RoutePageActions {
    do_generate: bool,
    to_remove: Option<usize>,
    add_via: bool,
    set_start: Option<String>,
    set_end: Option<String>,
    add_via_with_pos: Option<String>,
    open_geojson_dialog: bool,
    /// Draw/import tab: remove the last polyline vertex.
    draw_undo_last: bool,
    /// Draw/import tab: remove all polyline vertices.
    draw_clear: bool,
    /// Draw/import tab: open a `GPX`/`KML` import file dialog.
    draw_open_import_dialog: bool,
}

/// Lazily initialises the HTTP tile fetcher the first time the map is shown.
fn ensure_map_tiles(app: &mut MyApp, ctx: &egui::Context) {
    if app.map_tiles.is_none() {
        app.map_tiles = Some(HttpTiles::new(OpenStreetMap, ctx.clone()));
    }
}

/// Renders the OSM map widget with waypoint markers and click capture.
fn show_map_widget(
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
fn show_map_click_popup(
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

#[expect(
    clippy::too_many_lines,
    reason = "three source modes (ORS API / GeoJSON file / Draw+Import) with their own sub-sections make this inherently long"
)]
fn show_create_route_page(app: &mut MyApp, ui: &mut egui::Ui) -> RoutePageActions {
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

// ---------------------------------------------------------------------------
// Page: Waypoint Manager
// ---------------------------------------------------------------------------

/// Deferred mutations requested by the waypoint-manager page UI.
#[derive(Default)]
struct WaypointPageActions {
    edit_index: Option<usize>,
    delete_index: Option<usize>,
    /// Row that was clicked (to select and center map on).
    select_index: Option<usize>,
    save: bool,
}

fn show_waypoints_page(app: &mut MyApp, ui: &mut egui::Ui) -> WaypointPageActions {
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
fn ensure_wp_map_tiles(app: &mut MyApp, ctx: &egui::Context) {
    if app.wp_map_tiles.is_none() {
        app.wp_map_tiles = Some(HttpTiles::new(OpenStreetMap, ctx.clone()));
    }
}

/// Renders the waypoint-manager OSM map widget with optional markers.
fn show_wp_map_widget(
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

/// Renders a clickable bold text header for a sortable table column.
///
/// Appends a ▲ or ▼ to the label when this column is the active sort column.
/// Clicking toggles ascending/descending; clicking a new column resets to ascending.
fn sortable_header_text(
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

#[expect(
    clippy::too_many_lines,
    reason = "table with 7 columns, sortable headers, and filter/sort snapshot logic is inherently long"
)]
fn show_waypoint_table(app: &mut MyApp, ui: &mut egui::Ui, actions: &mut WaypointPageActions) {
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
fn show_add_waypoint_form(app: &mut MyApp, ui: &mut egui::Ui) {
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

// ---------------------------------------------------------------------------
// Page: UMF Route Manager
// ---------------------------------------------------------------------------

/// Lazily initialises the HTTP tile fetcher for the draw-route map.
fn ensure_draw_map_tiles(app: &mut MyApp, ctx: &egui::Context) {
    if app.draw_map_tiles.is_none() {
        app.draw_map_tiles = Some(HttpTiles::new(OpenStreetMap, ctx.clone()));
    }
}

/// Renders the draw-route OSM map with the polyline overlay and click capture.
fn show_draw_map_widget(
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

/// Deferred mutations requested by the route-manager page.
#[derive(Default)]
struct RouteLibraryActions {
    /// Row that was clicked (select for preview).
    select_row: Option<usize>,
    /// Row whose "Delete" button was pressed.
    delete_row: Option<usize>,
    /// Row whose "Edit" button was pressed.
    edit_row: Option<usize>,
    /// "Done" pressed in the route editor — dismiss editor.
    done_editing: bool,
    /// "Open in Draw Route" pressed — transfer edited route and navigate.
    open_in_draw: bool,
}

fn show_routes_page(app: &mut MyApp, ui: &mut egui::Ui) -> RouteLibraryActions {
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

fn show_library_table(app: &MyApp, ui: &mut egui::Ui, actions: &mut RouteLibraryActions) {
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

/// Formats a duration in seconds as `H:MM:SS` (or `M:SS` when < 1 h).
fn format_duration(seconds: f64) -> String {
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
