//! Background simulation thread entry-points.

use std::{
    path::Path,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    thread,
    time::Duration,
};

use gps::SignalGeneratorBuilder;
use libhackrf::prelude::*;

use super::state::{SimSettings, SimState, SimStatus};

/// GPS L1 C/A centre frequency in Hz — the default when no override is set.
pub const GPS_L1_HZ: u64 = 1_575_420_000;

/// Entry point for the static-position looping simulator.
///
/// Builds a [`gps::SignalGenerator`] at a fixed lat/lon/alt, opens the `HackRF`,
/// and streams I/Q samples indefinitely by re-initialising the generator at the
/// start of every pass until the stop flag is set.
#[expect(
    clippy::too_many_arguments,
    reason = "lat/lon/alt cannot be bundled into SimSettings without polluting the dynamic-mode path"
)]
pub fn run_static_loop(
    rinex_path: &Path,
    lat: f64,
    lon: f64,
    alt: f64,
    loop_duration: f64,
    settings: &SimSettings,
    state: &Arc<Mutex<SimState>>,
    stop: &Arc<AtomicBool>,
) {
    match do_run_static(rinex_path, lat, lon, alt, loop_duration, settings, state, stop) {
        Ok(()) => {
            #[expect(clippy::unwrap_used, reason = "mutex poison means the UI thread panicked; further recovery is not meaningful")]
            let mut s = state.lock().unwrap();
            if s.status == SimStatus::Running {
                s.status = SimStatus::Done;
            }
        }
        Err(e) => {
            #[expect(clippy::unwrap_used, reason = "mutex poison means the UI thread panicked; further recovery is not meaningful")]
            let mut s = state.lock().unwrap();
            if stop.load(Ordering::Relaxed) {
                s.status = SimStatus::Stopped;
            } else if s.status == SimStatus::Running {
                s.status = SimStatus::Error;
                if s.error.is_none() {
                    s.error = Some(e.to_string());
                }
            }
        }
    }
}

