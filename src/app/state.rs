//! Enums describing which page is shown and what background work is running.

/// Identifies which page is shown in the central panel.
#[derive(Debug, PartialEq, Eq, Clone, Copy, serde::Serialize, serde::Deserialize, Default)]
pub enum AppPage {
    #[default]
    Home,
    SdrGpsSimulator,
    CreateUmfRoute,
    ManageWaypoints,
    ManageUmfRoutes,
}

/// How the `GeoJSON` route geometry is obtained on the [`AppPage::CreateUmfRoute`] page.
#[derive(Debug, PartialEq, Eq, Clone, Copy, Default)]
pub enum RouteSource {
    /// Fetch the route from the `OpenRouteService` directions API.
    #[default]
    OrsApi,
    /// Load a pre-existing `GeoJSON` file from disk.
    GeoJsonFile,
    /// Draw a polyline on the map.
    DrawImport,
    /// Import a `GPX` or `KML` file and use its track as the route.
    ImportKmlGpx,
}

/// Selects the active tab on the [`AppPage::SdrGpsSimulator`] page.
#[derive(Debug, PartialEq, Eq, Clone, Copy, Default)]
pub enum SimTab {
    /// Route-based simulation driven by a user-motion CSV file.
    #[default]
    Dynamic,
    /// Single fixed-position looping simulation (static coordinates).
    Static,
    /// Real-time keyboard-steered receiver position.
    Interactive,
    /// Shared simulation and `HackRF` hardware settings for both simulators.
    Settings,
}

/// Tracks the current state of the background route-generation task.
#[derive(Default)]
pub enum AppStatus {
    #[default]
    Idle,
    Working,
    Done(usize),
    Error(String),
}
