//! UI rendering: menu bar, navigation sidebar, and all page views.
//!
//! The public entry point is [`update`], called every frame by `MyApp`'s
//! `eframe::App` implementation.  Everything below it is split by page: one
//! module per screen, plus [`chrome`] for the surrounding menu and sidebar, and
//! [`widgets`] for helpers shared between pages.

pub(crate) mod chrome;
pub(crate) mod home;
pub(crate) mod library;
pub(crate) mod route;
pub(crate) mod sim;
pub(crate) mod waypoints;
pub(crate) mod widgets;

use eframe::egui;

use crate::app::{AppStatus, MyApp};
use crate::ui::chrome::{show_api_key_dialog, show_central_panel, show_menu_bar, show_nav_panel};

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