#[expect(
    clippy::too_many_lines,
    reason = "linear pipeline: builder setup, HackRF config, FIFO channel, looping producer, teardown"
)]
#[expect(
    clippy::too_many_arguments,
    reason = "lat/lon/alt cannot be bundled into SimSettings without polluting the dynamic-mode path"
)]
fn do_run_static(
    rinex_path: &Path,
    lat: f64,
    lon: f64,
    alt: f64,
    loop_duration: f64,
    settings: &SimSettings,
    state: &Arc<Mutex<SimState>>,
    stop: &Arc<AtomicBool>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // 1. Build the signal generator for a fixed static position.
    let start_time = settings.start_time.as_deref().map(|s| {
        if s.eq_ignore_ascii_case("now") {
            "now".to_owned()
        } else {
            let mut iso = s.replace('/', "-").replace(',', "T");
            iso.push('Z');
            iso
        }
    });

    let leap = settings
        .leap
        .map(|(week, day, delta)| vec![week, day, delta]);

    let mut generator = SignalGeneratorBuilder::default()
        .navigation_file(Some(rinex_path.to_path_buf()))?
        .location(Some(vec![lat, lon, alt]))?
        .duration(Some(loop_duration))
        .data_format(Some(8))?
        .frequency(Some(settings.frequency))?
        .time(start_time)?
        .time_override(Some(settings.time_override))
        .ionospheric_disable(Some(settings.ionospheric_disable))
        .path_loss(settings.fixed_gain)
        .leap(leap)
        .verbose(Some(false))
        .build()?;

    // 2. Open and configure HackRF.
    let mut sdr = HackRF::new_auto()?;
    sdr.set_freq(settings.center_frequency)?;
    sdr.set_sample_rate_auto(settings.frequency as f64)?;
    if let Some(bw) = settings.baseband_filter {
        sdr.set_baseband_filter_bandwidth(bw)?;
    }
    sdr.set_txvga_gain(settings.txvga_gain)?;
    sdr.set_amp_enable(settings.amp_enable)?;
    sdr.enter_tx_mode()?;

    // 3. FIFO channel: generator (producer) → HackRF thread (consumer).
    //    Capacity of 8 blocks ≈ 8 × 520 KB ≈ 4 MB of lookahead.
    let (tx, rx) = mpsc::sync_channel::<Vec<u8>>(8);

    let hackrf_err: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));
    let hackrf_err_t = Arc::clone(&hackrf_err);
    let state_t = Arc::clone(state);

    // 4. HackRF consumer thread — persists across all loop passes.
    let hackrf_thread = thread::spawn(move || {
        let mut endpoint = match sdr.tx_queue() {
            Ok(ep) => ep,
            Err(e) => {
                #[expect(clippy::unwrap_used, reason = "mutex poison means simulation thread panicked; unrecoverable")]
                {
                    *hackrf_err_t.lock().unwrap() = Some(e.to_string());
                }
                return;
            }
        };

        let mut chunk = vec![0u8; HACKRF_TRANSFER_BUFFER_SIZE];

        while let Ok(block) = rx.recv() {
            for window in block.chunks(HACKRF_TRANSFER_BUFFER_SIZE) {
                let n = window.len();
                #[expect(
                    clippy::indexing_slicing,
                    reason = "n = window.len() ≤ HACKRF_TRANSFER_BUFFER_SIZE = chunk.len()"
                )]
                {
                    chunk[..n].copy_from_slice(window);
                    if n < HACKRF_TRANSFER_BUFFER_SIZE {
                        chunk[n..].fill(0);
                    }
                }

                if let Err(e) = endpoint
                    .transfer_blocking(chunk.clone().into(), Duration::from_secs(5))
                    .into_result()
                {
                    #[expect(clippy::unwrap_used, reason = "mutex poison means simulation thread panicked; unrecoverable")]
                    {
                        *hackrf_err_t.lock().unwrap() = Some(e.to_string());
                    }
                    return;
                }

                #[expect(clippy::unwrap_used, reason = "mutex poison means simulation thread panicked; unrecoverable")]
                {
                    state_t.lock().unwrap().bytes_sent += n as u64;
                }
            }
        }

        sdr.stop_tx().ok();
    });

    // 5. Looping simulation: re-initialise the generator on every pass.
    let mut loop_error: Option<Box<dyn std::error::Error + Send + Sync>> = None;

    loop {
        if stop.load(Ordering::Relaxed) {
            break;
        }

        generator.initialize()?;

        #[expect(clippy::unwrap_used, reason = "mutex poison means UI thread panicked; unrecoverable")]
        {
            let mut s = state.lock().unwrap();
            s.total_steps = generator.simulation_step_count;
            s.current_step = 0;
            s.loop_count += 1;
        }

        let tx_pass = tx.clone();
        let state_pass = Arc::clone(state);
        let stop_pass = Arc::clone(stop);
        let mut step: usize = 0;

        let sim_result = generator.run_simulation_with_callback(move |block| {
            if stop_pass.load(Ordering::Relaxed) {
                return Err(gps::Error::msg("stopped"));
            }

            tx_pass
                .send(block.to_vec())
                .map_err(|_err| gps::Error::msg("HackRF channel closed"))?;

            step += 1;
            #[expect(clippy::unwrap_used, reason = "mutex poison means UI thread panicked; unrecoverable")]
            {
                state_pass.lock().unwrap().current_step = step;
            }
            Ok(())
        });

        if stop.load(Ordering::Relaxed) {
            break;
        }
        if let Err(e) = sim_result {
            loop_error = Some(e.into());
            break;
        }
    }

    // Signal the HackRF thread to exit and wait for it to flush.
    drop(tx);
    hackrf_thread.join().ok();

    // Surface any error the HackRF thread recorded.
    #[expect(clippy::unwrap_used, reason = "mutex poison means UI thread panicked; unrecoverable")]
    let hackrf_error = hackrf_err.lock().unwrap().take();
    if let Some(err) = hackrf_error {
        #[expect(clippy::unwrap_used, reason = "mutex poison means UI thread panicked; unrecoverable")]
        {
            state.lock().unwrap().error = Some(format!("HackRF error: {err}"));
        }
        return Err(err.into());
    }

    // When the stop flag caused the loop to exit, return Err so the wrapper
    // can set SimStatus::Stopped (rather than Done).
    if stop.load(Ordering::Relaxed) {
        return Err("stopped by user".into());
    }

    if let Some(e) = loop_error {
        return Err(e);
    }

    Ok(())
}

