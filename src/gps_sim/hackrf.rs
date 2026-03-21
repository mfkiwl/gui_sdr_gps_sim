//! `HackRF` One USB driver — fully inlined, no external hardware-library crate.
//!
//! All USB control logic, constants, enums, and error types that previously
//! lived in the separate `libhackrf` crate are now embedded here.  The only
//! external dependency is [`nusb`], a pure-Rust USB library (no `libusb` C
//! library required).
//!
//! # Windows
//! `nusb` requires the `WinUSB` kernel driver for the `HackRF` device.
//! Use [Zadig](https://zadig.akeo.ie/) to replace the default `HackRF` driver
//! with `WinUSB` before running the simulator.
//!
//! # Module layout
//! - [`HackRF`] — raw USB device struct (open, configure, TX/RX mode).
//! - [`GpsHackRf`] — thin GPS-simulator wrapper around [`HackRF`].
//! - [`HackrfError`] — unified error type for all USB operations.
//! - Inline sub-modules: `constants`, `enums`.

// ── Constants ─────────────────────────────────────────────────────────────────

mod constants {
    /// USB Vendor ID for `HackRF` devices.
    pub const HACKRF_USB_VID: u16 = 0x1D50;
    /// USB Product ID for `HackRF` One.
    pub const HACKRF_ONE_USB_PID: u16 = 0x6089;
    /// USB bulk-IN endpoint address (device → host).
    pub const HACKRF_RX_ENDPOINT_ADDRESS: u8 = 0x81;
    /// USB bulk-OUT endpoint address (host → device).
    pub const HACKRF_TX_ENDPOINT_ADDRESS: u8 = 0x02;
    /// `HackRF` bulk-transfer buffer size: 262 144 bytes (256 KiB).
    pub const HACKRF_TRANSFER_BUFFER_SIZE: usize = 2 * 128 * 1024;
    /// 1 MHz in Hz.
    pub const MHZ: u64 = 1_000_000;
    /// Available baseband filter bandwidths for the MAX2837 transceiver (Hz).
    pub const MAX2837: [u32; 17] = [
        1_750_000, 2_500_000, 3_500_000, 5_000_000, 5_500_000, 6_000_000, 7_000_000, 8_000_000,
        9_000_000, 10_000_000, 12_000_000, 14_000_000, 15_000_000, 20_000_000, 24_000_000,
        28_000_000, 0,
    ];
}

// ── Enums ─────────────────────────────────────────────────────────────────────

mod enums {
    /// Current operating mode of the `HackRF` device.
    #[derive(Debug)]
    pub enum DeviceMode {
        Off,
        Tx,
        Rx,
    }

    /// Transceiver operating mode sent via USB control request.
    #[repr(u8)]
    pub enum TransceiverMode {
        Off = 0,
        Receive = 1,
        Transmit = 2,
        Ss = 3,
        CpldUpdate = 4,
        RxSweep = 5,
    }

    impl From<TransceiverMode> for u16 {
        fn from(m: TransceiverMode) -> Self {
            m as Self
        }
    }

    /// USB vendor request codes for `HackRF` device operations.
    #[repr(u8)]
    pub enum Request {
        SetTransceiverMode = 1,
        Max2837Write = 2,
        Max2837Read = 3,
        Si5351CWrite = 4,
        Si5351CRead = 5,
        SampleRateSet = 6,
        BasebandFilterBandwidthSet = 7,
        Rffc5071Write = 8,
        Rffc5071Read = 9,
        SpiflashErase = 10,
        SpiflashWrite = 11,
        SpiflashRead = 12,
        BoardIdRead = 14,
        VersionStringRead = 15,
        SetFreq = 16,
        AmpEnable = 17,
        BoardPartidSerialnoRead = 18,
        SetLnaGain = 19,
        SetVgaGain = 20,
        SetTxvgaGain = 21,
        AntennaEnable = 23,
        SetFreqExplicit = 24,
        UsbWcidVendorReq = 25,
        InitSweep = 26,
        OperacakeGetBoards = 27,
        OperacakeSetPorts = 28,
        SetHwSyncMode = 29,
        Reset = 30,
        OperacakeSetRanges = 31,
        ClkoutEnable = 32,
        SpiflashStatus = 33,
        SpiflashClearStatus = 34,
        OperacakeGpioTest = 35,
        CpldChecksum = 36,
        UiEnable = 37,
    }

