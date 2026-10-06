//! Real native device binding. Device selection, stream construction, error
//! formatting, shutdown and recovery are control-thread operations.
#![forbid(unsafe_code)]

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{
    BufferSize, SampleFormat, SampleRate, SupportedBufferSize, SupportedStreamConfig,
    SupportedStreamConfigRange,
};
use std::time::Duration;
use wonderland_native_audio::{
    channel, Control, DeviceFault, DriverConfig, DriverError, WorkerThread,
};

#[derive(Clone, Debug, Default)]
pub struct OutputOptions {
    pub driver: DriverConfig,
    /// None selects the operating system default. A supplied exact name must
    /// resolve uniquely; there is no silent fallback to a different device.
    pub device_name: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DeviceInfo {
    pub name: String,
    pub sample_rate: u32,
    pub channels: u16,
    pub sample_format: String,
    pub ring_frames: usize,
    pub block_frames: usize,
    pub device_buffer_frames: u32,
}

#[derive(Debug)]
pub enum OpenError {
    NoDevice,
    AmbiguousDevice,
    UnsupportedStereoFormat,
    UnsupportedBufferBound,
    Backend(String),
    Transport(DriverError),
    Worker(std::io::Error),
}
impl std::fmt::Display for OpenError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for OpenError {}
impl From<DriverError> for OpenError {
    fn from(error: DriverError) -> Self {
        Self::Transport(error)
    }
}

fn preference(
    range: &SupportedStreamConfigRange,
    driver: &DriverConfig,
) -> Option<(u32, u8, u32, u32)> {
    if range.channels() != 2 {
        return None;
    }
    let format_rank = match range.sample_format() {
        SampleFormat::F32 => 0,
        SampleFormat::I16 => 1,
        SampleFormat::U16 => 2,
        _ => return None,
    };
    let min = range.min_sample_rate().0.max(1);
    let max = range.max_sample_rate().0.min(384_000);
    if min > max {
        return None;
    }
    let rate = driver.sample_rate.clamp(min, max);
    let capacity = driver.ring_frames.min(driver.max_callback_frames) as u32;
    let (buffer_min, buffer_max) = match *range.buffer_size() {
        SupportedBufferSize::Range { min, max } => (min.max(1), max.min(capacity)),
        // Without an advertised bound we cannot prove that the device's normal
        // buffer fits the ring. The caller gets an explicit configuration error.
        SupportedBufferSize::Unknown => return None,
    };
    if buffer_min > buffer_max {
        return None;
    }
    let desired = driver.block_frames.saturating_mul(4).min(capacity as usize) as u32;
    let buffer = desired.clamp(buffer_min, buffer_max);
    Some((rate.abs_diff(driver.sample_rate), format_rank, rate, buffer))
}

fn select_config(
    ranges: impl Iterator<Item = SupportedStreamConfigRange>,
    driver: &DriverConfig,
) -> Result<(SupportedStreamConfig, u32), OpenError> {
    ranges
        .filter_map(|range| preference(&range, driver).map(|rank| (rank, range)))
        .min_by_key(|(rank, _)| *rank)
        .map(|((_, _, rate, buffer), range)| (range.with_sample_rate(SampleRate(rate)), buffer))
        .ok_or(OpenError::UnsupportedBufferBound)
}

/// Own this on an application/control thread. Drop closes the native stream
/// before joining the mixer worker. Hardware reconnect creates a fresh session
/// and requires sample/cue rebinding by C's presentation owner.
pub struct NativeOutput {
    stream: Option<cpal::Stream>,
    worker: Option<WorkerThread>,
    control: Control,
    info: DeviceInfo,
    options: OutputOptions,
}
impl NativeOutput {
    pub fn open(mut options: OutputOptions) -> Result<Self, OpenError> {
        options.driver.validate()?;
        let requested_options = options.clone();
        let host = cpal::default_host();
        let device = if let Some(name) = &options.device_name {
            let mut found = None;
            for device in host
                .output_devices()
                .map_err(|e| OpenError::Backend(e.to_string()))?
            {
                if device
                    .name()
                    .map_err(|e| OpenError::Backend(e.to_string()))?
                    == *name
                {
                    if found.is_some() {
                        return Err(OpenError::AmbiguousDevice);
                    }
                    found = Some(device);
                }
            }
            found.ok_or(OpenError::NoDevice)?
        } else {
            host.default_output_device().ok_or(OpenError::NoDevice)?
        };
        let (supported, buffer_frames) = select_config(
            device
                .supported_output_configs()
                .map_err(|e| OpenError::Backend(e.to_string()))?,
            &options.driver,
        )?;
        let sample_format = supported.sample_format();
        let mut config = supported.config();
        config.buffer_size = BufferSize::Fixed(buffer_frames);
        // NativeMixer resamples off callback to this actual device rate. Its
        // phase never depends on callback count or the simulation tick clock.
        options.driver.sample_rate = config.sample_rate.0;
        options.driver.max_callback_frames = buffer_frames as usize;
        options.driver.block_frames = options
            .driver
            .block_frames
            .min(config.sample_rate.0 as usize);
        let info = DeviceInfo {
            name: device
                .name()
                .map_err(|e| OpenError::Backend(e.to_string()))?,
            sample_rate: config.sample_rate.0,
            channels: config.channels,
            sample_format: format!("{sample_format:?}"),
            ring_frames: options.driver.ring_frames,
            block_frames: options.driver.block_frames,
            device_buffer_frames: buffer_frames,
        };
        let (control, worker, mut callback) = channel(options.driver.clone())?;
        let fault = control.fault_signal();
        let error_callback = move |error: cpal::StreamError| {
            // No formatting, logging, allocation or application callback here.
            fault.report(match error {
                cpal::StreamError::DeviceNotAvailable => DeviceFault::Disconnected,
                _ => DeviceFault::Backend,
            });
        };
        let timeout = Some(Duration::from_secs(2));
        let stream = match sample_format {
            SampleFormat::F32 => device.build_output_stream(
                &config,
                move |data: &mut [f32], _| callback.write_f32(data),
                error_callback,
                timeout,
            ),
            SampleFormat::I16 => device.build_output_stream(
                &config,
                move |data: &mut [i16], _| callback.write_i16(data),
                error_callback,
                timeout,
            ),
            SampleFormat::U16 => device.build_output_stream(
                &config,
                move |data: &mut [u16], _| callback.write_u16(data),
                error_callback,
                timeout,
            ),
            _ => return Err(OpenError::UnsupportedStereoFormat),
        }
        .map_err(|e| OpenError::Backend(e.to_string()))?;
        let worker = worker.spawn().map_err(OpenError::Worker)?;
        stream
            .play()
            .map_err(|e| OpenError::Backend(e.to_string()))?;
        Ok(Self {
            stream: Some(stream),
            worker: Some(worker),
            control,
            info,
            options: requested_options,
        })
    }
    pub fn control(&mut self) -> &mut Control {
        &mut self.control
    }
    pub fn info(&self) -> &DeviceInfo {
        &self.info
    }
    /// Portable software suspension: keep the native callback alive and silent.
    /// CPAL hardware pause is deliberately not required by this transport.
    pub fn suspend(&mut self) {
        self.control.suspend();
    }
    pub fn resume(&mut self) -> Result<(), DriverError> {
        self.control.resume()
    }
    /// Reopen consumes the faulted stream and invalidates all of its sessions.
    /// The caller receives either a new device or an explicit open error.
    pub fn reopen(self) -> Result<Self, OpenError> {
        let options = self.options.clone();
        drop(self);
        Self::open(options)
    }
}
impl Drop for NativeOutput {
    fn drop(&mut self) {
        drop(self.stream.take());
        if let Some(worker) = self.worker.take() {
            let _ = worker.shutdown();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn range(
        channels: u16,
        min: u32,
        max: u32,
        format: SampleFormat,
    ) -> SupportedStreamConfigRange {
        SupportedStreamConfigRange::new(
            channels,
            SampleRate(min),
            SampleRate(max),
            SupportedBufferSize::Range { min: 16, max: 4096 },
            format,
        )
    }
    #[test]
    fn selection_prefers_exact_rate_then_supported_float_stereo() {
        let (config, buffer) = select_config(
            [
                range(1, 48_000, 48_000, SampleFormat::F32),
                range(2, 44_100, 44_100, SampleFormat::F32),
                range(2, 48_000, 48_000, SampleFormat::I16),
                range(2, 44_100, 96_000, SampleFormat::F32),
            ]
            .into_iter(),
            &DriverConfig::default(),
        )
        .unwrap();
        assert_eq!(config.sample_rate(), SampleRate(48_000));
        assert_eq!(config.sample_format(), SampleFormat::F32);
        assert_eq!(config.channels(), 2);
        assert_eq!(buffer, 1024);
    }
    #[test]
    fn nearest_supported_rate_is_explicit_and_unsupported_formats_are_rejected() {
        let (config, _) = select_config(
            [range(2, 44_100, 44_100, SampleFormat::U16)].into_iter(),
            &DriverConfig::default(),
        )
        .unwrap();
        assert_eq!(config.sample_rate(), SampleRate(44_100));
        assert!(matches!(
            select_config(
                [range(2, 48_000, 48_000, SampleFormat::F64)].into_iter(),
                &DriverConfig::default()
            ),
            Err(OpenError::UnsupportedBufferBound)
        ));
    }
    #[test]
    fn high_rate_device_buffer_fits_both_transport_budgets() {
        let driver = DriverConfig {
            sample_rate: 96_000,
            ..DriverConfig::default()
        };
        let (config, buffer) = select_config(
            [range(2, 96_000, 96_000, SampleFormat::F32)].into_iter(),
            &driver,
        )
        .unwrap();
        assert_eq!(config.sample_rate(), SampleRate(96_000));
        assert_eq!(buffer, 1024);
        assert!(buffer as usize <= driver.ring_frames);
        assert!(buffer as usize <= driver.max_callback_frames);
    }
    #[test]
    fn unknown_or_incompatible_minimum_buffer_is_rejected_before_open() {
        for size in [
            SupportedBufferSize::Unknown,
            SupportedBufferSize::Range {
                min: 4096,
                max: 8192,
            },
        ] {
            let range = SupportedStreamConfigRange::new(
                2,
                SampleRate(48_000),
                SampleRate(48_000),
                size,
                SampleFormat::F32,
            );
            assert!(matches!(
                select_config([range].into_iter(), &DriverConfig::default()),
                Err(OpenError::UnsupportedBufferBound)
            ));
        }
    }
}
