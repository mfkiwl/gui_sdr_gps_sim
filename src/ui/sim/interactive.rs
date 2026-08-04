//! Interactive Mode tab - live steering via a compass dial and speed sliders.

use eframe::egui;
use walkers::{HttpTiles, Map, lat_lon, sources::OpenStreetMap};

use crate::app::MyApp;
use crate::map_plugin::{ClickCapturePlugin, WaypointMarkerPlugin};
use crate::ui::widgets::{add_map_zoom_controls, section_title};

/// Compute the initial bearing (degrees, 0–360) from one geographic point to another.
pub(crate) fn geodetic_bearing(from_lat: f64, from_lon: f64, to_lat: f64, to_lon: f64) -> f64 {
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
pub(crate) fn bearing_dial(ui: &mut egui::Ui, bearing_deg: &mut f64, enabled: bool) -> bool {
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
pub(crate) fn show_sim_interactive_tab(app: &mut MyApp, ui: &mut egui::Ui, ctx: &egui::Context) {
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
