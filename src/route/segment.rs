//! Route segmentation and GPS transmit-point generation.

use geo::{Distance as _, Geodesic, InterpolatePoint as _, Point};

/// A processed route segment with pre-computed GPS transmit positions.
///
/// Each segment spans two consecutive route coordinates. Transmit points
/// are evenly spaced along the geodesic at 0.1-second intervals.
#[expect(dead_code, reason = "fields available for future features (logging, visualisation)")]
#[derive(Debug, Clone)]
pub struct Segment {
    pub segment_id: i32,
    pub start_point: Point,
    pub start_elevation: f64,
    pub end_point: Point,
    pub end_elevation: f64,
    /// Geodesic length of this segment in metres.
    pub segment_distance: f64,
    /// Speed in m/s used to compute transmit-point spacing.
    pub velocity: f64,
    /// Distance between consecutive transmit points in metres.
    pub transmit_point_distance: f64,
    /// Pre-computed transmit positions as `[lon, lat, elevation_m]`.
    pub transmit_points: Vec<[f64; 3]>,
}

/// Splits a coordinate sequence into segments with GPS transmit points.
///
/// `segment_velocity` is in km/h. Points are spaced `velocity / 36` metres
/// apart, which equals one position every 0.1 s at that speed.
///
/// Returns an empty `Vec` when fewer than two coordinate points are supplied.
#[expect(clippy::indexing_slicing, reason = "loop bounds guarantee all accesses are within range")]
pub fn segmentize(lon: &[f64], lat: &[f64], ele: &[f64], segment_velocity: f64) -> Vec<Segment> {
    if lon.len() < 2 || lat.len() < 2 || ele.len() < 2 {
        return Vec::new();
    }

    (0..lat.len() - 1)
        .map(|i| {
            let start = Point::new(lon[i], lat[i]);
            let end = Point::new(lon[i + 1], lat[i + 1]);
            let avg_elevation = f64::midpoint(ele[i], ele[i + 1]);

            let distance = Geodesic.distance(start, end);
            let step = segment_velocity / 36.0; // metres between points (= 0.1 s apart)

            let transmit_points: Vec<[f64; 3]> = Geodesic
                .points_along_line(start, end, step, false)
                .map(|p| [p.x(), p.y(), avg_elevation])
                .collect();

            Segment {
                segment_id: i as i32,
                start_point: start,
                start_elevation: ele[i],
                end_point: end,
                end_elevation: ele[i + 1],
                segment_distance: distance,
                velocity: segment_velocity / 3.6,
                transmit_point_distance: step,
                transmit_points,
            }
        })
        .collect()
}
