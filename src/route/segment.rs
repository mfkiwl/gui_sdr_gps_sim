//! Route segmentation and GPS transmit-point generation.

use geo::{Distance as _, Geodesic, InterpolatePoint as _, Point};

/// A processed route segment with pre-computed GPS transmit positions.
///
/// Each segment spans two consecutive route coordinates. Transmit points
/// are evenly spaced along the geodesic at 0.1-second intervals.
#[expect(
    dead_code,
    reason = "fields available for future features (logging, visualisation)"
)]
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
/// A **carry** value is maintained across segment boundaries so that transmit
/// points are evenly spaced over the whole route, not reset at every ORS
/// coordinate. Without this, segments shorter than one step (common in dense
/// urban routes) produce zero points and are skipped instantly, causing the
/// effective speed to be much higher than configured.
///
/// Returns an empty `Vec` when fewer than two coordinate points are supplied.
#[expect(
    clippy::indexing_slicing,
    reason = "loop bounds guarantee all accesses are within range"
)]
pub fn segmentize(lon: &[f64], lat: &[f64], ele: &[f64], segment_velocity: f64) -> Vec<Segment> {
    if lon.len() < 2 || lat.len() < 2 || ele.len() < 2 {
        return Vec::new();
    }

    let step = segment_velocity / 36.0; // metres per transmit point (= 0.1 s at this speed)

    // Metres already consumed into the current step at the start of each segment.
    // Carried over from the previous segment so that the 0.1 s cadence is
    // continuous across segment boundaries.
    let mut carry = 0.0_f64;

    let mut segments = Vec::with_capacity(lon.len() - 1);

    for i in 0..lon.len() - 1 {
        let start = Point::new(lon[i], lat[i]);
        let end = Point::new(lon[i + 1], lat[i + 1]);
        let avg_elevation = f64::midpoint(ele[i], ele[i + 1]);
        let distance = Geodesic.distance(start, end);

        // Place transmit points at `step - carry`, `2*step - carry`, …
        // i.e. the first point is `step - carry` metres from the segment start.
        let transmit_points: Vec<[f64; 3]> = if distance > 0.0 {
            let mut pts = Vec::new();
            let mut d = step - carry;
            while d <= distance {
                let fraction = (d / distance).clamp(0.0, 1.0);
                let pt = Geodesic.point_at_ratio_between(start, end, fraction);
                pts.push([pt.x(), pt.y(), avg_elevation]);
                d += step;
            }
            pts
        } else {
            Vec::new()
        };

        // Update carry: total metres traversed modulo step length.
        carry = (carry + distance) % step;

        segments.push(Segment {
            segment_id: i as i32,
            start_point: start,
            start_elevation: ele[i],
            end_point: end,
            end_elevation: ele[i + 1],
            segment_distance: distance,
            velocity: segment_velocity / 3.6,
            transmit_point_distance: step,
            transmit_points,
        });
    }

    segments
}
