// Route generation: API fetching, segmentation, and CSV output.

mod geojson;
mod ors;
mod pipeline;
mod segment;

pub use pipeline::{run_pipeline, run_pipeline_from_geojson};
