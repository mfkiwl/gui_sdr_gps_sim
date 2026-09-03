//! Dynamic Mode tab - replays a UMF motion file and follows the simulated
//! position on a live map.

use eframe::egui;
use egui_extras::Column;
use walkers::{HttpTiles, Map, lat_lon, sources::OpenStreetMap};

use crate::app::MyApp;
use crate::map_plugin::{RouteLinePlugin, WaypointMarkerPlugin};
use crate::ui::sim::settings::sim_file_row;
use crate::ui::widgets::{add_map_zoom_controls, format_duration, section_title};

#[expect(
    clippy::too_many_lines,
    reason = "dynamic tab: RINEX file group, route library table, map preview with live position, control buttons, and status panel"
)]
pub(crate) fn show_sim_dynamic_tab(app: &mut MyApp, ui: &mut egui::Ui) {
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
pub(crate) fn interpolate_route_pos(
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