    impl From<Request> for u8 {
        fn from(r: Request) -> Self {
            r as Self
        }
    }
}

// ── Error ─────────────────────────────────────────────────────────────────────

/// Error type for all `HackRF` USB operations.
#[derive(Debug, thiserror::Error)]
pub enum HackrfError {
    /// USB bus error (e.g., device not found, access denied).
    #[error("USB error: {0}")]
    Usb(#[from] nusb::Error),

    /// No `HackRF` device is connected.
    #[error("No HackRF device found")]
    InvalidDevice,

    /// No device with the given serial number is connected.
    #[error("No HackRF device with serial number '{0}' found")]
    InvalidSerialNumber(String),

    /// Device firmware is too old for the requested operation.
    #[error("Device firmware {device} is older than required {minimal}")]
    VersionMismatch { device: u16, minimal: u16 },

    /// USB bulk/control transfer error.
    #[error("USB transfer error: {0}")]
    Transfer(#[from] nusb::transfer::TransferError),

    /// Control transfer returned wrong byte count.
    #[error("Control transfer ({direction:?}): got {actual} B, expected {expected} B")]
    ControlTransfer {
        direction: nusb::transfer::Direction,
        actual: usize,
        expected: usize,
    },

    /// Byte-slice to fixed-size array conversion failed.
    #[error("Slice/array conversion error: {0}")]
    TryFromSlice(#[from] std::array::TryFromSliceError),

    /// Parameter value is out of range.
    #[error("Invalid argument")]
    Argument,

    /// UTF-8 decoding of a firmware version string failed.
    #[error("UTF-8 conversion error: {0}")]
    Utf8Error(#[from] std::string::FromUtf8Error),

    /// `std::fmt::Write` formatting error.
    #[error("Formatting error: {0}")]
    Fmt(#[from] std::fmt::Error),
}

// ── HackRF device ─────────────────────────────────────────────────────────────

use constants::{
    HACKRF_ONE_USB_PID, HACKRF_RX_ENDPOINT_ADDRESS, HACKRF_TRANSFER_BUFFER_SIZE,
    HACKRF_TX_ENDPOINT_ADDRESS, HACKRF_USB_VID, MAX2837, MHZ,
};
use enums::{DeviceMode, Request, TransceiverMode};
use nusb::{
    Device, DeviceInfo, Endpoint, Interface, MaybeFuture as _,
    transfer::{ControlIn, ControlOut, ControlType, Recipient},
};
use std::time::Duration;

/// Raw USB interface to a `HackRF` One.
///
/// Provides device discovery, configuration (frequency, sample rate, gain),
/// and TX/RX mode switching.  All operations are synchronous — they call
/// `.wait()` on the underlying `nusb` futures.
pub struct HackRF {
    mode: DeviceMode,
    #[expect(
        unused,
        reason = "Device must be kept alive to hold the USB interface open; it is not directly accessed after open"
    )]
    device: Device,
    device_version: u16,
    interface: Interface,
}

impl std::fmt::Debug for HackRF {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "HackRF {{ mode: {:?}, fw: {} }}",
            self.mode, self.device_version
        )
    }
}

impl HackRF {
    /// Open the first connected `HackRF` One.
    ///
    /// # Errors
    /// Returns [`HackrfError::InvalidDevice`] if no device is found,
    /// or [`HackrfError::Usb`] if the device cannot be opened.
    pub fn new_auto() -> Result<Self, HackrfError> {
        let devices = Self::list_devices()?;
        let deviceinfo = devices.first().ok_or(HackrfError::InvalidDevice)?;
        let device_version = deviceinfo.device_version();
        let device = deviceinfo.open().wait()?;
        let interface = device.claim_interface(0).wait()?;
        Ok(Self {
            mode: DeviceMode::Off,
            device,
            device_version,
            interface,
        })
    }