/// Entry point called from the UI after spawning a dedicated thread.
///
/// Updates `state` on completion or error; sets `status` to `Done`, `Stopped`,
/// or `Error` as appropriate.
pub fn run(
    rinex_path: &Path,
    motion_path: &Path,
    settings: &SimSettings,
    state: &Arc<Mutex<SimState>>,
    stop: &Arc<AtomicBool>,
) {
    match do_run(rinex_path, motion_path, settings, state, stop) {
        Ok(()) => {
            #[expect(clippy::unwrap_used, reason = "mutex poison means the UI thread panicked; further recovery is not meaningful")]
            let mut s = state.lock().unwrap();
            if s.status == SimStatus::Running {
                s.status = SimStatus::Done;
            }
        }
        Err(e) => {
            #[expect(clippy::unwrap_used, reason = "mutex poison means the UI thread panicked; further recovery is not meaningful")]
            let mut s = state.lock().unwrap();
            if stop.load(Ordering::Relaxed) {
                s.status = SimStatus::Stopped;
            } else if s.status == SimStatus::Running {
                s.status = SimStatus::Error;
                if s.error.is_none() {
                    s.error = Some(e.to_string());
                }
            }
        }
    }
}

#[expect(
    clippy::too_many_lines,
    reason = "linear pipeline: builder setup, HackRF config, FIFO channel, producer loop, teardown"
)]
fn do_run(
    rinex_path: &Path,
    motion_path: &Path,
    settings: &SimSettings,
    state: &Arc<Mutex<SimState>>,
    stop: &Arc<AtomicBool>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // 1. Build and initialise the signal generator.
    //
    // Convert the user-supplied start time from "YYYY/MM/DD,hh:mm:ss" (the
    // anywhere-sdr CLI format) to RFC 3339 "YYYY-MM-DDThh:mm:ssZ" which jiff
    // can parse.  "now" and None are passed through unchanged.
    let start_time = settings.start_time.as_deref().map(|s| {
        if s.eq_ignore_ascii_case("now") {
            "now".to_owned()
        } else {
            // Convert "YYYY/MM/DD,hh:mm:ss" → "YYYY-MM-DDThh:mm:ssZ" (RFC 3339).
            let mut iso = s.replace('/', "-").replace(',', "T");
            iso.push('Z');
            iso
        }
    });

    let leap = settings
        .leap
        .map(|(week, day, delta)| vec![week, day, delta]);

    let mut generator = SignalGeneratorBuilder::default()
        .navigation_file(Some(rinex_path.to_path_buf()))?
        .user_motion_file(Some(motion_path.to_path_buf()))?
        .data_format(Some(8))? // 8-bit signed I/Q — HackRF native format
        .frequency(Some(settings.frequency))?
        .time(start_time)?
        .time_override(Some(settings.time_override))
        .ionospheric_disable(Some(settings.ionospheric_disable))
        .path_loss(settings.fixed_gain)
        .leap(leap)
        .verbose(Some(false))
        .build()?;

    generator.initialize()?;

    // Expose total step count to the UI.
    #[expect(clippy::unwrap_used, reason = "mutex poison means the UI thread panicked; further recovery is not meaningful")]
    {
        state.lock().unwrap().total_steps = generator.simulation_step_count;
    }

    // 2. Open and configure HackRF.
    let mut sdr = HackRF::new_auto()?;
    sdr.set_freq(settings.center_frequency)?;
    sdr.set_sample_rate_auto(settings.frequency as f64)?;
    // If a manual baseband filter bandwidth was specified, override the value
    // that set_sample_rate_auto chose automatically.
    if let Some(bw) = settings.baseband_filter {
        sdr.set_baseband_filter_bandwidth(bw)?;
    }
    sdr.set_txvga_gain(settings.txvga_gain)?;
    sdr.set_amp_enable(settings.amp_enable)?;
    sdr.enter_tx_mode()?;

    // 3. FIFO channel: generator (producer) → HackRF thread (consumer).
    //    Capacity of 8 blocks ≈ 8 × 520 KB ≈ 4 MB of lookahead.
    let (tx, rx) = mpsc::sync_channel::<Vec<u8>>(8);
    let tx_cb = tx.clone();

    let hackrf_err: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));
    let hackrf_err_t = Arc::clone(&hackrf_err);
    let state_t = Arc::clone(state);

    // 4. HackRF consumer thread.
    let hackrf_thread = thread::spawn(move || {
        let mut endpoint = match sdr.tx_queue() {
            Ok(ep) => ep,
            Err(e) => {
                #[expect(clippy::unwrap_used, reason = "mutex poison means simulation thread panicked; unrecoverable")]
                {
                    *hackrf_err_t.lock().unwrap() = Some(e.to_string());
                }
                return;
            }
        };

        // Each 100 ms block must be split into HACKRF_TRANSFER_BUFFER_SIZE
        // (256 KB) chunks for the USB DMA engine.
        let mut chunk = vec![0u8; HACKRF_TRANSFER_BUFFER_SIZE];

        while let Ok(block) = rx.recv() {
            for window in block.chunks(HACKRF_TRANSFER_BUFFER_SIZE) {
                let n = window.len();
                #[expect(
                    clippy::indexing_slicing,
                    reason = "n = window.len() ≤ HACKRF_TRANSFER_BUFFER_SIZE = chunk.len()"
                )]
                {
                    chunk[..n].copy_from_slice(window);
                    if n < HACKRF_TRANSFER_BUFFER_SIZE {
                        chunk[n..].fill(0);
                    }
                }

                if let Err(e) = endpoint
                    .transfer_blocking(chunk.clone().into(), Duration::from_secs(5))
                    .into_result()
                {
                    #[expect(clippy::unwrap_used, reason = "mutex poison means simulation thread panicked; unrecoverable")]
                    {
                        *hackrf_err_t.lock().unwrap() = Some(e.to_string());
                    }
                    return;
                }

                #[expect(clippy::unwrap_used, reason = "mutex poison means simulation thread panicked; unrecoverable")]
                {
                    state_t.lock().unwrap().bytes_sent += n as u64;
                }
            }
        }

        // Channel closed — flush and stop TX.
        sdr.stop_tx().ok();
    });

    // 5. GPS simulation loop with streaming callback.
    let mut step: usize = 0;
    let state_cb = Arc::clone(state);
    let stop_cb = Arc::clone(stop);

    let sim_result = generator.run_simulation_with_callback(move |block| {
        if stop_cb.load(Ordering::Relaxed) {
            return Err(gps::Error::msg("stopped"));
        }

        tx_cb
            .send(block.to_vec())
            .map_err(|_err| gps::Error::msg("HackRF channel closed"))?;

        step += 1;
        #[expect(clippy::unwrap_used, reason = "mutex poison means UI thread panicked; unrecoverable")]
        {
            state_cb.lock().unwrap().current_step = step;
        }
        Ok(())
    });

    // Drop sender so the HackRF thread's rx.recv() returns Err and it exits.
    drop(tx);

    // Wait for the HackRF thread to finish flushing.
    hackrf_thread.join().ok();

    // Surface any error the HackRF thread recorded.
    #[expect(clippy::unwrap_used, reason = "mutex poison means UI thread panicked; unrecoverable")]
    let hackrf_error = hackrf_err.lock().unwrap().take();
    if let Some(err) = hackrf_error {
        #[expect(clippy::unwrap_used, reason = "mutex poison means UI thread panicked; unrecoverable")]
        {
            state.lock().unwrap().error = Some(format!("HackRF error: {err}"));
        }
        return Err(err.into());
    }

    // Propagate any simulation error (e.g. "stopped" or file I/O issues).
    sim_result?;

    Ok(())
}
