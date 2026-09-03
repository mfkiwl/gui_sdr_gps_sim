//! Starting and stopping the three simulators, plus RINEX download helpers.
//!
//! Each `start_*` method spawns a dedicated OS thread running the matching
//! `simulator::worker` entry point and stores the handles on [`MyApp`].

use std::sync::mpsc;

use crate::app::MyApp;

impl MyApp {
    /// Spawns an async task that downloads today's RINEX nav file from CDDIS.
    ///
    /// The result is delivered via `sim_rinex_download`; the UI polls it each
    /// frame and updates `sim_rinex_path` on success.
    pub fn download_rinex(&mut self) {
        let (tx, rx) = mpsc::channel();
        self.sim_rinex_download = Some(rx);
        self.sim_rinex_dl_error = None;
        let (doy, year) = crate::rinex::today_doy_year();
        // Use a plain OS thread rather than Tokio's spawn_blocking.  On
        // Windows, spawn_blocking threads can interact with the SChannel TLS
        // certificate-verification machinery (CRL/OCSP via WinHTTP/COM),
        // which can deadlock against eframe's Win32 message pump.  A plain
        // std::thread is fully isolated from both Tokio and the UI thread's
        // COM apartment.
        std::thread::spawn(move || {
            tx.send(crate::rinex::blocking_download(doy, year)).ok();
        });
    }

    /// Spawns a thread that downloads today's RINEX nav file from CDDIS for the
    /// static simulator.
    ///
    /// The result is delivered via `sim_static_rinex_download`; the UI polls it
    /// each frame and updates `sim_static_rinex_path` on success.
    pub fn download_rinex_static(&mut self) {
        let (tx, rx) = mpsc::channel();
        self.sim_static_rinex_download = Some(rx);
        self.sim_static_rinex_dl_error = None;
        let (doy, year) = crate::rinex::today_doy_year();
        std::thread::spawn(move || {
            tx.send(crate::rinex::blocking_download(doy, year)).ok();
        });
    }

    /// Spawns a background thread to download today's RINEX file for the interactive simulator.
    pub fn download_rinex_interactive(&mut self) {
        let (tx, rx) = mpsc::channel();
        self.sim_interactive_rinex_download = Some(rx);
        self.sim_interactive_rinex_dl_error = None;
        let (doy, year) = crate::rinex::today_doy_year();
        std::thread::spawn(move || {
            tx.send(crate::rinex::blocking_download(doy, year)).ok();
        });
    }