    /// Open a specific `HackRF` One by serial number.
    ///
    /// # Errors
    /// Returns [`HackrfError::InvalidSerialNumber`] if the serial is not found,
    /// or [`HackrfError::Usb`] if the device cannot be opened.
    pub fn new(serial_number: &dyn AsRef<str>) -> Result<Self, HackrfError> {
        let devices = Self::list_devices()?;
        let deviceinfo = devices
            .iter()
            .find(|d| {
                d.serial_number()
                    .is_some_and(|sn| sn.eq_ignore_ascii_case(serial_number.as_ref()))
            })
            .ok_or_else(|| HackrfError::InvalidSerialNumber(serial_number.as_ref().to_owned()))?;
        let device_version = deviceinfo.device_version();
        let device = deviceinfo.open().wait()?;
        let interface = device.claim_interface(0).wait()?;
        Ok(Self {
            mode: DeviceMode::Off,
            device,
            device_version,
            interface,
        })
    }

    /// List all connected `HackRF` One devices.
    ///
    /// # Errors
    /// Returns [`HackrfError::Usb`] if the USB enumeration fails.
    pub fn list_devices() -> Result<Vec<DeviceInfo>, HackrfError> {
        Ok(nusb::list_devices()
            .wait()?
            .filter(|d| d.vendor_id() == HACKRF_USB_VID && d.product_id() == HACKRF_ONE_USB_PID)
            .collect())
    }

    /// Maximum transfer unit for USB bulk transfers (262 144 bytes).
    pub fn max_transmission_unit(&self) -> usize {
        HACKRF_TRANSFER_BUFFER_SIZE
    }

    fn check_api_version(&self, minimal: u16) -> Result<(), HackrfError> {
        if self.device_version >= minimal {
            Ok(())
        } else {
            Err(HackrfError::VersionMismatch {
                device: self.device_version,
                minimal,
            })
        }
    }

    /// Return the firmware version word.
    pub fn device_version(&self) -> u16 {
        self.device_version
    }

    fn read_control<const N: u16>(
        &self,
        request: Request,
        value: u16,
        index: u16,
    ) -> Result<Vec<u8>, HackrfError> {
        Ok(self
            .interface
            .control_in(
                ControlIn {
                    control_type: ControlType::Vendor,
                    recipient: Recipient::Device,
                    request: request.into(),
                    value,
                    index,
                    length: N,
                },
                Duration::from_secs(1),
            )
            .wait()?)
    }

    fn write_control(
        &self,
        request: Request,
        value: u16,
        index: u16,
        data: &[u8],
    ) -> Result<(), HackrfError> {
        self.interface
            .control_out(
                ControlOut {
                    control_type: ControlType::Vendor,
                    recipient: Recipient::Device,
                    request: request.into(),
                    value,
                    index,
                    data,
                },
                Duration::from_secs(1),
            )
            .wait()?;
        Ok(())
    }

    /// Read the board ID byte.
    ///
    /// # Errors
    /// Returns [`HackrfError::Transfer`] if the USB control transfer fails.
    pub fn board_id(&self) -> Result<u8, HackrfError> {
        let data = self.read_control::<1>(Request::BoardIdRead, 0, 0)?;
        Ok(*data.first().unwrap_or(&0))
    }

    /// Read the 32-byte part ID and serial number.
    ///
    /// # Errors
    /// Returns [`HackrfError::Transfer`] if the USB control transfer fails,
    /// or [`HackrfError::TryFromSlice`] if the response is malformed.
    pub fn part_id_serial_read(self) -> Result<((u32, u32), String), HackrfError> {
        let data = self.read_control::<32>(Request::BoardPartidSerialnoRead, 0, 0)?;
        let part_id_1 = data
            .get(0..4)
            .and_then(|s| s.try_into().ok())
            .map(u32::from_le_bytes)
            .unwrap_or(0);
        let part_id_2 = data
            .get(4..8)
            .and_then(|s| s.try_into().ok())
            .map(u32::from_le_bytes)
            .unwrap_or(0);
        let mut serial = String::new();
        for i in 0..4 {
            if let Some(slice) = data.get(8 + 4 * i..12 + 4 * i) {
                if let Ok(bytes) = <[u8; 4]>::try_from(slice) {
                    use std::fmt::Write as _;
                    write!(serial, "{:08x?}", u32::from_le_bytes(bytes))?;
                }
            }
        }
        Ok(((part_id_1, part_id_2), serial))
    }

