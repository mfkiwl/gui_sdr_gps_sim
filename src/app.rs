use egui::{ScrollArea, Slider};
use geo::{Distance, Geodesic, InterpolatePoint, Point};
use reqwest;
use serde::{Deserialize, Serialize};
use serde_json::json;

#[derive(Debug, PartialEq, Clone, Copy, serde::Deserialize, serde::Serialize)] // #[derive(Default)] makes it easy to create a starting instance
// #[derive(...)]: Asks Rust to auto-generate common trait implementations
// (Debug for printing, PartialEq for ==, Clone/Copy for duplicating).
// enum: Defines a type with specific choice types (e.g., AppPage can only be Page1, Page2 or Page3).
enum AppPage {
    Page1,
    Page2,
    Page3,
}

/// We derive Deserialize/Serialize so we can persist app state on shutdown.

/// GeoJSON structure for OpenRouteService response
#[derive(Serialize, Deserialize, Debug)]
pub struct GeoJson {
    pub r#type: String,
    pub features: Vec<Feature>,
}

/// Feature in GeoJSON
#[derive(Serialize, Deserialize, Debug)]
pub struct Feature {
    pub geometry: Geometry,
}

/// Geometry in GeoJSON
#[derive(Serialize, Deserialize, Debug)]
pub struct Geometry {
    pub coordinates: Vec<[f64; 3]>, // [lon, lat, elevation]
}

/// Represents a segment of the route with associated metadata
#[derive(Debug, Clone)]
pub struct Segment {
    pub segment_id: i32,
    pub start_point: Point,
    pub start_elevation: f64,
    pub end_point: Point,
    pub end_elevation: f64,
    pub segment_distance: f64,
    pub velocity: f64,
    pub transmit_point_distance: f64,
    pub transmit_points: Vec<[f64; 3]>,
}

#[derive(serde::Deserialize, serde::Serialize)]
#[serde(default)] // #[derive(Default)] makes it easy to create a starting instance
// #[derive(...)]: Asks Rust to auto-generate common trait implementations
// (Debug for printing, PartialEq for ==, Clone/Copy for duplicating).
// if we add new fields, give them default values when deserializing old state

// --- Update TemplateApp Struct (holds ALL application state) ---
//Holds all the data your UI needs.
pub struct TemplateApp {
    // Example stuff:
    current_mode: AppPage, // To control which view is shown
    label: String,
    status: String,
    #[serde(skip)] // This how you opt-out of serialization of a field
    value: f32,
    #[serde(skip)]
    start_lon: String,
    #[serde(skip)]
    start_lat: String,
    #[serde(skip)] // This how you opt-out of serialization of a field
    via_points: Vec<String>,
    #[serde(skip)]
    end_lon: String,
    #[serde(skip)]
    end_lat: String,
    #[serde(skip)]
    velocity: f64,
}

// --- Manually Implement `Default` (starting values) ---
// We do this manually because ColorChoice/AppMode don't have automatic defaults.
impl Default for TemplateApp {
    fn default() -> Self {
        Self {
            // Self { ... }: Syntax to create an instance of the struct (TemplateApp) within its own impl block.
            // // Put all the app defaults here.
            // Example stuff:
            //label: "Hello World!".to_owned(),
            value: 2.7,
            current_mode: AppPage::Page1.to_owned(),
            label: "Type here".to_string(),
            start_lat: "latitude".to_string(),
            start_lon: "Longitude".to_string(),
            via_points: vec![String::new()],
            end_lat: "latitude".to_string(),
            end_lon: "Longitude".to_string(),
            velocity: 1.0, // Default velocity in m/s
            status: String::from("Ready"),
        }
    }
}

impl TemplateApp {
    /// Called once before the first frame.
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        // This is also where you can customize the look and feel of egui using
        // `cc.egui_ctx.set_visuals` and `cc.egui_ctx.set_fonts`.

        // Load previous app state (if any).
        // Note that you must enable the `persistence` feature for this to work.
        if let Some(storage) = cc.storage {
            eframe::get_value(storage, eframe::APP_KEY).unwrap_or_default()
        } else {
            Default::default()
        }
    }
}

// This block tells eframe that TemplateApp knows how to be an application.
// It requires the update method.
impl eframe::App for TemplateApp {
    // The heart of egui! Called every frame (many times per second).
    // &mut self: Gets mutable access to your TemplateApp instance. This is crucial!
    //            It means update can change the data (like self.value += 1.0).
    // ctx: &egui::Context: Provides access to egui's context (input, style, etc.).
    // |ui| { ... }: A closure! An inline function where you define the UI for this frame.
    //               egui gives you the ui object to use inside it.
    // ui.heading(...), ui.horizontal(...), etc.: Methods called on the ui object add widgets (visual elements) to the screen.
    // &mut self.label, &mut self.value: Passing mutable borrows (&mut) links the widget directly to your state field.
    //                                   When the user interacts, the widget modifies your TemplateApp field.

