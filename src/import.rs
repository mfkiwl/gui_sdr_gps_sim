//! Parsers for importing `GPX` and `KML` route files into ordered `[lat, lon]`
//! coordinate sequences.

use std::path::Path;

/// Reads a `.gpx` or `.kml` route file and returns its coordinates as an
/// ordered sequence of `[lat, lon]` pairs.
///
/// The format is determined from the file extension (case-insensitive).
///
/// # Errors
/// Returns a human-readable error string if the file cannot be read, the
/// extension is unsupported, or the content cannot be parsed.
pub fn load_route_file(path: &Path) -> Result<Vec<[f64; 2]>, String> {
    let content = std::fs::read_to_string(path)
        .map_err(|e| format!("Read '{}': {e}", path.display()))?;

    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();

    match ext.as_str() {
        "gpx" => parse_gpx(&content),
        "kml" => parse_kml(&content),
        other => Err(format!("Unsupported file extension: '{other}'")),
    }
}

// ── GPX ───────────────────────────────────────────────────────────────────────

/// Extracts ordered `[lat, lon]` pairs from a `GPX` document.
///
/// Track points (`trkpt`) are preferred; route points (`rtept`) are used when
/// no track is present; standalone waypoints (`wpt`) serve as a last resort.
fn parse_gpx(content: &str) -> Result<Vec<[f64; 2]>, String> {
    let doc = roxmltree::Document::parse(content)
        .map_err(|e| format!("GPX parse error: {e}"))?;

    let mut track_pts: Vec<[f64; 2]> = Vec::new();
    let mut route_pts: Vec<[f64; 2]> = Vec::new();
    let mut waypoints: Vec<[f64; 2]> = Vec::new();

    for node in doc.descendants() {
        match node.tag_name().name() {
            "trkpt" => {
                if let Some(pt) = gpx_point(&node) {
                    track_pts.push(pt);
                }
            }
            "rtept" => {
                if let Some(pt) = gpx_point(&node) {
                    route_pts.push(pt);
                }
            }
            "wpt" => {
                if let Some(pt) = gpx_point(&node) {
                    waypoints.push(pt);
                }
            }
            _ => {}
        }
    }

    let points = if !track_pts.is_empty() {
        track_pts
    } else if !route_pts.is_empty() {
        route_pts
    } else {
        waypoints
    };

    if points.is_empty() {
        Err("No track points found in GPX file.".to_owned())
    } else {
        Ok(points)
    }
}

/// Parses the `lat` and `lon` attributes of a `trkpt` / `rtept` / `wpt` node.
fn gpx_point(node: &roxmltree::Node<'_, '_>) -> Option<[f64; 2]> {
    let lat = node.attribute("lat")?.parse::<f64>().ok()?;
    let lon = node.attribute("lon")?.parse::<f64>().ok()?;
    Some([lat, lon])
}

// ── KML ───────────────────────────────────────────────────────────────────────

/// Extracts ordered `[lat, lon]` pairs from a `KML` document.
///
/// `LineString` coordinates are preferred over individual `Point` coordinates.
fn parse_kml(content: &str) -> Result<Vec<[f64; 2]>, String> {
    let doc = roxmltree::Document::parse(content)
        .map_err(|e| format!("KML parse error: {e}"))?;

    let mut line_pts: Vec<[f64; 2]> = Vec::new();
    let mut point_pts: Vec<[f64; 2]> = Vec::new();

    for node in doc.descendants() {
        if node.tag_name().name() != "coordinates" {
            continue;
        }
        let in_linestring = node
            .ancestors()
            .any(|a| a.tag_name().name() == "LineString");
        let coords = kml_coordinates(node.text().unwrap_or(""))?;
        if in_linestring {
            line_pts.extend(coords);
        } else {
            point_pts.extend(coords);
        }
    }

    let points = if !line_pts.is_empty() {
        line_pts
    } else {
        point_pts
    };

    if points.is_empty() {
        Err("No coordinate data found in KML file.".to_owned())
    } else {
        Ok(points)
    }
}

/// Parses a `KML` `<coordinates>` text node into `[lat, lon]` pairs.
///
/// `KML` uses space-separated `lon,lat[,alt]` triplets.
fn kml_coordinates(text: &str) -> Result<Vec<[f64; 2]>, String> {
    let mut out = Vec::new();
    for token in text.split_whitespace() {
        let mut parts = token.splitn(3, ',');
        let Some(lon_str) = parts.next() else {
            continue;
        };
        let Some(lat_str) = parts.next() else {
            continue;
        };
        if lon_str.is_empty() || lat_str.is_empty() {
            continue;
        }
        let lon = lon_str
            .parse::<f64>()
            .map_err(|_e| format!("Invalid longitude: '{lon_str}'"))?;
        let lat = lat_str
            .parse::<f64>()
            .map_err(|_e| format!("Invalid latitude: '{lat_str}'"))?;
        out.push([lat, lon]);
    }
    Ok(out)
}