    /// Read the firmware version string (up to 16 bytes).
    ///
    /// # Errors
    /// Returns [`HackrfError::Transfer`] if the USB control transfer fails.
    pub fn version(&self) -> Result<String, HackrfError> {
        let data = self.read_control::<16>(Request::VersionStringRead, 0, 0)?;
        Ok(String::from_utf8_lossy(&data).into())
    }

    /// Enable or disable the RF amplifier.
    ///
    /// # Errors
    /// Returns [`HackrfError::Transfer`] if the USB control transfer fails.
    pub fn set_amp_enable(&mut self, en: bool) -> Result<(), HackrfError> {
        self.write_control(Request::AmpEnable, en.into(), 0, &[])
    }

    /// Set the RF centre frequency in Hz.
    ///
    /// # Errors
    /// Returns [`HackrfError::Transfer`] if the USB control transfer fails.
    pub fn set_freq(&mut self, hz: u64) -> Result<(), HackrfError> {
        self.write_control(Request::SetFreq, 0, 0, &freq_params(hz))
    }

    /// Set the baseband filter bandwidth in Hz.
    ///
    /// # Errors
    /// Returns [`HackrfError::Transfer`] if the USB control transfer fails.
    pub fn set_baseband_filter_bandwidth(&mut self, hz: u32) -> Result<(), HackrfError> {
        self.write_control(
            Request::BasebandFilterBandwidthSet,
            (hz & 0xFFFF) as u16,
            (hz >> 16) as u16,
            &[],
        )
    }

    /// Set sample rate with explicit frequency and divider (also sets BBF BW).
    ///
    /// # Errors
    /// Returns [`HackrfError::Transfer`] if any USB control transfer fails.
    pub fn set_sample_rate_manual(
        &mut self,
        freq_hz: u32,
        divider: u32,
    ) -> Result<(), HackrfError> {
        let mut bytes = [0u8; 8];
        bytes[0..4].copy_from_slice(&freq_hz.to_le_bytes());
        bytes[4..8].copy_from_slice(&divider.to_le_bytes());
        self.write_control(Request::SampleRateSet, 0, 0, &bytes)?;
        let bw = compute_baseband_filter_bw((0.75 * freq_hz as f32 / divider as f32) as u32);
        self.set_baseband_filter_bandwidth(bw)
    }

    /// Set sample rate automatically from a floating-point Hz value.
    ///
    /// Finds the best integer `(freq_hz, divider)` pair and also sets the
    /// baseband filter to 75% of the sample rate.
    ///
    /// # Errors
    /// Returns [`HackrfError::Transfer`] if any USB control transfer fails.
    pub fn set_sample_rate_auto(&mut self, freq: f64) -> Result<(), HackrfError> {
        const MAX_N: usize = 32;
        let freq_frac = 1.0 + freq.fract();
        let mut acc = 0u64;
        let mut mult = 1usize;
        let exp = ((freq.to_bits() >> 52) & 0x7FF) as i32 - 1023;
        let mut mask = (1u64 << 52) - 1;
        let mut frac_b = freq_frac.to_bits();
        frac_b &= mask;
        mask &= !((1u64 << (exp + 4)) - 1);
        for ii in 1..=MAX_N {
            mult = ii;
            acc += frac_b;
            if acc & mask == 0 || !acc & mask == 0 {
                break;
            }
        }
        if mult == MAX_N {
            mult = 1;
        }
        let freq_hz = (freq * mult as f64).round() as u32;
        self.set_sample_rate_manual(freq_hz, mult as u32)
    }