    /// Called by the framework to save state before shutdown.
    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        eframe::set_value(storage, eframe::APP_KEY, self);
    }

    /// Called each time the UI needs repainting, which may be many times per second.
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Put your widgets into a `SidePanel`, `TopBottomPanel`, `CentralPanel`, `Window` or `Area`.
        // For inspiration and more examples, go to https://emilk.github.io/egui

        // --- Top Panel: Menu Bar ---
        egui::TopBottomPanel::top("menu_bar").show(ctx, |ui| {
            egui::MenuBar::new().ui(ui, |ui| {
                // Use the menu bar layout helper

                // File Menu
                ui.menu_button("File", |ui| {
                    // Creates "File" button with dropdown
                    // Place all menu items here
                    if ui.button("Quit").clicked() {
                        // Send command to close the window
                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                }); // End File Menu dropdown

                // Display state on the right side of the menu bar
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.horizontal(|ui| {
                        egui::widgets::global_theme_preference_buttons(ui);
                    });
                }); // End right-aligned section
            }); // End menu bar
        }); // End Top Panel

        // --- Left Panel: Info / Tools ---
        egui::SidePanel::left("nav_panel")
            .default_width(150.0) // Set a default width
            .show(ctx, |ui| {
                ui.heading("nav_panel");
                ui.separator(); // Visual dividing line
                // Mode Menu
                //ui.menu_button("Mode", |ui| {
                // Creates "Mode" button with dropdown
                // Use radio buttons to change mode, close menu on click
                // Clicking a mode radio button (ui.radio_value(&mut self.current_mode, ...)
                // directly changes the self.current_mode field.
                if ui.button("Page 1").clicked() {
                    self.current_mode = AppPage::Page1; // Assuming a `AppPage` enum with a `Page1` variant
                }
                {
                    ui.close();
                }
                if ui.button("Page 2").clicked() {
                    self.current_mode = AppPage::Page2; // Assuming a `AppPage` enum with a `Page2` variant
                }
                {
                    ui.close();
                }
                if ui.button("Page 3").clicked() {
                    self.current_mode = AppPage::Page3; // Assuming a `AppPage` enum with a `Page3` variant
                }
                {
                    ui.close();
                }
                //}); // End Mode Menu dropdown

                ui.separator();

                // Add some spacing at the bottom
                ui.allocate_space(ui.available_size());
            }); // End Side Panel

        // --- Central Panel: Main Content (Changes based on Mode) ---
        egui::CentralPanel::default().show(ctx, |ui| {
            //ui.heading(format!("Current page: {:?}", self.current_mode));
            //ui.separator();

            // Use `match` to show different UI based on `self.current_mode`
            // self.current_mode holds the state (View, Edit, or Settings).
            // The match self.current_mode { ... } block reads this state.
            // Based on the state, different UI code runs, showing the View, Edit, or Settings widgets.
            match self.current_mode {
                // --- Page 1 MODE UI ---
                AppPage::Page1 => {
                    // add the rest of the page 1 layout here
                    //ui.label(self.label.to_string());
                    ui.heading("Original eframe template");

                    ui.horizontal(|ui| {
                        ui.label("Write something: ");
                        ui.text_edit_singleline(&mut self.label);
                    });

                    ui.add(egui::Slider::new(&mut self.value, 0.0..=10.0).text("value"));
                    if ui.button("Increment").clicked() {
                        self.value += 1.0;
                    }

                    ui.separator();
                } // End Page 1 Mode Arm

                // --- PAGE 2 UI ---
                AppPage::Page2 => {
                    ui.heading("Page 2");
                    // add the rest of the page 2 layout here
                } // End Page 2 Mode Arm

                // --- SETTINGS MODE UI ---
                AppPage::Page3 => {
                    let mut route_points: Vec<[f64; 2]> = Vec::new();
                    ui.heading("Route Processor");
                    ui.label("Enter route coordinates and process them to ECEF points.");
                    ui.separator();
                    ui.vertical(|ui| {
                        // Start point
                        ui.label("Start Point (lon,lat):");
                        ui.vertical(|ui| {
                            ui.horizontal(|ui| {
                                ui.label("Longitude:");
                                ui.text_edit_singleline(&mut self.start_lon);
                            });
                            ui.horizontal(|ui| {
                                ui.label("Latitude:");
                                ui.text_edit_singleline(&mut self.start_lat);
                            });
                        });
                        ui.separator();

                        // Via points
                        ui.label("Via Points (one per line):");
                        ScrollArea::vertical().max_height(150.0).show(ui, |ui| {
                            for i in 0..self.via_points.len() {
                                ui.horizontal(|ui| {
                                    ui.label(format!("Point {}:", i + 1));
                                    ui.text_edit_singleline(&mut self.via_points[i]);
                                });
                            }
                            // if ui.button("Add Via Point").clicked() {
                            //     self.via_points.push(String::new());
                            // }
                        });
                        if ui.button("Add Via Point").clicked() {
                            self.via_points.push(String::new());
                        }
                        ui.separator();

                        // End point
                        ui.label("End Point (lon,lat):");
                        ui.vertical(|ui| {
                            ui.horizontal(|ui| {
                                ui.label("Longitude:");
                                ui.text_edit_singleline(&mut self.end_lon);
                            });
                            ui.horizontal(|ui| {
                                ui.label("Latitude:");
                                ui.text_edit_singleline(&mut self.end_lat);
                            });
                        });
                        ui.separator();

                        // Velocity input
                        ui.add(Slider::new(&mut self.velocity, 0.0..=15.0).text("Velocity (km/h)"));
                        ui.separator();

                        // Process button
                        if ui.button("Process Route").clicked() {
                            self.process_route();
                        }

                        ui.separator();
                        ui.label("Status:");
                        ui.label(&self.status);
                    });
                } // End Page 3 Mode Arm
            } // End match self.current_mode
        }); // End CentralPanel
    } // End update fn
} // End impl eframe::App

