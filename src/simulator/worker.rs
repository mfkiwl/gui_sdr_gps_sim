//! Background simulation thread entry-points.

use std::{
    path::Path,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
};

use super::state::{SimSettings, SimState, SimStatus};

/// GPS L1 C/A centre frequency in Hz — the default when no override is set.
pub const GPS_L1_HZ: u64 = 1_575_420_000;

/// IQ sample rate used by the GPS simulator (Hz).
///
/// Mirrors `gps_sim::types::consts::SAMPLE_RATE`.
pub const GPS_SAMPLE_RATE_HZ: usize = 3_000_000;

// ── Public entry-points ───────────────────────────────────────────────────────

/// Entry point for the static-position looping simulator.
///
/// Runs the GPS L1 C/A signal generation in a loop until `stop` is set.
/// Each pass simulates `loop_duration` seconds of signal at the given position.
#[expect(
    clippy::too_many_arguments,
    reason = "lat/lon/alt cannot be bundled into SimSettings without polluting the dynamic-mode path"
)]
pub fn run_static_loop(
    rinex_path:    &Path,
    lat:           f64,
    lon:           f64,
    alt:           f64,
    loop_duration: f64,
    settings:      &SimSettings,
    state:         &Arc<Mutex<SimState>>,
    stop:          &Arc<AtomicBool>,
) {
    run_static_loop_native(rinex_path, lat, lon, alt, loop_duration, settings, state, stop);
}

/// Entry point for the interactive simulator.
///
/// Runs in a dedicated thread until `stop` is set.  The caller holds a clone
/// of `istate` and updates it each UI frame from keyboard/gamepad events;
/// the simulator reads it every 100 ms to derive the next receiver position.
#[expect(
    clippy::too_many_arguments,
    reason = "starting position components cannot be bundled without polluting other modes"
)]
pub fn run_interactive(
    rinex_path: &Path,
    lat:        f64,
    lon:        f64,
    alt:        f64,
    settings:   &SimSettings,
    state:      &Arc<Mutex<SimState>>,
    stop:       &Arc<AtomicBool>,
    istate:     Arc<std::sync::Mutex<crate::gps_sim::InteractiveState>>,
) {
    run_interactive_native(rinex_path, lat, lon, alt, settings, state, stop, istate);
}

/// Entry point called from the UI after spawning a dedicated thread.
///
/// Runs the GPS L1 C/A signal generation for a single motion-file pass.
pub fn run(
    rinex_path:  &Path,
    motion_path: &Path,
    settings:    &SimSettings,
    state:       &Arc<Mutex<SimState>>,
    stop:        &Arc<AtomicBool>,
    pause:       &Arc<AtomicBool>,
) {
    run_native(rinex_path, motion_path, settings, state, stop, pause);
}

// ── Native implementations ────────────────────────────────────────────────────

#[expect(
    clippy::too_many_arguments,
    reason = "mirrors the public function signature"
)]
fn run_interactive_native(
    rinex_path: &Path,
    lat:        f64,
    lon:        f64,
    alt:        f64,
    settings:   &SimSettings,
    state:      &Arc<Mutex<SimState>>,
    stop:       &Arc<AtomicBool>,
    istate:     Arc<std::sync::Mutex<crate::gps_sim::InteractiveState>>,
) {
    use crate::gps_sim::{Location, Simulator};

    let rinex_str = rinex_path.to_string_lossy().into_owned();
    let output    = build_output(settings);
    let state2    = Arc::clone(state);
    let state3    = Arc::clone(state);
    let stop2     = Arc::clone(stop);

    let mut builder = Simulator::builder()
        .rinex(rinex_str)
        .location(Location::degrees(lat, lon, alt))
        // Large duration — the user stops interactively via the Stop button.
        .duration_secs(86_400)
        .start_time(parse_start_time(&settings.start_time))
        .output(output)
        .with_stop(Arc::clone(stop))
        .with_interactive_state(istate)
        .on_event(move |e| handle_event(&e, &state2, &stop2))
        .ppb(settings.ppb)
        .elevation_mask_deg(settings.elevation_mask_deg)
        .block_prns(settings.blocked_prns.clone())
        .ionospheric_disable(settings.ionospheric_disable)
        .time_override(settings.time_override)
        .fixed_gain(settings.fixed_gain)
        .leap_override(settings.leap)
        .hackrf_sample_rate(settings.frequency as f64)
        .hackrf_center_freq(settings.center_frequency);

    if let Some(bw) = settings.baseband_filter {
        builder = builder.hackrf_baseband_filter(bw);
    }
    if let Some(path) = &settings.log_path {
        builder = builder.log_path(path.clone());
    }

    let result = builder.build().and_then(|sim| sim.run());

    let (final_status, error_msg) = finish_status(result, stop);
    #[expect(clippy::unwrap_used, reason = "mutex poison means UI thread panicked; no recovery")]
    {
        let mut s = state3.lock().unwrap();
        s.status = final_status;
        s.error  = error_msg;
    }

    // Ensure the stop flag is set so the UI knows we finished.
    if !stop.load(Ordering::Relaxed) {
        stop.store(true, Ordering::Relaxed);
    }
}

