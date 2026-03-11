//! HTTP client for the `OpenRouteService` directions API.

use serde_json::json;

use super::geojson::GeoJson;

/// Fetches a route from the `OpenRouteService` directions API.
///
/// * `profile` — routing profile URL segment (e.g. `"foot-walking"`,
///   `"driving-car"`).
///
/// Returns `(lon, lat, ele, geojson_text)` where the first three are separate
/// coordinate vectors for [`super::segment::segmentize`] and `geojson_text`
/// is the raw `GeoJSON` response body for saving to disk.
///
/// # Errors
/// Returns an error if the HTTP request fails or the response cannot be
/// decoded as valid `GeoJSON`.
pub async fn get_ors_route(
    route_points: Vec<[f64; 2]>,
    api_key: String,
    profile: &str,
) -> Result<(Vec<f64>, Vec<f64>, Vec<f64>, String), Box<dyn std::error::Error>> {
    let client = reqwest::Client::new();

    let body = json!({
        "coordinates": route_points,
        "elevation": "true",
        "instructions": "false",
    });

    let url = format!(
        "https://api.openrouteservice.org/v2/directions/{profile}/geojson"
    );

    let response = client
        .post(url)
        .header("Content-Type", "application/json; charset=utf-8")
        .header(
            "Accept",
            "application/json, application/geo+json, application/gpx+xml, img/png; charset=utf-8",
        )
        .header("Authorization", api_key)
        .json(&body)
        .send()
        .await?;

    if !response.status().is_success() {
        let status = response.status();
        let text = response.text().await.unwrap_or_default();
        return Err(format!("ORS request failed ({status}): {text}").into());
    }

    let text = response.text().await?;
    let geojson: GeoJson = serde_json::from_str(&text)
        .map_err(|e| format!("Failed to parse GeoJSON response: {e}"))?;

    let mut lon = Vec::new();
    let mut lat = Vec::new();
    let mut ele = Vec::new();

    for feature in &geojson.features {
        for coord in &feature.geometry.coordinates {
            lon.push(coord[0]);
            lat.push(coord[1]);
            ele.push(coord[2]);
        }
    }

    Ok((lon, lat, ele, text))
}
