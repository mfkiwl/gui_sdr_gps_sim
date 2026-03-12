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
            setup_fonts(&cc.egui_ctx);
            Ok(Box::new(gui_sdr_gps_sim::MyApp::new(cc)))
        }),
    )
}

/// Installs a symbol-capable font as a fallback for the proportional family.
///
/// The default egui font subset does not include all Unicode geometric shapes
/// (e.g. ▲ ▼).  This function tries to load a platform system font that does,
/// and appends it to the end of the `Proportional` family so it is only used
/// for glyphs that the primary font cannot render.
///
/// On `wasm32` this is a no-op because there is no local filesystem to read from.
fn setup_fonts(ctx: &egui::Context) {
    #[cfg(not(target_arch = "wasm32"))]
    {
        // Candidate paths, tried in order — first readable file wins.
        let mut candidates: Vec<&str> = Vec::new();

        #[cfg(target_os = "windows")]
        {
            // Segoe UI Symbol has excellent coverage of geometric / misc shapes.
            candidates.push("C:/Windows/Fonts/seguisym.ttf");
            // Fall back to the regular Segoe UI which also covers the basics.
            candidates.push("C:/Windows/Fonts/segoeui.ttf");
        }
        #[cfg(target_os = "macos")]
        {
            candidates.push("/System/Library/Fonts/Supplemental/Symbol.ttf");
            candidates.push("/System/Library/Fonts/Helvetica.ttc");
        }
        #[cfg(target_os = "linux")]
        {
            candidates.push("/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf");
            candidates.push("/usr/share/fonts/TTF/DejaVuSans.ttf");
            candidates.push("/usr/share/fonts/dejavu/DejaVuSans.ttf");
        }

        for path in candidates {
            if let Ok(data) = std::fs::read(path) {
                let mut fonts = egui::FontDefinitions::default();
                fonts.font_data.insert(
                    "symbol_fallback".to_owned(),
                    egui::FontData::from_owned(data).into(),
                );
                fonts
                    .families
                    .entry(egui::FontFamily::Proportional)
                    .or_default()
                    .push("symbol_fallback".to_owned());
                ctx.set_fonts(fonts);
                return;
            }
        }
    }
}
