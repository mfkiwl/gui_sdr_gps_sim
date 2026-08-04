//! Static Mode tab - transmits from one fixed position, looping indefinitely.

use eframe::egui;
use egui_extras::Column;
use walkers::{HttpTiles, Map, lat_lon, sources::OpenStreetMap};

use crate::app::MyApp;
use crate::map_plugin::{ClickCapturePlugin, WaypointMarkerPlugin};
use crate::ui::widgets::{add_map_zoom_controls, section_title};

#[expect(
    clippy::too_many_lines,
    reason = "static tab: RINEX file group, waypoint picker, map, position group, control buttons, and status panel"
)]
pub(crate) fn show_sim_static_tab(app: &mut MyApp, ui: &mut egui::Ui) {
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