#[expect(
    clippy::too_many_arguments,
    reason = "mirrors the public function signature"
)]
fn run_static_loop_native(
    rinex_path:    &Path,
    lat:           f64,
    lon:           f64,
    alt:           f64,
    loop_duration: f64,
    settings:      &SimSettings,
    state:         &Arc<Mutex<SimState>>,
    stop:          &Arc<AtomicBool>,
) {
    use crate::gps_sim::{Location, Simulator};

    let rinex_str = rinex_path.to_string_lossy().into_owned();
    let output    = build_output(settings);
    let duration  = loop_duration.max(1.0) as u32;

    let mut loop_count = 0usize;

    while !stop.load(Ordering::Relaxed) {
        let state2 = Arc::clone(state);
        let state3 = Arc::clone(state);
        let stop2  = Arc::clone(stop);

        let builder = Simulator::builder()
            .rinex(rinex_str.clone())
            .location(Location::degrees(lat, lon, alt))
            .duration_secs(duration)
            .start_time(parse_start_time(&settings.start_time))
            .output(output.clone())
            .with_stop(Arc::clone(stop))
            .on_event(move |e| handle_event(&e, &state2, &stop2))
            .ppb(settings.ppb)
            .elevation_mask_deg(settings.elevation_mask_deg)
            .block_prns(settings.blocked_prns.clone())
            .ionospheric_disable(settings.ionospheric_disable)
            .time_override(settings.time_override)
            .fixed_gain(settings.fixed_gain)
            .leap_override(settings.leap)
            .hackrf_sample_rate(settings.frequency as f64)
            .hackrf_center_freq(settings.center_frequency);
        let builder = if let Some(bw) = settings.baseband_filter {
            builder.hackrf_baseband_filter(bw)
        } else {
            builder
        };
        let builder = if let Some(path) = &settings.log_path {
            builder.log_path(path.clone())
        } else {
            builder
        };
        let result = builder.build().and_then(|sim| sim.run());

        // Increment loop count and update state.
        loop_count += 1;
        let (final_status, error_msg) = finish_status(result, stop);
        #[expect(clippy::unwrap_used, reason = "mutex poison means UI thread panicked; no recovery")]
        {
            let mut s = state3.lock().unwrap();
            s.loop_count = loop_count;
            s.status     = final_status.clone();
            s.error      = error_msg;
        }

        if final_status != SimStatus::Done {
            break; // Stop, Aborted, or Error — exit the loop.
        }
    }

    // Ensure the stop flag is set so the UI knows we finished.
    if !stop.load(Ordering::Relaxed) {
        stop.store(true, Ordering::Relaxed);
    }
}