impl TemplateApp {
    fn process_route(&mut self) {
        // Validate inputs
        let start_lon = match self.start_lon.parse::<f64>() {
            Ok(val) => val,
            Err(_) => {
                self.status = "Invalid start longitude".to_string();
                return;
            }
        };

        let start_lat = match self.start_lat.parse::<f64>() {
            Ok(val) => val,
            Err(_) => {
                self.status = "Invalid start latitude".to_string();
                return;
            }
        };

        // Validate end point
        let end_lon = match self.end_lon.parse::<f64>() {
            Ok(val) => val,
            Err(_) => {
                self.status = "Invalid end longitude".to_string();
                return;
            }
        };

        let end_lat = match self.end_lat.parse::<f64>() {
            Ok(val) => val,
            Err(_) => {
                self.status = "Invalid end latitude".to_string();
                return;
            }
        };
        self.status = "Processing...".to_string();
    }
}

/// Fetches a walking route from OpenRouteService API
///
/// # Arguments
/// * `route_points` - Vector of [lon, lat] coordinates defining the route
///
/// # Returns
/// * `Result<(Vec<f64>, Vec<f64>, Vec<f64>), Box<dyn std::error::Error>>` -
///   Tuple containing longitude, latitude, and elevation vectors
pub async fn get_ors_route(
    route_points: Vec<[f64; 2]>,
) -> Result<(Vec<f64>, Vec<f64>, Vec<f64>), Box<dyn std::error::Error>> {
    // Create HTTP client
    let client = reqwest::Client::new();

    // Prepare request body for OpenRouteService API
    let body = json!({
        "coordinates": route_points,
        "elevation": "true",
        "instructions": "false"
    });

    // Send POST request to OpenRouteService API
    let response = client
        .post("https://api.openrouteservice.org/v2/directions/foot-walking/geojson")
        .header("Content-Type", "application/json; charset=utf-8")
        .header(
            "Accept",
            "application/json, application/geo+json, application/gpx+xml, img/png; charset=utf-8",
        )
        .header(
            "Authorization",
            "5b3ce3597851110001cf6248b413d12a4b7748ac803eae3d90839f42",
        )
        .json(&body)
        .send()
        .await?;

    // Check if request was successful
    if response.status().is_success() {
        // Parse response as GeoJSON
        let geojson_str = response.text().await?;
        let geojson: GeoJson = serde_json::from_str(&geojson_str)
            .map_err(|e| format!("Failed to parse GeoJSON: {}", e))?;

        let mut lon: Vec<f64> = Vec::new();
        let mut lat: Vec<f64> = Vec::new();
        let mut ele: Vec<f64> = Vec::new();

        // Extract coordinates from features
        for feature in &geojson.features {
            let coordinates = &feature.geometry.coordinates;
            for coord in coordinates.iter() {
                lon.push(coord[0]);
                lat.push(coord[1]);
                ele.push(coord[2]);
            }
        }

        Ok((lon, lat, ele))
    } else {
        // Handle API errors
        eprintln!("Request failed with status: {}", response.status());
        let error_text = response.text().await?;
        eprintln!("Error details: {}", error_text);
        Err("Request failed".into())
    }
}