    /// Set TX VGA gain (0–47 dB).
    ///
    /// # Errors
    /// Returns [`HackrfError::Argument`] if `value > 47` or the device rejects it.
    pub fn set_txvga_gain(&mut self, value: u16) -> Result<(), HackrfError> {
        if value > 47 {
            return Err(HackrfError::Argument);
        }
        let buf = self.read_control::<1>(Request::SetTxvgaGain, 0, value)?;
        if buf.first().copied().unwrap_or(0) == 0 {
            Err(HackrfError::Argument)
        } else {
            Ok(())
        }
    }

    /// Set RX LNA gain (0–40 dB, stepped by 8).
    ///
    /// # Errors
    /// Returns [`HackrfError::Argument`] if `value > 40` or the device rejects it.
    pub fn set_lna_gain(&mut self, value: u16) -> Result<(), HackrfError> {
        if value > 40 {
            return Err(HackrfError::Argument);
        }
        let buf = self.read_control::<1>(Request::SetLnaGain, 0, value & !0x07)?;
        if buf.first().copied().unwrap_or(0) == 0 {
            Err(HackrfError::Argument)
        } else {
            Ok(())
        }
    }

    /// Set RX VGA gain (0–62 dB, stepped by 2).
    ///
    /// # Errors
    /// Returns [`HackrfError::Argument`] if `value > 62` or the device rejects it.
    pub fn set_vga_gain(&mut self, value: u16) -> Result<(), HackrfError> {
        if value > 62 {
            return Err(HackrfError::Argument);
        }
        let buf = self.read_control::<1>(Request::SetVgaGain, 0, value & !0b1)?;
        if buf.first().copied().unwrap_or(0) == 0 {
            Err(HackrfError::Argument)
        } else {
            Ok(())
        }
    }

    /// Enable or disable the antenna port power supply.
    ///
    /// # Errors
    /// Returns [`HackrfError::Transfer`] if the USB control transfer fails.
    pub fn set_antenna_enable(&mut self, value: u8) -> Result<(), HackrfError> {
        self.write_control(Request::AntennaEnable, value.into(), 0, &[])
    }

    /// Enable or disable the clock output (requires firmware ≥ 0x0103).
    ///
    /// # Errors
    /// Returns [`HackrfError::VersionMismatch`] if firmware is too old,
    /// or [`HackrfError::Transfer`] if the USB control transfer fails.
    pub fn set_clkout_enable(&mut self, value: bool) -> Result<(), HackrfError> {
        self.check_api_version(0x0103)?;
        self.write_control(Request::ClkoutEnable, value.into(), 0, &[])
    }

    /// Set the hardware synchronisation mode.
    ///
    /// # Errors
    /// Returns [`HackrfError::Transfer`] if the USB control transfer fails.
    pub fn set_hw_sync_mode(&mut self, value: u8) -> Result<(), HackrfError> {
        self.write_control(Request::SetHwSyncMode, value.into(), 0, &[])
    }

    fn set_transceiver_mode(&self, mode: TransceiverMode) -> Result<(), HackrfError> {
        self.write_control(Request::SetTransceiverMode, mode.into(), 0, &[])
    }

    /// Switch to receive mode.
    ///
    /// # Errors
    /// Returns [`HackrfError::Transfer`] if the USB control transfer fails.
    pub fn enter_rx_mode(&mut self) -> Result<(), HackrfError> {
        self.set_transceiver_mode(TransceiverMode::Receive)?;
        self.mode = DeviceMode::Rx;
        Ok(())
    }

    /// Switch to transmit mode.
    ///
    /// # Errors
    /// Returns [`HackrfError::Transfer`] if the USB control transfer fails.
    pub fn enter_tx_mode(&mut self) -> Result<(), HackrfError> {
        self.set_transceiver_mode(TransceiverMode::Transmit)?;
        self.mode = DeviceMode::Tx;
        Ok(())
    }

    /// Claim the bulk-IN endpoint (RX data, device → host).
    ///
    /// # Errors
    /// Returns [`HackrfError::Transfer`] if the endpoint cannot be claimed.
    pub fn rx_queue(
        &mut self,
    ) -> Result<Endpoint<nusb::transfer::Bulk, nusb::transfer::In>, HackrfError> {
        Ok(self.interface.endpoint(HACKRF_RX_ENDPOINT_ADDRESS)?)
    }

