//! Generate an IQ file together with the ground-truth list of PRNs in it.
//!
//! ```text
//! cargo run --release --example gen_iq_labeled
//! ```
//!
//! Acquisition results are only meaningful against a known answer. This writes
//! 20 s of GPS L1 C/A and prints exactly which satellites the simulator put in
//! the file, so `gnuradio/gps_acquisition.py` can be scored for both misses and
//! false positives:
//!
//! ```text
//! cargo run --release --example gen_iq_labeled
//! python gnuradio/gps_acquisition.py --file gnuradio/gps_signal_fixed.iq
//! ```
//!
//! A correct run acquires precisely the printed PRN set, with the strongest
//! peaks on the highest-elevation satellites.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use gui_sdr_gps_sim::gps_sim::{Location, SdrOutput, SimEvent, Simulator};

#[expect(
    clippy::print_stderr,
    reason = "example binary: progress output is the intended side effect"
)]
fn main() {
    let rinex = "Rinex_files/brdc1510.26n";
    let output = "gnuradio/gps_signal_fixed.iq";

    let sats: Arc<Mutex<BTreeMap<u8, (f64, f64)>>> = Arc::new(Mutex::new(BTreeMap::new()));
    let sats2 = sats.clone();

    let result = Simulator::builder()
        .rinex(rinex)
        .location(Location::degrees(52.3791, 4.9003, 5.0))
        .duration_secs(20)
        .time_override(true)
        .on_event(move |e| match e {
            SimEvent::Satellite {
                prn,
                az_deg,
                el_deg,
                ..
            } => {
                sats2
                    .lock()
                    .unwrap_or_else(|p| p.into_inner())
                    .insert(prn, (az_deg, el_deg));
            }
            SimEvent::Status(s) => eprint!("{s}"),
            _ => {}
        })
        .output(SdrOutput::IqFile {
            path: output.to_owned(),
        })
        .build()
        .and_then(|sim| sim.run());

    if let Err(e) = result {
        eprintln!("Error: {e}");
        std::process::exit(1);
    }

    let sats = sats.lock().unwrap_or_else(|p| p.into_inner());
    eprintln!("\nGROUND TRUTH — {} satellites transmitted:", sats.len());
    let list: Vec<String> = sats.keys().map(|p| p.to_string()).collect();
    eprintln!("PRNS={}", list.join(","));
    for (prn, (az, el)) in sats.iter() {
        eprintln!("  PRN {prn:2}  az {az:6.1}°  el {el:5.1}°");
    }
}