fn run_native(
    rinex_path:  &Path,
    motion_path: &Path,
    settings:    &SimSettings,
    state:       &Arc<Mutex<SimState>>,
    stop:        &Arc<AtomicBool>,
    pause:       &Arc<AtomicBool>,
) {
    use crate::gps_sim::Simulator;

    let rinex_str  = rinex_path.to_string_lossy().into_owned();
    let motion_str = motion_path.to_string_lossy().into_owned();
    let output     = build_output(settings);

    let state2 = Arc::clone(state);
    let state3 = Arc::clone(state);
    let stop2  = Arc::clone(stop);

    let builder = Simulator::builder()
        .rinex(rinex_str)
        .motion_file(motion_str)
        .start_time(parse_start_time(&settings.start_time))
        .output(output)
        .with_stop(Arc::clone(stop))
        .with_pause(Arc::clone(pause))
        .on_event(move |e| handle_event(&e, &state2, &stop2))
        .ppb(settings.ppb)
        .elevation_mask_deg(settings.elevation_mask_deg)
        .block_prns(settings.blocked_prns.clone())
        .ionospheric_disable(settings.ionospheric_disable)
        .time_override(settings.time_override)
        .fixed_gain(settings.fixed_gain)
        .leap_override(settings.leap)
        .hackrf_sample_rate(settings.frequency as f64)
        .hackrf_center_freq(settings.center_frequency);
    let builder = if let Some(bw) = settings.baseband_filter {
        builder.hackrf_baseband_filter(bw)
    } else {
        builder
    };
    let builder = if let Some(path) = &settings.log_path {
        builder.log_path(path.clone())
    } else {
        builder
    };
    let result = builder.build().and_then(|sim| sim.run());

    let (final_status, error_msg) = finish_status(result, stop);
    #[expect(clippy::unwrap_used, reason = "mutex poison means UI thread panicked; no recovery")]
    {
        let mut s = state3.lock().unwrap();
        s.status = final_status;
        s.error  = error_msg;
    }
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn build_output(settings: &SimSettings) -> crate::gps_sim::SdrOutput {
    use crate::gps_sim::SdrOutput;
    use super::state::SimOutputType;
    match &settings.output_type {
        SimOutputType::HackRf => SdrOutput::HackRf {
            gain_db: i32::from(settings.txvga_gain),
            amp:     settings.amp_enable,
        },
        SimOutputType::IqFile => SdrOutput::IqFile {
            path: settings.iq_file_path.clone(),
        },
        SimOutputType::Udp => SdrOutput::UdpStream {
            addr: settings.udp_addr.clone(),
        },
        SimOutputType::Tcp => SdrOutput::TcpServer {
            port: settings.tcp_port,
        },
        SimOutputType::Null => SdrOutput::Null,
    }
}

/// Update `SimState` from a `SimEvent` received by the `on_event` callback.
fn handle_event(
    event: &crate::gps_sim::SimEvent,
    state: &Arc<Mutex<SimState>>,
    stop:  &Arc<AtomicBool>,
) {
    use crate::gps_sim::SimEvent;

    if stop.load(Ordering::Relaxed) {
        return;
    }

    #[expect(clippy::unwrap_used, reason = "mutex poison means UI thread panicked; no recovery")]
    let mut s = state.lock().unwrap();

    match event {
        SimEvent::Progress { current_step, total_steps, bytes_sent } => {
            s.current_step = *current_step;
            s.total_steps  = *total_steps;
            s.bytes_sent   = *bytes_sent;
        }
        SimEvent::Done => {
            s.status = SimStatus::Done;
        }
        SimEvent::Position { lat_deg, lon_deg, height_m } => {
            s.lat_deg  = *lat_deg;
            s.lon_deg  = *lon_deg;
            s.height_m = *height_m;
            s.satellites.clear();
        }
        SimEvent::Satellite { prn, az_deg, el_deg, .. } => {
            s.satellites.push(crate::simulator::state::SimSatInfo {
                prn:    *prn,
                az_deg: *az_deg,
                el_deg: *el_deg,
            });
        }
        _ => {}
    }
}

/// Determine the final `SimStatus` and optional error message from the result and stop flag.
fn finish_status(
    result: Result<(), crate::gps_sim::SimError>,
    stop:   &Arc<AtomicBool>,
) -> (SimStatus, Option<String>) {
    match result {
        Ok(()) if stop.load(Ordering::Relaxed) => (SimStatus::Stopped, None),
        Ok(())                                  => (SimStatus::Done, None),
        Err(crate::gps_sim::SimError::Aborted)  => (SimStatus::Stopped, None),
        Err(e)                                   => (SimStatus::Error, Some(e.to_string())),
    }
}

/// Parse a start-time string from the UI into a [`crate::gps_sim::StartTime`] value.
///
/// Accepts `""` / `"now"` (→ `Now`) or `"YYYY/MM/DD,hh:mm:ss"` (→ `DateTime`).
/// Falls back to `Now` if the string cannot be parsed.
fn parse_start_time(s: &Option<String>) -> crate::gps_sim::StartTime {
    use crate::gps_sim::{StartTime, UtcDate};

    let s = match s.as_deref() {
        None | Some("") => return StartTime::Now,
        Some(s) => s.trim(),
    };
    if s.eq_ignore_ascii_case("now") {
        return StartTime::Now;
    }
    // Expected format: "YYYY/MM/DD,hh:mm:ss"
    let Some((date_part, time_part)) = s.split_once(',') else { return StartTime::Now; };
    let mut d = date_part.splitn(3, '/');
    let (Some(year), Some(month), Some(day)) = (
        d.next().and_then(|v| v.parse::<i32>().ok()),
        d.next().and_then(|v| v.parse::<u8>().ok()),
        d.next().and_then(|v| v.parse::<u8>().ok()),
    ) else { return StartTime::Now; };
    let mut t = time_part.splitn(3, ':');
    let (Some(hour), Some(min), Some(sec)) = (
        t.next().and_then(|v| v.parse::<u8>().ok()),
        t.next().and_then(|v| v.parse::<u8>().ok()),
        t.next().and_then(|v| v.parse::<f64>().ok()),
    ) else { return StartTime::Now; };
    StartTime::DateTime(UtcDate { year, month, day, hour, min, sec })
}