    /// Claim the bulk-OUT endpoint (TX data, host → device).
    ///
    /// # Errors
    /// Returns [`HackrfError::Transfer`] if the endpoint cannot be claimed.
    pub fn tx_queue(
        &mut self,
    ) -> Result<Endpoint<nusb::transfer::Bulk, nusb::transfer::Out>, HackrfError> {
        Ok(self.interface.endpoint(HACKRF_TX_ENDPOINT_ADDRESS)?)
    }

    /// Stop receive mode and return to idle.
    ///
    /// # Errors
    /// Returns [`HackrfError::Transfer`] if the USB control transfer fails.
    pub fn stop_rx(&mut self) -> Result<(), HackrfError> {
        self.set_transceiver_mode(TransceiverMode::Off)?;
        self.mode = DeviceMode::Off;
        Ok(())
    }

    /// Stop transmit mode and return to idle.
    ///
    /// # Errors
    /// Returns [`HackrfError::Transfer`] if the USB control transfer fails.
    pub fn stop_tx(&mut self) -> Result<(), HackrfError> {
        self.set_transceiver_mode(TransceiverMode::Off)?;
        self.mode = DeviceMode::Off;
        Ok(())
    }

    /// Reset the device (requires firmware ≥ 0x0102).  Consumes `self`.
    ///
    /// # Errors
    /// Returns [`HackrfError::VersionMismatch`] if firmware is too old,
    /// or [`HackrfError::Transfer`] if the USB control transfer fails.
    pub fn reset(mut self) -> Result<(), HackrfError> {
        self.check_api_version(0x0102)?;
        self.write_control(Request::Reset, 0, 0, &[])?;
        self.mode = DeviceMode::Off;
        Ok(())
    }
}

/// Encode a frequency in Hz as the 8-byte `[freq_mhz_le, freq_hz_le]` payload.
fn freq_params(hz: u64) -> [u8; 8] {
    let mhz = (hz / MHZ) as u32;
    let rem = (hz % MHZ) as u32;
    let mut b = [0u8; 8];
    b[0..4].copy_from_slice(&mhz.to_le_bytes());
    b[4..8].copy_from_slice(&rem.to_le_bytes());
    b
}

/// Select the largest MAX2837 bandwidth ≤ `bandwidth_hz`.
fn compute_baseband_filter_bw(bandwidth_hz: u32) -> u32 {
    let mut p = 0u32;
    let mut ix = 0usize;
    for (i, &v) in MAX2837.iter().enumerate() {
        if v >= bandwidth_hz {
            p = v;
            ix = i;
            break;
        }
    }
    if ix != 0 && p > bandwidth_hz {
        p = MAX2837.get(ix - 1).copied().unwrap_or(0);
    }
    p
}

// ── GPS-simulator wrapper ─────────────────────────────────────────────────────

use super::error::SimError;
use super::types::consts::SAMPLE_RATE;

/// GPS simulator wrapper around a `HackRF` One device.
///
/// Opens, configures (frequency + sample rate + gain), enters TX mode, and
/// vends the bulk-OUT endpoint for IQ streaming.
pub struct GpsHackRf {
    inner: HackRF,
}

impl GpsHackRf {
    /// Open the first available `HackRF` One.
    ///
    /// # Errors
    /// Returns [`SimError::HackRf`] if no device is found or USB fails.
    pub fn open() -> Result<Self, SimError> {
        Ok(Self {
            inner: HackRF::new_auto()?,
        })
    }