/// Converts coordinate vectors into segments with transmit points
///
/// # Arguments
/// * `lon` - Vector of longitudes
/// * `lat` - Vector of latitudes
/// * `ele` - Vector of elevations
/// * `segment_velocity` - Velocity in km/h for segment calculation
///
/// # Returns
/// * `Vec<Segment>` - Vector of route segments
pub fn segmentize(
    lon: Vec<f64>,
    lat: Vec<f64>,
    ele: Vec<f64>,
    segment_velocity: f64, // segment velocity in km/h
) -> Vec<Segment> {
    let mut segments = Vec::new();

    // Ensure we have at least 2 points to create a segment
    if lon.len() < 2 || lat.len() < 2 || ele.len() < 2 {
        return segments;
    }

    // Create segments between consecutive points
    for i in 0..lat.len() - 1 {
        let start_point_geo = Point::new(lon[i], lat[i]);
        let end_point_geo = Point::new(lon[i + 1], lat[i + 1]);
        let avg_elevation = (ele[i] + ele[i + 1]) / 2.0; // average elevation between start_point and end_point

        // Calculate geodesic distance between points
        let segment_geo_distance = Geodesic.distance(start_point_geo, end_point_geo);

        // Calculate transmit point distance (0.1 seconds at given velocity)
        let transmit_point_distance_geo = segment_velocity / 36.0;

        // Generate intermediate points along the line
        let transmit_points_geo: Vec<Point> = Geodesic
            .points_along_line(
                start_point_geo,
                end_point_geo,
                transmit_point_distance_geo,
                false,
            )
            .collect();

        // Convert Point objects to [f64; 3] arrays for consistency
        let transmit_points: Vec<[f64; 3]> = transmit_points_geo
            .into_iter()
            .map(|point| [point.x(), point.y(), avg_elevation])
            .collect();

        println!("{}", avg_elevation);

        let segment = Segment {
            segment_id: i as i32,
            start_point: start_point_geo,
            start_elevation: ele[i],
            end_point: end_point_geo,
            end_elevation: ele[i + 1],
            segment_distance: segment_geo_distance,
            velocity: segment_velocity / 3.6, // Convert km/h to m/s
            transmit_point_distance: transmit_point_distance_geo,
            transmit_points,
        };

        segments.push(segment);
    }

    segments
}

/// Converts LLA (Latitude, Longitude, Altitude) to ECEF (Earth-Centered Earth-Fixed) coordinates
///
/// # Arguments
/// * `lat` - Latitude in degrees
/// * `lon` - Longitude in degrees
/// * `alt` - Altitude in meters
///
/// # Returns
/// * `(f64, f64, f64)` - ECEF X, Y, Z coordinates in meters
pub fn lla_to_ecef(lat: f64, lon: f64, alt: f64) -> (f64, f64, f64) {
    // WGS84 ellipsoid parameters
    const A: f64 = 6378137.0; // semi-major axis in meters
    const F: f64 = 1.0 / 298.257223563; // flattening
    const E2: f64 = 2.0 * F - F * F; // first eccentricity squared
    // Convert degrees to radians
    let lat_rad = lat.to_radians();
    let lon_rad = lon.to_radians();

    // Calculate N (radius of curvature in the prime vertical)
    let cos_lat = lat_rad.cos();
    let sin_lat = lat_rad.sin();
    let n = A / (1.0 - E2 * sin_lat * sin_lat).sqrt();

    // Convert to ECEF coordinates
    let x = (n + alt) * cos_lat * lon_rad.cos();
    let y = (n + alt) * cos_lat * lon_rad.sin();
    let z = (n * (1.0 - E2) + alt) * sin_lat;

    (x, y, z)
}

/// Converts all transmit points to ECEF format and writes them to a CSV file
///
/// # Arguments
/// * `transmit_points` - Vector of [lon, lat, alt] points
/// * `filename` - Name of the output CSV file
pub fn write_transmit_points_to_csv(
    transmit_points: Vec<[f64; 3]>,
    filename: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    use std::fs::File;
    use std::io::Write;

    let mut file = File::create(filename)?;

    // Write data to CSV
    for (index, point) in transmit_points.iter().enumerate() {
        let (x, y, z) = lla_to_ecef(point[1], point[0], point[2]); // lat, lon, alt
        writeln!(file, "{:.1},{:.6},{:.6},{:.6}", index as f64 * 0.1, x, y, z)?;
    }

    Ok(())
}

pub fn parse_coords(input: &str) -> Result<Vec<f64>, Box<dyn std::error::Error>> {
    let parsed_coords: Result<Vec<f64>, _> =
        input.split(',').map(|s| s.trim().parse::<f64>()).collect();

    parsed_coords.map_err(|e| e.into())
}
