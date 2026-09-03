//! Settings tab - simulation and `HackRF` options shared by all three simulators.

use eframe::egui;

use crate::app::MyApp;
use crate::ui::widgets::section_title;

#[expect(
    clippy::too_many_lines,
    reason = "settings tab: simulation-settings group and HackRF-settings group shared by both simulators"
)]
pub(crate) fn show_sim_settings_tab(app: &mut MyApp, ui: &mut egui::Ui) {
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
                        .range(0.01..=10.0)
                        .speed(0.05)
                        .fixed_decimals(2),
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
pub(crate) fn sim_file_row(
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
