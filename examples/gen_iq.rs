//! Generate a short IQ file for signal analysis.
//!
//! ```text
//! cargo run --example gen_iq
//! ```
//!
//! Writes 5 seconds of GPS L1 C/A baseband (sc8, 3 MSPS) to `gps_signal.iq`
//! using the most recent RINEX file in `Rinex_files/`.  Takes ~5 s to run
//! (the real-time gate paces generation to match the sample rate).

use gui_sdr_gps_sim::gps_sim::{Location, SdrOutput, Simulator};

#[expect(
    clippy::print_stderr,
    reason = "example binary: progress output is the intended side effect"
)]
fn main() {
    // Amsterdam – Centraal Station
    let lat = 52.3791;
    let lon = 4.9003;
    let height = 5.0;

    // Most recent broadcast RINEX available locally.
    let rinex = "Rinex_files/brdc1510.26n";
    let output = "gps_signal.iq";

    eprintln!("Generating 5 s of GPS L1 C/A -> {output}");
    eprintln!("Location: {lat}N {lon}E  height {height} m");

    let result = Simulator::builder()
        .rinex(rinex)
        .location(Location::degrees(lat, lon, height))
        .duration_secs(5)
        .time_override(true)
        .output(SdrOutput::IqFile {
            path: output.to_owned(),
        })
        .build()
        .and_then(|sim| sim.run());

    match result {
        Ok(()) => eprintln!("Done -> {output}  ({} MB)", 5 * 3_000_000 * 2 / 1_000_000),
        Err(e) => {
            eprintln!("Error: {e}");
            std::process::exit(1);
        }
    }
}