    /// Configure the `HackRF` for GPS L1 transmission.
    ///
    /// Sets the carrier frequency (with PPB correction), sample rate,
    /// TX VGA gain, and amplifier state.
    ///
    /// # Parameters
    /// - `gain_db`:         TX VGA gain 0–47 dB.
    /// - `amp`:             Enable the RF power amplifier (~11 dB; use with caution).
    /// - `ppb`:             Oscillator offset in parts-per-billion (positive = runs fast).
    /// - `sample_rate`:     Override the sample rate in Hz (defaults to 3 MSPS).
    /// - `center_freq`:     Override the centre frequency in Hz (defaults to GPS L1 C/A).
    /// - `baseband_filter`: Override the baseband filter bandwidth in Hz (auto when `None`).
    ///
    /// # Errors
    /// Returns [`SimError::HackRf`] if any USB control transfer fails.
    pub fn configure(
        &mut self,
        gain_db: i32,
        amp: bool,
        ppb: i32,
        sample_rate: Option<f64>,
        center_freq: Option<u64>,
        baseband_filter: Option<u32>,
    ) -> Result<(), SimError> {
        const BASE_HZ: u64 = 1_575_420_000;
        let center = center_freq.unwrap_or(BASE_HZ);
        let correction = (center as f64 * ppb as f64 / 1_000_000_000.0) as i64;
        let freq_hz = (center as i64 - correction) as u64;

        self.inner.set_freq(freq_hz)?;
        let rate = sample_rate.unwrap_or(SAMPLE_RATE);
        self.inner.set_sample_rate_auto(rate)?;
        if let Some(bw) = baseband_filter {
            self.inner.set_baseband_filter_bandwidth(bw)?;
        }
        self.inner.set_txvga_gain(gain_db.clamp(0, 47) as u16)?;
        self.inner.set_amp_enable(amp)?;
        self.inner.set_antenna_enable(0)?;
        self.inner.set_hw_sync_mode(0u8)?;
        Ok(())
    }

    /// Switch to TX mode and return the bulk-OUT endpoint for IQ streaming.
    ///
    /// The returned endpoint supports `.submit(data)` +
    /// `.wait_next_complete(timeout)` for pipelined USB transfers.
    ///
    /// # Errors
    /// Returns [`SimError::HackRf`] if entering TX mode or claiming the endpoint fails.
    pub fn enter_tx(
        &mut self,
    ) -> Result<nusb::Endpoint<nusb::transfer::Bulk, nusb::transfer::Out>, SimError> {
        self.inner.enter_tx_mode()?;
        Ok(self.inner.tx_queue()?)
    }

    /// Stop TX mode and return the device to idle.
    ///
    /// # Errors
    /// Returns [`SimError::HackRf`] if the USB control transfer fails.
    pub fn stop_tx(&mut self) -> Result<(), SimError> {
        Ok(self.inner.stop_tx()?)
    }

    /// Maximum USB bulk-transfer size in bytes (262 144).
    pub fn mtu(&self) -> usize {
        self.inner.max_transmission_unit()
    }
}

// ── Hardware tests (ignored; require physical HackRF) ─────────────────────────

#[cfg(test)]
mod tests {
    use super::HackRF;

    #[test]
    #[ignore = "Requires HackRF hardware"]
    #[expect(clippy::unwrap_used, reason = "test code; hardware required")]
    #[expect(clippy::print_stdout, reason = "diagnostic test output")]
    fn list_device() {
        let devices = HackRF::list_devices().unwrap();
        println!("Found {} devices", devices.len());
    }

    #[test]
    #[ignore = "Requires HackRF hardware"]
    #[expect(clippy::unwrap_used, reason = "test code; hardware required")]
    #[expect(clippy::print_stdout, reason = "diagnostic test output")]
    fn hackrf_info() {
        let sdr = HackRF::new_auto().unwrap();
        println!("Board ID   : {}", sdr.board_id().unwrap());
        println!("FW version : {}", sdr.version().unwrap());
        println!("API version: {}", sdr.device_version());
    }

    #[test]
    #[ignore = "Requires HackRF hardware"]
    #[expect(clippy::unwrap_used, reason = "test code; hardware required")]
    fn hackrf_setting() {
        let mut sdr = HackRF::new_auto().unwrap();
        sdr.set_freq(1_575_420_000).unwrap();
    }

    #[test]
    #[ignore = "Requires HackRF hardware"]
    #[expect(clippy::unwrap_used, reason = "test code; hardware required")]
    fn hackrf_reset() {
        HackRF::new_auto().unwrap().reset().unwrap();
    }
}
