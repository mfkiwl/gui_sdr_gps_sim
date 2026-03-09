#![warn(clippy::all, rust_2018_idioms)]
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() -> eframe::Result {
    env_logger::init(); // Log to stderr; control with RUST_LOG=debug

    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([400.0, 300.0])
            .with_min_inner_size([300.0, 220.0])
            .with_icon(
                eframe::icon_data::from_png_bytes(
                    &include_bytes!("../assets/img/icon-256.png")[..],
                )
                .expect("Failed to load icon"),
            ),
        ..Default::default()
    };

    eframe::run_native(
        "Gui SDR GPS Sim",
        native_options,
        Box::new(|cc| {
            egui_extras::install_image_loaders(&cc.egui_ctx);
            Ok(Box::new(gui_sdr_gps_sim::MyApp::new(cc)))
        }),
    )
}