    /// Spawns the simulation worker thread.
    ///
    /// Resets shared state, configures settings from current UI values, and
    /// spawns a thread that drives the GPS signal generator and `HackRF` device.
    pub fn start_simulation(&mut self) {
        use std::sync::atomic::Ordering;

        #[expect(
            clippy::unwrap_used,
            reason = "mutex poison means a prior panic; reset is best-effort"
        )]
        {
            *self.sim_state.lock().unwrap() = crate::simulator::SimState {
                status: crate::simulator::SimStatus::Running,
                ..crate::simulator::SimState::default()
            };
        }
        self.sim_stop_flag.store(false, Ordering::Relaxed);

        let rinex_path = self
            .sim_rinex_path
            .clone()
            .expect("start_simulation requires sim_rinex_path; caller must check");
        let motion_path = self
            .sim_motion_path
            .clone()
            .expect("start_simulation requires sim_motion_path; caller must check");

        let settings = crate::simulator::SimSettings {
            frequency: self.sim_frequency,
            txvga_gain: self.sim_txvga_gain,
            amp_enable: self.sim_amp_enable,
            start_time: if self.sim_start_time.trim().is_empty() {
                None
            } else {
                Some(self.sim_start_time.trim().to_owned())
            },
            time_override: self.sim_time_override,
            ionospheric_disable: self.sim_ionospheric_disable,
            fixed_gain: self.sim_fixed_gain_enable.then_some(self.sim_fixed_gain),
            center_frequency: self.sim_center_freq,
            baseband_filter: self
                .sim_baseband_filter_enable
                .then_some(self.sim_baseband_filter),
            leap: self.sim_leap_enable.then_some((
                self.sim_leap_week,
                self.sim_leap_day,
                self.sim_leap_delta,
            )),
            ppb: self.sim_ppb,
            elevation_mask_deg: self.sim_elevation_mask,
            blocked_prns: parse_blocked_prns(&self.sim_blocked_prns),
            log_path: self.sim_log_enable.then(|| {
                if self.sim_log_path.trim().is_empty() {
                    "sim_position_log.csv".to_owned()
                } else {
                    self.sim_log_path.trim().to_owned()
                }
            }),
            output_type: self.sim_output_type.clone(),
            iq_file_path: self.sim_iq_file_path.clone(),
            udp_addr: self.sim_udp_addr.clone(),
            tcp_port: self.sim_tcp_port,
            use_beidou: self.sim_use_beidou,
            use_galileo: self.sim_use_galileo,
        };
        let state = std::sync::Arc::clone(&self.sim_state);
        let stop = std::sync::Arc::clone(&self.sim_stop_flag);
        let pause = std::sync::Arc::clone(&self.sim_pause_flag);
        // Reset pause flag when starting a new simulation.
        pause.store(false, std::sync::atomic::Ordering::Relaxed);

        self.sim_thread = Some(std::thread::spawn(move || {
            crate::simulator::run(&rinex_path, &motion_path, &settings, &state, &stop, &pause);
        }));
    }

    /// Spawns the static looping simulation worker thread.
    ///
    /// Resets shared state, builds [`crate::simulator::SimSettings`] from current
    /// UI values, and spawns a thread that runs the GPS signal generator at a
    /// fixed position in an indefinite loop until the stop flag is set.
    pub fn start_static_simulation(&mut self) {
        use std::sync::atomic::Ordering;

        #[expect(
            clippy::unwrap_used,
            reason = "mutex poison means a prior panic; reset is best-effort"
        )]
        {
            *self.sim_static_state.lock().unwrap() = crate::simulator::SimState {
                status: crate::simulator::SimStatus::Running,
                ..crate::simulator::SimState::default()
            };
        }
        self.sim_static_stop_flag.store(false, Ordering::Relaxed);

        let rinex_path = self
            .sim_static_rinex_path
            .clone()
            .expect("start_static_simulation requires sim_static_rinex_path; caller must check");

        let lat: f64 = self.sim_static_lat.trim().parse().unwrap_or(0.0);
        let lon: f64 = self.sim_static_lon.trim().parse().unwrap_or(0.0);
        let alt: f64 = self.sim_static_alt.trim().parse().unwrap_or(10.0);
        let loop_duration = self.sim_static_loop_duration;

        let settings = crate::simulator::SimSettings {
            frequency: self.sim_frequency,
            txvga_gain: self.sim_txvga_gain,
            amp_enable: self.sim_amp_enable,
            start_time: if self.sim_start_time.trim().is_empty() {
                None
            } else {
                Some(self.sim_start_time.trim().to_owned())
            },
            time_override: self.sim_time_override,
            ionospheric_disable: self.sim_ionospheric_disable,
            fixed_gain: self.sim_fixed_gain_enable.then_some(self.sim_fixed_gain),
            center_frequency: self.sim_center_freq,
            baseband_filter: self
                .sim_baseband_filter_enable
                .then_some(self.sim_baseband_filter),
            leap: self.sim_leap_enable.then_some((
                self.sim_leap_week,
                self.sim_leap_day,
                self.sim_leap_delta,
            )),
            ppb: self.sim_ppb,
            elevation_mask_deg: self.sim_elevation_mask,
            blocked_prns: parse_blocked_prns(&self.sim_blocked_prns),
            log_path: self.sim_log_enable.then(|| {
                if self.sim_log_path.trim().is_empty() {
                    "sim_position_log.csv".to_owned()
                } else {
                    self.sim_log_path.trim().to_owned()
                }
            }),
            output_type: self.sim_output_type.clone(),
            iq_file_path: self.sim_iq_file_path.clone(),
            udp_addr: self.sim_udp_addr.clone(),
            tcp_port: self.sim_tcp_port,
            use_beidou: self.sim_use_beidou,
            use_galileo: self.sim_use_galileo,
        };

        let state = std::sync::Arc::clone(&self.sim_static_state);
        let stop = std::sync::Arc::clone(&self.sim_static_stop_flag);

        self.sim_static_thread = Some(std::thread::spawn(move || {
            crate::simulator::run_static_loop(
                &rinex_path,
                lat,
                lon,
                alt,
                loop_duration,
                &settings,
                &state,
                &stop,
            );
        }));
    }

    /// Spawns the interactive simulation worker thread.
    ///
    /// Resets shared state, builds [`crate::simulator::SimSettings`] from current
    /// UI values, resets the [`crate::gps_sim::InteractiveState`] to zero, and
    /// spawns a thread that runs the GPS signal generator driven by egui key events
    /// until the stop flag is set.
    pub fn start_interactive_simulation(&mut self) {
        use std::sync::atomic::Ordering;

        #[expect(
            clippy::unwrap_used,
            reason = "mutex poison means a prior panic; reset is best-effort"
        )]
        {
            *self.sim_interactive_state.lock().unwrap() = crate::simulator::SimState {
                status: crate::simulator::SimStatus::Running,
                ..crate::simulator::SimState::default()
            };
            *self.sim_interactive_istate.lock().unwrap() =
                crate::gps_sim::InteractiveState::default();
        }
        self.sim_interactive_stop_flag
            .store(false, Ordering::Relaxed);

        let rinex_path = self.sim_interactive_rinex_path.clone().expect(
            "start_interactive_simulation requires sim_interactive_rinex_path; caller must check",
        );

        let lat: f64 = self.sim_interactive_lat.trim().parse().unwrap_or(0.0);
        let lon: f64 = self.sim_interactive_lon.trim().parse().unwrap_or(0.0);
        let alt: f64 = self.sim_interactive_alt.trim().parse().unwrap_or(10.0);

        let settings = crate::simulator::SimSettings {
            frequency: self.sim_frequency,
            txvga_gain: self.sim_txvga_gain,
            amp_enable: self.sim_amp_enable,
            start_time: if self.sim_start_time.trim().is_empty() {
                None
            } else {
                Some(self.sim_start_time.trim().to_owned())
            },
            time_override: self.sim_time_override,
            ionospheric_disable: self.sim_ionospheric_disable,
            fixed_gain: self.sim_fixed_gain_enable.then_some(self.sim_fixed_gain),
            center_frequency: self.sim_center_freq,
            baseband_filter: self
                .sim_baseband_filter_enable
                .then_some(self.sim_baseband_filter),
            leap: self.sim_leap_enable.then_some((
                self.sim_leap_week,
                self.sim_leap_day,
                self.sim_leap_delta,
            )),
            ppb: self.sim_ppb,
            elevation_mask_deg: self.sim_elevation_mask,
            blocked_prns: parse_blocked_prns(&self.sim_blocked_prns),
            log_path: self.sim_log_enable.then(|| {
                if self.sim_log_path.trim().is_empty() {
                    "sim_position_log.csv".to_owned()
                } else {
                    self.sim_log_path.trim().to_owned()
                }
            }),
            output_type: self.sim_output_type.clone(),
            iq_file_path: self.sim_iq_file_path.clone(),
            udp_addr: self.sim_udp_addr.clone(),
            tcp_port: self.sim_tcp_port,
            use_beidou: self.sim_use_beidou,
            use_galileo: self.sim_use_galileo,
        };

        let state = std::sync::Arc::clone(&self.sim_interactive_state);
        let stop = std::sync::Arc::clone(&self.sim_interactive_stop_flag);
        let istate = std::sync::Arc::clone(&self.sim_interactive_istate);

        self.sim_interactive_thread = Some(std::thread::spawn(move || {
            crate::simulator::run_interactive(
                &rinex_path,
                lat,
                lon,
                alt,
                &settings,
                &state,
                &stop,
                istate,
            );
        }));
    }
}

/// Parse a comma-/space-separated list of PRN numbers (1–32) into a `Vec<u8>`.
fn parse_blocked_prns(s: &str) -> Vec<u8> {
    s.split(|c: char| c == ',' || c.is_whitespace())
        .filter_map(|token| token.trim().parse::<u8>().ok())
        .filter(|&prn| (1..=32).contains(&prn))
        .collect()
}
