//! `GeoJSON` response types for the `OpenRouteService` API.

/// Top-level `GeoJSON` feature collection returned by the routing API.
#[derive(serde::Serialize, serde::Deserialize, Debug)]
pub struct GeoJson {
    pub r#type: String,
    pub features: Vec<Feature>,
}

/// A single feature inside a `GeoJSON` feature collection.
#[derive(serde::Serialize, serde::Deserialize, Debug)]
pub struct Feature {
    pub geometry: Geometry,
}

/// The geometry of a `GeoJSON` feature, holding the route coordinates.
#[derive(serde::Serialize, serde::Deserialize, Debug)]
pub struct Geometry {
    /// Each entry is `[longitude, latitude, elevation_m]`.
    pub coordinates: Vec<[f64; 3]>,
}
