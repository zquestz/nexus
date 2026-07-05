//! Audio device management and streaming
//!
//! Provides audio device enumeration, microphone capture, and speaker playback
//! using the cpal crate for cross-platform audio I/O. Uses f32 samples throughout
//! for compatibility with WebRTC audio processing.
//!
//! Supports resampling for devices that don't natively support 48kHz (required
//! by Opus codec) using the rubato crate.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc as std_mpsc;
use std::sync::{Arc, Mutex};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{Device, ErrorKind, FromSample, Host, Sample, SampleFormat, Stream, StreamConfig};

use nexus_common::names::fold_name;
use nexus_common::voice::{
    MONO_CHANNELS, STEREO_CHANNELS, VOICE_SAMPLE_RATE, VOICE_SAMPLES_PER_FRAME,
};

use super::resample::{InputResampler, OutputResampler, needs_resampling};
use crate::constants::ERR_MIXER_BUFFER_MISSING_AFTER_INSERT;

// =============================================================================
// Constants
// =============================================================================

/// System default device display name
pub const SYSTEM_DEFAULT_DEVICE_NAME: &str = "System Default";

/// Scaling factor for RMS to UI level conversion (provides headroom for typical speech)
const RMS_DISPLAY_SCALE: f64 = 2.0;

/// Maximum capture buffer size in frames (prevents unbounded growth if processing stalls)
const MAX_CAPTURE_BUFFER_FRAMES: usize = 10;

/// Maximum playback buffer size in frames (prevents latency buildup)
const MAX_PLAYBACK_BUFFER_FRAMES: usize = 20;

/// Supported sample formats in order of preference (best quality first)
const SUPPORTED_FORMATS: [SampleFormat; 3] =
    [SampleFormat::F32, SampleFormat::I16, SampleFormat::U16];

/// Mono channel count for audio device configuration
const MONO: u16 = MONO_CHANNELS;

/// Stereo channel count for audio device configuration
const STEREO: u16 = STEREO_CHANNELS;

/// Soft clip gain factor - provides ~3dB of headroom before noticeable saturation
const SOFT_CLIP_GAIN: f32 = 0.7;

// =============================================================================
// Audio Device
// =============================================================================

/// Represents an audio device (input or output)
#[derive(Debug, Clone)]
pub struct AudioDevice {
    /// Device name for display
    pub name: String,
    /// Whether this represents the system default device
    pub is_default: bool,
}

impl AudioDevice {
    /// Create a new audio device entry
    pub fn new(name: String, is_default: bool) -> Self {
        Self { name, is_default }
    }

    /// Create the system default device entry
    pub fn system_default() -> Self {
        Self {
            name: SYSTEM_DEFAULT_DEVICE_NAME.to_string(),
            is_default: true,
        }
    }
}

impl std::fmt::Display for AudioDevice {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.name)
    }
}

impl PartialEq for AudioDevice {
    fn eq(&self, other: &Self) -> bool {
        self.name == other.name
    }
}

impl Eq for AudioDevice {}

// =============================================================================
// Device Enumeration
// =============================================================================

/// Get the default audio host for the platform
fn get_host() -> Host {
    cpal::default_host()
}

/// List available audio output devices
///
/// Returns a list of output devices with "System Default" as the first entry.
pub fn list_output_devices() -> Vec<AudioDevice> {
    let host = get_host();
    let mut names = Vec::new();

    if let Ok(output_devices) = host.output_devices() {
        for device in output_devices {
            if let Ok(desc) = device.description() {
                names.push(desc.name().to_string());
            }
        }
    }

    audio_device_list_from_names(names)
}

/// List available audio input devices
///
/// Returns a list of input devices with "System Default" as the first entry.
pub fn list_input_devices() -> Vec<AudioDevice> {
    let host = get_host();
    let mut names = Vec::new();

    if let Ok(input_devices) = host.input_devices() {
        for device in input_devices {
            if let Ok(desc) = device.description() {
                names.push(desc.name().to_string());
            }
        }
    }

    audio_device_list_from_names(names)
}

fn audio_device_list_from_names<I>(names: I) -> Vec<AudioDevice>
where
    I: IntoIterator<Item = String>,
{
    let mut devices = vec![AudioDevice::system_default()];

    for name in names {
        if !devices.iter().any(|d| d.name == name) {
            devices.push(AudioDevice::new(name, false));
        }
    }

    devices
}

/// Find an output device by name, or return the default
fn find_output_device(name: &str) -> Option<Device> {
    let host = get_host();

    if name.is_empty() || name == SYSTEM_DEFAULT_DEVICE_NAME {
        return host.default_output_device();
    }

    host.output_devices()
        .ok()?
        .find(|d| d.description().is_ok_and(|desc| desc.name() == name))
        .or_else(|| host.default_output_device())
}

/// Find an input device by name, or return the default
fn find_input_device(name: &str) -> Option<Device> {
    let host = get_host();

    if name.is_empty() || name == SYSTEM_DEFAULT_DEVICE_NAME {
        return host.default_input_device();
    }

    host.input_devices()
        .ok()?
        .find(|d| d.description().is_ok_and(|desc| desc.name() == name))
        .or_else(|| host.default_input_device())
}

// =============================================================================
// Device Configuration Selection
// =============================================================================

/// Configuration for audio stream
struct AudioConfig {
    channels: u16,
    sample_rate: u32,
    sample_format: SampleFormat,
}

/// Find the best input configuration for a device
///
/// Priority:
/// 1. Mono at 48kHz with best format (no resampling needed)
/// 2. Stereo at 48kHz with best format (no resampling needed)
/// 3. Mono at any rate with best format (will resample)
/// 4. Stereo at any rate with best format (will resample)
fn find_best_input_config(device: &Device) -> Result<AudioConfig, String> {
    let configs: Vec<_> = device
        .supported_input_configs()
        .map_err(|e| format!("Failed to get supported input configs: {}", e))?
        .collect();

    if configs.is_empty() {
        return Err("Input device has no supported configurations".to_string());
    }

    // Try 48kHz first (no resampling needed)
    // Mono at 48kHz
    for format in &SUPPORTED_FORMATS {
        if let Some(_cfg) = configs.iter().find(|c| {
            c.channels() == MONO
                && c.min_sample_rate() <= VOICE_SAMPLE_RATE
                && c.max_sample_rate() >= VOICE_SAMPLE_RATE
                && c.sample_format() == *format
        }) {
            return Ok(AudioConfig {
                channels: MONO,
                sample_rate: VOICE_SAMPLE_RATE,
                sample_format: *format,
            });
        }
    }

    // Stereo at 48kHz
    for format in &SUPPORTED_FORMATS {
        if let Some(_cfg) = configs.iter().find(|c| {
            c.channels() == STEREO
                && c.min_sample_rate() <= VOICE_SAMPLE_RATE
                && c.max_sample_rate() >= VOICE_SAMPLE_RATE
                && c.sample_format() == *format
        }) {
            return Ok(AudioConfig {
                channels: STEREO,
                sample_rate: VOICE_SAMPLE_RATE,
                sample_format: *format,
            });
        }
    }

    // Need resampling - find best mono config at any rate
    for format in &SUPPORTED_FORMATS {
        if let Some(cfg) = configs
            .iter()
            .find(|c| c.channels() == MONO && c.sample_format() == *format)
        {
            // Use device's max supported rate for best quality before downsampling
            return Ok(AudioConfig {
                channels: MONO,
                sample_rate: cfg.max_sample_rate(),
                sample_format: *format,
            });
        }
    }

    // Find best stereo config at any rate (will downmix and resample)
    for format in &SUPPORTED_FORMATS {
        if let Some(cfg) = configs
            .iter()
            .find(|c| c.channels() == STEREO && c.sample_format() == *format)
        {
            return Ok(AudioConfig {
                channels: STEREO,
                sample_rate: cfg.max_sample_rate(),
                sample_format: *format,
            });
        }
    }

    Err("Input device has no supported audio configuration".to_string())
}

/// Find the best output configuration for a device
///
/// Priority:
/// 1. Stereo at 48kHz with best format (no resampling needed)
/// 2. Mono at 48kHz with best format (no resampling needed)
/// 3. Stereo at any rate with best format (will resample)
/// 4. Mono at any rate with best format (will resample)
fn find_best_output_config(device: &Device) -> Result<AudioConfig, String> {
    let configs: Vec<_> = device
        .supported_output_configs()
        .map_err(|e| format!("Failed to get supported output configs: {}", e))?
        .collect();

    if configs.is_empty() {
        return Err("Output device has no supported configurations".to_string());
    }

    // Try 48kHz first (no resampling needed)
    // Stereo at 48kHz (preferred for output)
    for format in &SUPPORTED_FORMATS {
        if let Some(_cfg) = configs.iter().find(|c| {
            c.channels() == STEREO
                && c.min_sample_rate() <= VOICE_SAMPLE_RATE
                && c.max_sample_rate() >= VOICE_SAMPLE_RATE
                && c.sample_format() == *format
        }) {
            return Ok(AudioConfig {
                channels: STEREO,
                sample_rate: VOICE_SAMPLE_RATE,
                sample_format: *format,
            });
        }
    }

    // Mono at 48kHz
    for format in &SUPPORTED_FORMATS {
        if let Some(_cfg) = configs.iter().find(|c| {
            c.channels() == MONO
                && c.min_sample_rate() <= VOICE_SAMPLE_RATE
                && c.max_sample_rate() >= VOICE_SAMPLE_RATE
                && c.sample_format() == *format
        }) {
            return Ok(AudioConfig {
                channels: MONO,
                sample_rate: VOICE_SAMPLE_RATE,
                sample_format: *format,
            });
        }
    }

    // Need resampling - find best stereo config at any rate
    for format in &SUPPORTED_FORMATS {
        if let Some(cfg) = configs
            .iter()
            .find(|c| c.channels() == STEREO && c.sample_format() == *format)
        {
            return Ok(AudioConfig {
                channels: STEREO,
                sample_rate: cfg.max_sample_rate(),
                sample_format: *format,
            });
        }
    }

    // Find best mono config at any rate
    for format in &SUPPORTED_FORMATS {
        if let Some(cfg) = configs
            .iter()
            .find(|c| c.channels() == MONO && c.sample_format() == *format)
        {
            return Ok(AudioConfig {
                channels: MONO,
                sample_rate: cfg.max_sample_rate(),
                sample_format: *format,
            });
        }
    }

    Err("Output device has no supported audio configuration".to_string())
}

// =============================================================================
// Audio Capture
// =============================================================================

/// Audio capture from microphone
///
/// Captures audio at 48kHz mono for voice encoding. Uses f32 samples internally
/// for compatibility with WebRTC audio processing. Automatically resamples if
/// the device doesn't support 48kHz natively.
pub struct AudioCapture {
    /// The cpal input stream
    _stream: Stream,
    /// Buffer for captured audio samples (f32 normalized to -1.0..1.0, always at 48kHz)
    buffer: Arc<Mutex<Vec<f32>>>,
    /// Flag indicating if capture is active
    active: Arc<AtomicBool>,
    /// Receiver for audio stream errors
    error_rx: std_mpsc::Receiver<String>,
}

impl AudioCapture {
    /// Create a new audio capture from the specified device
    ///
    /// # Arguments
    /// * `device_name` - Device name, or empty string for system default
    ///
    /// # Returns
    /// * `Ok(AudioCapture)` - Capture ready to start
    /// * `Err(String)` - Error message if device not found or couldn't be opened
    pub fn new(device_name: &str) -> Result<Self, String> {
        let device =
            find_input_device(device_name).ok_or_else(|| "Input device not found".to_string())?;

        let buffer = Arc::new(Mutex::new(Vec::with_capacity(
            VOICE_SAMPLES_PER_FRAME as usize * 4,
        )));
        let buffer_clone = buffer.clone();
        let active = Arc::new(AtomicBool::new(false));
        let active_clone = active.clone();

        // Create channel for error reporting from audio callback
        let (error_tx, error_rx) = std_mpsc::channel();

        // Find best configuration for this device
        let audio_config = find_best_input_config(&device)?;

        let config = StreamConfig {
            channels: audio_config.channels,
            sample_rate: audio_config.sample_rate,
            buffer_size: cpal::BufferSize::Default,
        };

        // Create resampler if device doesn't support 48kHz
        // Note: InputResampler only handles mono - stereo downmix happens in build_input_stream_stereo
        let resampler = if needs_resampling(audio_config.sample_rate) {
            Some(Arc::new(Mutex::new(
                InputResampler::new(audio_config.sample_rate)
                    .map_err(|e| format!("Failed to create input resampler: {}", e))?,
            )))
        } else {
            None
        };

        // Build stream based on sample format and channel count
        let stream = match (audio_config.sample_format, audio_config.channels) {
            (SampleFormat::F32, MONO) => build_input_stream_mono::<f32>(
                &device,
                &config,
                buffer_clone,
                active_clone,
                error_tx,
                resampler,
            ),
            (SampleFormat::I16, MONO) => build_input_stream_mono::<i16>(
                &device,
                &config,
                buffer_clone,
                active_clone,
                error_tx,
                resampler,
            ),
            (SampleFormat::U16, MONO) => build_input_stream_mono::<u16>(
                &device,
                &config,
                buffer_clone,
                active_clone,
                error_tx,
                resampler,
            ),
            (SampleFormat::F32, STEREO) => build_input_stream_stereo::<f32>(
                &device,
                &config,
                buffer_clone,
                active_clone,
                error_tx,
                resampler,
            ),
            (SampleFormat::I16, STEREO) => build_input_stream_stereo::<i16>(
                &device,
                &config,
                buffer_clone,
                active_clone,
                error_tx,
                resampler,
            ),
            (SampleFormat::U16, STEREO) => build_input_stream_stereo::<u16>(
                &device,
                &config,
                buffer_clone,
                active_clone,
                error_tx,
                resampler,
            ),
            _ => {
                return Err(format!(
                    "Unsupported audio format: {:?} with {} channels",
                    audio_config.sample_format, audio_config.channels
                ));
            }
        }?;

        Ok(Self {
            _stream: stream,
            buffer,
            active,
            error_rx,
        })
    }

    /// Start capturing audio
    pub fn start(&self) -> Result<(), String> {
        self.active.store(true, Ordering::SeqCst);
        self._stream
            .play()
            .map_err(|e| format!("Failed to start capture: {}", e))
    }

    /// Stop capturing audio
    pub fn stop(&self) {
        self.active.store(false, Ordering::SeqCst);
        // Clear the buffer when stopping
        if let Ok(mut buf) = self.buffer.lock() {
            buf.clear();
        }
    }

    /// Check if capture is currently active
    pub fn is_active(&self) -> bool {
        self.active.load(Ordering::SeqCst)
    }

    /// Take a frame of audio samples into a caller-owned buffer, avoiding a
    /// per-frame allocation on the ~100 Hz transmit path (mirrors the
    /// resampler's `process_into` pattern).
    ///
    /// Returns `true` when a full `VOICE_SAMPLES_PER_FRAME` frame was written
    /// into `out` (replacing its contents), `false` when not enough samples
    /// have been captured yet (`out` untouched). Samples are f32 normalized
    /// to [-1.0, 1.0].
    pub fn take_frame_into(&self, out: &mut Vec<f32>) -> bool {
        let Ok(mut buffer) = self.buffer.lock() else {
            return false;
        };
        let frame_size = VOICE_SAMPLES_PER_FRAME as usize;

        if buffer.len() >= frame_size {
            out.clear();
            out.extend(buffer.drain(..frame_size));
            true
        } else {
            false
        }
    }

    /// Check for audio stream errors (non-blocking)
    ///
    /// Returns the first error if one has occurred, or None if no errors.
    /// Only returns the first error since the session will be torn down anyway.
    pub fn check_error(&self) -> Option<String> {
        self.error_rx.try_recv().ok()
    }

    /// Get the current input level (0.0 - 1.0) for UI display
    pub fn get_input_level(&self) -> f32 {
        let buffer = match self.buffer.lock() {
            Ok(b) => b,
            Err(_) => return 0.0,
        };

        if buffer.is_empty() {
            return 0.0;
        }

        // Calculate RMS of recent samples (f32 samples are already normalized)
        let sample_count = buffer.len().min(VOICE_SAMPLES_PER_FRAME as usize);
        let samples = &buffer[buffer.len() - sample_count..];

        let sum_squares: f64 = samples.iter().map(|&s| (s as f64) * (s as f64)).sum();

        let rms = (sum_squares / sample_count as f64).sqrt();

        // Convert to 0-1 range with some headroom
        (rms * RMS_DISPLAY_SCALE).min(1.0) as f32
    }
}

/// Build a mono input stream for the given sample type
fn build_input_stream_mono<T>(
    device: &Device,
    config: &StreamConfig,
    buffer: Arc<Mutex<Vec<f32>>>,
    active: Arc<AtomicBool>,
    error_tx: std_mpsc::Sender<String>,
    resampler: Option<Arc<Mutex<InputResampler>>>,
) -> Result<Stream, String>
where
    T: Sample + cpal::SizedSample,
    f32: FromSample<T>,
{
    let callback_error_tx = error_tx.clone();
    let mut converted_samples =
        Vec::with_capacity(VOICE_SAMPLES_PER_FRAME as usize * MONO as usize);
    let mut resampled_samples = Vec::with_capacity(VOICE_SAMPLES_PER_FRAME as usize);
    device
        .build_input_stream(
            *config,
            move |data: &[T], _: &cpal::InputCallbackInfo| {
                if !active.load(Ordering::SeqCst) {
                    return;
                }

                converted_samples.clear();
                converted_samples.extend(data.iter().map(|s| f32::from_sample(*s)));

                // Resample if needed, otherwise use directly
                let output_samples = if let Some(ref resampler) = resampler {
                    if let Ok(mut r) = resampler.lock() {
                        resampled_samples.clear();
                        if let Err(e) = r.process_into(&converted_samples, &mut resampled_samples) {
                            let _ = callback_error_tx.send(e);
                            return;
                        }
                        &resampled_samples
                    } else {
                        &converted_samples
                    }
                } else {
                    &converted_samples
                };

                append_capture_samples(&buffer, output_samples);
            },
            {
                let error_tx = error_tx.clone();
                move |err| {
                    // Filter out transient buffer underrun/overrun errors (common during high CPU load)
                    if matches!(err.kind(), ErrorKind::Xrun) {
                        return;
                    }
                    let _ = error_tx.send(format!("Audio capture error: {}", err));
                }
            },
            None,
        )
        .map_err(|e| format!("Failed to build input stream: {}", e))
}

/// Build a stereo input stream that downmixes to mono
fn build_input_stream_stereo<T>(
    device: &Device,
    config: &StreamConfig,
    buffer: Arc<Mutex<Vec<f32>>>,
    active: Arc<AtomicBool>,
    error_tx: std_mpsc::Sender<String>,
    resampler: Option<Arc<Mutex<InputResampler>>>,
) -> Result<Stream, String>
where
    T: Sample + cpal::SizedSample,
    f32: FromSample<T>,
{
    let callback_error_tx = error_tx.clone();
    let mut mono_samples = Vec::with_capacity(VOICE_SAMPLES_PER_FRAME as usize);
    let mut resampled_samples = Vec::with_capacity(VOICE_SAMPLES_PER_FRAME as usize);
    device
        .build_input_stream(
            *config,
            move |data: &[T], _: &cpal::InputCallbackInfo| {
                if !active.load(Ordering::SeqCst) {
                    return;
                }

                // Downmix stereo to mono by averaging L+R channels
                mono_samples.clear();
                mono_samples.extend(data.chunks_exact(STEREO as usize).map(|chunk| {
                    let left = f32::from_sample(chunk[0]);
                    let right = f32::from_sample(chunk[1]);
                    (left + right) * 0.5
                }));

                // Resample if needed, otherwise use directly
                let output_samples = if let Some(ref resampler) = resampler {
                    if let Ok(mut r) = resampler.lock() {
                        resampled_samples.clear();
                        if let Err(e) = r.process_into(&mono_samples, &mut resampled_samples) {
                            let _ = callback_error_tx.send(e);
                            return;
                        }
                        &resampled_samples
                    } else {
                        &mono_samples
                    }
                } else {
                    &mono_samples
                };

                append_capture_samples(&buffer, output_samples);
            },
            {
                let error_tx = error_tx.clone();
                move |err| {
                    // Filter out transient buffer underrun/overrun errors (common during high CPU load)
                    if matches!(err.kind(), ErrorKind::Xrun) {
                        return;
                    }
                    let _ = error_tx.send(format!("Audio capture error: {}", err));
                }
            },
            None,
        )
        .map_err(|e| format!("Failed to build stereo input stream: {}", e))
}

fn append_capture_samples(buffer: &Arc<Mutex<Vec<f32>>>, samples: &[f32]) {
    let Ok(mut buf) = buffer.lock() else {
        return;
    };

    buf.extend_from_slice(samples);

    // Limit buffer size to prevent unbounded growth
    let max_size = VOICE_SAMPLES_PER_FRAME as usize * MAX_CAPTURE_BUFFER_FRAMES;
    if buf.len() > max_size {
        let drain_count = buf.len() - max_size;
        buf.drain(..drain_count);
    }
}

// =============================================================================
// Audio Mixer
// =============================================================================

/// Per-user audio buffer for mixing
struct UserAudioBuffer {
    /// Audio samples waiting to be mixed (at 48kHz)
    samples: Vec<f32>,
}

impl UserAudioBuffer {
    fn new() -> Self {
        Self {
            samples: Vec::with_capacity(
                VOICE_SAMPLES_PER_FRAME as usize * MAX_PLAYBACK_BUFFER_FRAMES,
            ),
        }
    }
}

/// Shared state for the audio mixer (accessed from audio callback)
struct MixerState {
    /// Per-user audio buffers (keyed by lowercase nickname)
    user_buffers: HashMap<String, UserAudioBuffer>,
    /// Set of muted nicknames (lowercase)
    muted: std::collections::HashSet<String>,
    /// Whether all incoming audio is muted (deafened)
    deafened: bool,
    /// Pre-allocated buffer for mixing 48kHz audio (avoids allocation per callback)
    /// Always sized to MAX_MIX_BUFFER_SAMPLES, we just use a slice of it
    mix_buffer: Vec<f32>,
    /// Ring buffer for resampled output (avoids O(n) drain operations)
    resampled_ring: RingBuffer,
    /// Cached resampler ratio (48kHz / device_rate), 0.0 if no resampling
    resampler_ratio: f64,
}

/// Maximum size for the mix buffer (enough for typical callback sizes with ratio headroom)
const MAX_MIX_BUFFER_SAMPLES: usize = 2048;

/// Maximum size for resampled output ring buffer (prevents unbounded growth)
const MAX_RESAMPLED_OUTPUT_SAMPLES: usize = 4096;

fn output_resampler_input_samples_needed(resampler_ratio: f64, max_samples: usize) -> usize {
    (((VOICE_SAMPLES_PER_FRAME as f64) * resampler_ratio).ceil() as usize).min(max_samples)
}

/// Simple ring buffer for O(1) push and pop operations
struct RingBuffer {
    /// Pre-allocated storage
    data: Vec<f32>,
    /// Read index (where to consume from)
    read_idx: usize,
    /// Number of samples currently in buffer
    len: usize,
}

impl RingBuffer {
    fn new(capacity: usize) -> Self {
        Self {
            data: vec![0.0; capacity],
            read_idx: 0,
            len: 0,
        }
    }

    /// Number of samples available to read
    fn len(&self) -> usize {
        self.len
    }

    /// Push samples to the buffer, dropping oldest if full
    fn push(&mut self, samples: &[f32]) {
        let capacity = self.data.len();
        for &sample in samples {
            if self.len < capacity {
                // Buffer not full - write at end
                let write_idx = (self.read_idx + self.len) % capacity;
                self.data[write_idx] = sample;
                self.len += 1;
            } else {
                // Buffer full - overwrite oldest (advance read_idx)
                self.data[self.read_idx] = sample;
                self.read_idx = (self.read_idx + 1) % capacity;
            }
        }
    }

    /// Read a single sample from the buffer, returns 0.0 if empty
    fn read_sample(&mut self) -> f32 {
        if self.len == 0 {
            return 0.0;
        }
        let sample = self.data[self.read_idx];
        self.read_idx = (self.read_idx + 1) % self.data.len();
        self.len -= 1;
        sample
    }
}

impl MixerState {
    fn new(resampler_ratio: f64) -> Self {
        Self {
            user_buffers: HashMap::new(),
            muted: std::collections::HashSet::new(),
            deafened: false,
            // Pre-allocate to exact max size - we'll use fill() instead of resize()
            mix_buffer: vec![0.0; MAX_MIX_BUFFER_SAMPLES],
            resampled_ring: RingBuffer::new(MAX_RESAMPLED_OUTPUT_SAMPLES),
            resampler_ratio,
        }
    }

    /// Mix user buffers into mix_buffer and drain consumed samples
    ///
    /// Returns true if any audio was mixed.
    fn mix_and_drain(&mut self, samples_needed: usize) -> bool {
        // Zero the portion of mix_buffer we'll use (no resize, buffer is pre-allocated)
        let samples_needed = samples_needed.min(MAX_MIX_BUFFER_SAMPLES);
        self.mix_buffer[..samples_needed].fill(0.0);

        // Check if any user has audio
        let has_audio = self.user_buffers.values().any(|b| !b.samples.is_empty());

        if has_audio {
            // Mix all user buffers together
            for buffer in self.user_buffers.values() {
                let available = buffer.samples.len().min(samples_needed);
                for i in 0..available {
                    self.mix_buffer[i] += buffer.samples[i];
                }
            }

            // Drain consumed samples from all buffers
            for buffer in self.user_buffers.values_mut() {
                let drain_count = samples_needed.min(buffer.samples.len());
                buffer.samples.drain(..drain_count);
            }
        } else {
            // No audio playing - good time to clean up stale empty buffers
            // This is opportunistic: zero cost during active audio, cleanup during silence
            self.user_buffers.retain(|_, b| !b.samples.is_empty());
        }

        has_audio
    }

    fn remove_user_state(&mut self, sender_key: &str) {
        self.user_buffers.remove(sender_key);
        self.muted.remove(sender_key);
    }

    fn mute_user_and_clear(&mut self, nickname: &str) {
        let key = fold_name(nickname);
        self.muted.insert(key.clone());
        self.user_buffers.remove(&key);
    }

    fn queue_audio(&mut self, sender_key: &str, samples: &[f32]) {
        // Skip if deafened or user is muted
        if self.deafened || self.muted.contains(sender_key) {
            return;
        }

        // Get or create buffer for this user; get_mut-first so the steady
        // state (buffer exists, 100 Hz per talker) allocates nothing.
        let buffer = if let Some(b) = self.user_buffers.get_mut(sender_key) {
            b
        } else {
            self.user_buffers
                .insert(sender_key.to_string(), UserAudioBuffer::new());
            self.user_buffers
                .get_mut(sender_key)
                .expect(ERR_MIXER_BUFFER_MISSING_AFTER_INSERT)
        };

        buffer.samples.extend_from_slice(samples);

        // Limit buffer size to prevent latency buildup
        let max_size = VOICE_SAMPLES_PER_FRAME as usize * MAX_PLAYBACK_BUFFER_FRAMES;
        if buffer.samples.len() > max_size {
            let drain_count = buffer.samples.len() - max_size;
            buffer.samples.drain(..drain_count);
        }
    }

    fn rename_user(&mut self, old: &str, new: &str, muted: bool) {
        let old_key = fold_name(old);
        let new_key = fold_name(new);
        let should_mute_new =
            muted || self.muted.contains(&old_key) || self.muted.contains(&new_key);

        if old_key != new_key
            && let Some(buffer) = self.user_buffers.remove(&old_key)
        {
            self.user_buffers.entry(new_key.clone()).or_insert(buffer);
        }

        self.muted.remove(&old_key);
        if should_mute_new {
            self.muted.insert(new_key);
        } else {
            self.muted.remove(&new_key);
        }
    }
}

/// Mixes multiple audio streams into one output
///
/// Properly sums audio from multiple simultaneous speakers instead of
/// concatenating sequentially. Each user has their own buffer, and the
/// audio callback mixes them together at playback time.
///
/// Automatically resamples from 48kHz to device sample rate if needed.
pub struct AudioMixer {
    /// The cpal output stream
    _stream: Stream,
    /// Shared mixer state (accessed from audio callback and main thread)
    state: Arc<Mutex<MixerState>>,
    /// Flag indicating if playback is active
    active: Arc<AtomicBool>,
    /// Receiver for audio stream errors
    error_rx: std_mpsc::Receiver<String>,
}

impl AudioMixer {
    /// Create a new audio mixer with the specified output device
    pub fn new(device_name: &str) -> Result<Self, String> {
        let device =
            find_output_device(device_name).ok_or_else(|| "Output device not found".to_string())?;

        // Find best configuration for this device
        let audio_config = find_best_output_config(&device)?;

        // Calculate resampler ratio once at setup (0.0 means no resampling)
        let resampler_ratio = if needs_resampling(audio_config.sample_rate) {
            VOICE_SAMPLE_RATE as f64 / audio_config.sample_rate as f64
        } else {
            0.0
        };

        let state = Arc::new(Mutex::new(MixerState::new(resampler_ratio)));
        let state_clone = state.clone();
        let active = Arc::new(AtomicBool::new(false));
        let active_clone = active.clone();

        // Create channel for error reporting from audio callback
        let (error_tx, error_rx) = std_mpsc::channel();

        let config = StreamConfig {
            channels: audio_config.channels,
            sample_rate: audio_config.sample_rate,
            buffer_size: cpal::BufferSize::Default,
        };

        // Create resampler if device doesn't support 48kHz
        let resampler = if needs_resampling(audio_config.sample_rate) {
            Some(Arc::new(Mutex::new(
                OutputResampler::new(audio_config.sample_rate, audio_config.channels as usize)
                    .map_err(|e| format!("Failed to create output resampler: {}", e))?,
            )))
        } else {
            None
        };

        // Build the appropriate stream based on channel count and sample format
        let stream = match (audio_config.channels, audio_config.sample_format) {
            (MONO, SampleFormat::F32) => build_mixer_stream_mono::<f32>(
                &device,
                &config,
                state_clone,
                active_clone,
                error_tx,
                resampler,
            ),
            (MONO, SampleFormat::I16) => build_mixer_stream_mono::<i16>(
                &device,
                &config,
                state_clone,
                active_clone,
                error_tx,
                resampler,
            ),
            (MONO, SampleFormat::U16) => build_mixer_stream_mono::<u16>(
                &device,
                &config,
                state_clone,
                active_clone,
                error_tx,
                resampler,
            ),
            (STEREO, SampleFormat::F32) => build_mixer_stream_stereo::<f32>(
                &device,
                &config,
                state_clone,
                active_clone,
                error_tx,
                resampler,
            ),
            (STEREO, SampleFormat::I16) => build_mixer_stream_stereo::<i16>(
                &device,
                &config,
                state_clone,
                active_clone,
                error_tx,
                resampler,
            ),
            (STEREO, SampleFormat::U16) => build_mixer_stream_stereo::<u16>(
                &device,
                &config,
                state_clone,
                active_clone,
                error_tx,
                resampler,
            ),
            _ => Err(format!(
                "Unsupported audio format: {:?} with {} channels",
                audio_config.sample_format, audio_config.channels
            )),
        }?;

        Ok(Self {
            _stream: stream,
            state,
            active,
            error_rx,
        })
    }

    /// Start the mixer
    pub fn start(&self) -> Result<(), String> {
        self.active.store(true, Ordering::SeqCst);
        self._stream
            .play()
            .map_err(|e| format!("Failed to start mixer: {}", e))
    }

    /// Stop the mixer
    pub fn stop(&self) {
        self.active.store(false, Ordering::SeqCst);
        let _ = self._stream.pause();
    }

    /// Mute a user by nickname
    pub fn mute_user(&mut self, nickname: &str) {
        if let Ok(mut state) = self.state.lock() {
            state.muted.insert(fold_name(nickname));
        }
    }

    /// Unmute a user by nickname
    pub fn unmute_user(&mut self, nickname: &str) {
        if let Ok(mut state) = self.state.lock() {
            state.muted.remove(&fold_name(nickname));
        }
    }

    /// Clear queued audio and mute state for a user by folded sender key.
    pub fn remove_user_state(&mut self, sender_key: &str) {
        if let Ok(mut state) = self.state.lock() {
            state.remove_user_state(sender_key);
        }
    }

    /// Mute a user and clear queued audio under one mixer lock.
    pub fn mute_user_and_clear(&mut self, nickname: &str) {
        if let Ok(mut state) = self.state.lock() {
            state.mute_user_and_clear(nickname);
        }
    }

    /// Re-key queued audio and mute state after a nickname rename.
    pub fn rename_user(&mut self, old: &str, new: &str, muted: bool) {
        if let Ok(mut state) = self.state.lock() {
            state.rename_user(old, new, muted);
        }
    }

    /// Set deafened state (mute all incoming audio)
    pub fn set_deafened(&mut self, deafened: bool) {
        if let Ok(mut state) = self.state.lock() {
            state.deafened = deafened;
        }
    }

    /// Check for audio stream errors (non-blocking)
    pub fn check_error(&self) -> Option<String> {
        self.error_rx.try_recv().ok()
    }

    /// Queue audio from a user (by folded sender key) for playback
    ///
    /// Audio is buffered per-user and mixed together at playback time.
    /// Samples should be f32 normalized to [-1.0, 1.0] at 48kHz.
    pub fn queue_audio(&self, sender_key: &str, samples: &[f32]) {
        if let Ok(mut state) = self.state.lock() {
            state.queue_audio(sender_key, samples);
        }
    }
}

/// Build a mono mixer output stream
fn build_mixer_stream_mono<T>(
    device: &Device,
    config: &StreamConfig,
    state: Arc<Mutex<MixerState>>,
    active: Arc<AtomicBool>,
    error_tx: std_mpsc::Sender<String>,
    resampler: Option<Arc<Mutex<OutputResampler>>>,
) -> Result<Stream, String>
where
    T: Sample + cpal::SizedSample + FromSample<f32>,
{
    let callback_error_tx = error_tx.clone();
    device
        .build_output_stream(
            *config,
            move |data: &mut [T], _: &cpal::OutputCallbackInfo| {
                if !active.load(Ordering::SeqCst) {
                    // Not active - output silence
                    for sample in data.iter_mut() {
                        *sample = T::from_sample(0.0f32);
                    }
                    return;
                }

                let Ok(mut state) = state.lock() else {
                    // Couldn't lock state - output silence
                    for sample in data.iter_mut() {
                        *sample = T::from_sample(0.0f32);
                    }
                    return;
                };

                let output_samples_needed = data.len();

                // If resampling, use persistent buffer approach
                if let Some(ref resampler) = resampler {
                    // Lock resampler once for the entire callback (avoid repeated locking)
                    let Ok(mut resampler_guard) = resampler.lock() else {
                        // Mutex poisoned - output silence
                        for sample in data.iter_mut() {
                            *sample = T::from_sample(0.0f32);
                        }
                        return;
                    };

                    // Use cached ratio from state (computed once at setup)
                    let input_samples_needed = output_resampler_input_samples_needed(
                        state.resampler_ratio,
                        state.mix_buffer.len(),
                    );

                    // Feed more audio to resampler while ring buffer is low
                    while state.resampled_ring.len() < output_samples_needed {
                        // Mix user buffers into mix_buffer
                        let has_audio = state.mix_and_drain(input_samples_needed);

                        if !has_audio {
                            // No audio available - stop trying to fill buffer
                            break;
                        }

                        // Apply soft clipping
                        for i in 0..input_samples_needed.min(state.mix_buffer.len()) {
                            state.mix_buffer[i] = soft_clip(state.mix_buffer[i]);
                        }

                        // Resample and accumulate output in ring buffer
                        match resampler_guard.process(&state.mix_buffer[..input_samples_needed]) {
                            Ok(resampled) => {
                                // Ring buffer handles overflow by dropping oldest samples
                                state.resampled_ring.push(&resampled);
                            }
                            Err(e) => {
                                let _ = callback_error_tx.send(e);
                                break;
                            }
                        }
                    }

                    // Write from ring buffer directly to audio output (O(1) per sample).
                    // Resampling can introduce small overshoots, so keep the final device
                    // samples inside the normalized PCM range.
                    for dst in data.iter_mut() {
                        *dst = T::from_sample(sanitize_sample(state.resampled_ring.read_sample()));
                    }
                } else {
                    // No resampling needed - direct path (no persistent buffer needed)
                    // Mix user buffers into mix_buffer
                    state.mix_and_drain(output_samples_needed);

                    // Apply soft clipping and write directly to output
                    for (i, dst) in data.iter_mut().enumerate() {
                        let sample = soft_clip(state.mix_buffer.get(i).copied().unwrap_or(0.0));
                        *dst = T::from_sample(sample);
                    }
                }
            },
            {
                let error_tx = error_tx.clone();
                move |err| {
                    // Filter out transient buffer underrun/overrun errors (common during high CPU load)
                    if matches!(err.kind(), ErrorKind::Xrun) {
                        return;
                    }
                    let _ = error_tx.send(format!("Mixer error: {}", err));
                }
            },
            None,
        )
        .map_err(|e| format!("Failed to build mixer stream: {}", e))
}

/// Build a stereo mixer output stream (upmixes from mono)
fn build_mixer_stream_stereo<T>(
    device: &Device,
    config: &StreamConfig,
    state: Arc<Mutex<MixerState>>,
    active: Arc<AtomicBool>,
    error_tx: std_mpsc::Sender<String>,
    resampler: Option<Arc<Mutex<OutputResampler>>>,
) -> Result<Stream, String>
where
    T: Sample + cpal::SizedSample + FromSample<f32>,
{
    let callback_error_tx = error_tx.clone();
    device
        .build_output_stream(
            *config,
            move |data: &mut [T], _: &cpal::OutputCallbackInfo| {
                if !active.load(Ordering::SeqCst) {
                    // Not active - output silence
                    for sample in data.iter_mut() {
                        *sample = T::from_sample(0.0f32);
                    }
                    return;
                }

                let Ok(mut state) = state.lock() else {
                    // Couldn't lock state - output silence
                    for sample in data.iter_mut() {
                        *sample = T::from_sample(0.0f32);
                    }
                    return;
                };

                // Stereo: data.len() is total samples, we need half that in mono frames
                let stereo_frames_needed = data.len() / STEREO as usize;
                let output_samples_needed = data.len(); // Total stereo samples

                // If resampling, use persistent buffer approach
                if let Some(ref resampler) = resampler {
                    // Lock resampler once for the entire callback (avoid repeated locking)
                    let Ok(mut resampler_guard) = resampler.lock() else {
                        // Mutex poisoned - output silence
                        for sample in data.iter_mut() {
                            *sample = T::from_sample(0.0f32);
                        }
                        return;
                    };

                    // Use cached ratio from state (computed once at setup)
                    let input_samples_needed = output_resampler_input_samples_needed(
                        state.resampler_ratio,
                        state.mix_buffer.len(),
                    );

                    // Feed more audio to resampler while ring buffer is low
                    while state.resampled_ring.len() < output_samples_needed {
                        // Mix user buffers into mix_buffer
                        let has_audio = state.mix_and_drain(input_samples_needed);

                        if !has_audio {
                            // No audio available - stop trying to fill buffer
                            break;
                        }

                        // Apply soft clipping
                        for i in 0..input_samples_needed.min(state.mix_buffer.len()) {
                            state.mix_buffer[i] = soft_clip(state.mix_buffer[i]);
                        }

                        // Resample and accumulate output in ring buffer (resampler handles stereo upmix)
                        match resampler_guard.process(&state.mix_buffer[..input_samples_needed]) {
                            Ok(resampled) => {
                                // Ring buffer handles overflow by dropping oldest samples
                                state.resampled_ring.push(&resampled);
                            }
                            Err(e) => {
                                let _ = callback_error_tx.send(e);
                                break;
                            }
                        }
                    }

                    // Write from ring buffer directly to audio output (O(1) per sample).
                    // Resampling can introduce small overshoots, so keep the final device
                    // samples inside the normalized PCM range.
                    for dst in data.iter_mut() {
                        *dst = T::from_sample(sanitize_sample(state.resampled_ring.read_sample()));
                    }
                } else {
                    // No resampling needed - direct path with stereo upmix
                    // Mix user buffers into mix_buffer
                    state.mix_and_drain(stereo_frames_needed);

                    // Apply soft clipping and upmix mono to stereo directly
                    for (i, chunk) in data.chunks_exact_mut(STEREO as usize).enumerate() {
                        let sample = soft_clip(state.mix_buffer.get(i).copied().unwrap_or(0.0));
                        let out = T::from_sample(sample);
                        chunk[0] = out;
                        chunk[1] = out;
                    }
                }
            },
            {
                let error_tx = error_tx.clone();
                move |err| {
                    // Filter out transient buffer underrun/overrun errors (common during high CPU load)
                    if matches!(err.kind(), ErrorKind::Xrun) {
                        return;
                    }
                    let _ = error_tx.send(format!("Mixer error: {}", err));
                }
            },
            None,
        )
        .map_err(|e| format!("Failed to build stereo mixer stream: {}", e))
}

/// Soft clip function to prevent harsh digital clipping
///
/// Uses tanh-based soft clipping which smoothly limits the signal
/// as it approaches the maximum, resulting in less harsh distortion
/// when multiple loud sources are summed together.
pub(crate) fn soft_clip(sample: f32) -> f32 {
    if !sample.is_finite() {
        return 0.0;
    }

    // tanh gives smooth saturation, but we scale input to make it more gradual
    sanitize_sample((sample * SOFT_CLIP_GAIN).tanh() / SOFT_CLIP_GAIN.tanh())
}

/// Keep a sample inside the normalized full-scale PCM range.
pub(crate) fn sanitize_sample(sample: f32) -> f32 {
    if sample.is_finite() {
        sample.clamp(-1.0, 1.0)
    } else {
        0.0
    }
}

/// Sanitize an audio frame in place without changing valid in-range samples.
pub(crate) fn sanitize_audio_frame(frame: &mut [f32]) {
    for sample in frame {
        *sample = sanitize_sample(*sample);
    }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_audio_device_system_default() {
        let device = AudioDevice::system_default();
        assert_eq!(device.name, SYSTEM_DEFAULT_DEVICE_NAME);
        assert!(device.is_default);
    }

    #[test]
    fn test_audio_device_new() {
        let device = AudioDevice::new("Test Device".to_string(), false);
        assert_eq!(device.name, "Test Device");
        assert!(!device.is_default);
    }

    #[test]
    fn test_audio_device_equality() {
        let device1 = AudioDevice::new("Test".to_string(), false);
        let device2 = AudioDevice::new("Test".to_string(), true);
        let device3 = AudioDevice::new("Other".to_string(), false);

        assert_eq!(device1, device2); // Same name, different is_default
        assert_ne!(device1, device3); // Different name
    }

    #[test]
    fn test_audio_device_list_from_names_includes_default() {
        let devices = audio_device_list_from_names(Vec::new());
        assert!(!devices.is_empty());
        assert!(devices[0].is_default);
        assert_eq!(devices[0].name, SYSTEM_DEFAULT_DEVICE_NAME);
    }

    #[test]
    fn test_audio_device_list_from_names_deduplicates_devices() {
        let devices = audio_device_list_from_names([
            "Mic A".to_string(),
            "Mic B".to_string(),
            "Mic A".to_string(),
        ]);

        assert_eq!(devices.len(), 3);
        assert_eq!(devices[0], AudioDevice::system_default());
        assert_eq!(devices[1], AudioDevice::new("Mic A".to_string(), false));
        assert_eq!(devices[2], AudioDevice::new("Mic B".to_string(), false));
    }

    #[test]
    fn test_soft_clip_passthrough() {
        // Small values should pass through almost unchanged
        let input = 0.5;
        let output = soft_clip(input);
        assert!((output - input).abs() < 0.1);
    }

    #[test]
    fn test_soft_clip_limits() {
        // Large values should be limited
        let output = soft_clip(2.0);
        assert!(output <= 1.0);
        assert!(output > 0.0);

        let output_neg = soft_clip(-2.0);
        assert!(output_neg >= -1.0);
        assert!(output_neg < 0.0);
    }

    #[test]
    fn test_soft_clip_sanitizes_non_finite_samples() {
        assert_eq!(soft_clip(f32::NAN), 0.0);
        assert_eq!(soft_clip(f32::INFINITY), 0.0);
        assert_eq!(soft_clip(f32::NEG_INFINITY), 0.0);
    }

    #[test]
    fn test_sanitize_audio_frame_preserves_valid_samples_and_bounds_invalid() {
        let mut frame = vec![-2.0, -0.5, 0.0, 0.5, 2.0, f32::NAN, f32::INFINITY];

        sanitize_audio_frame(&mut frame);

        assert_eq!(frame, vec![-1.0, -0.5, 0.0, 0.5, 1.0, 0.0, 0.0]);
    }

    #[test]
    fn test_output_resampler_input_samples_needed_clamps_low_rate() {
        let low_rate_ratio = VOICE_SAMPLE_RATE as f64 / 8_000.0;
        let unclamped = ((VOICE_SAMPLES_PER_FRAME as f64) * low_rate_ratio).ceil() as usize;

        assert!(unclamped > MAX_MIX_BUFFER_SAMPLES);
        assert_eq!(
            output_resampler_input_samples_needed(low_rate_ratio, MAX_MIX_BUFFER_SAMPLES),
            MAX_MIX_BUFFER_SAMPLES
        );
    }

    fn process_low_rate_output_resampler(channels: usize) -> Vec<f32> {
        let low_rate_ratio = VOICE_SAMPLE_RATE as f64 / 8_000.0;
        let mut state = MixerState::new(low_rate_ratio);
        let mut buffer = UserAudioBuffer::new();
        buffer
            .samples
            .extend((0..MAX_MIX_BUFFER_SAMPLES * 2).map(|i| {
                let phase = i as f32 * 0.01;
                phase.sin() * 0.25
            }));
        state.user_buffers.insert(fold_name("Alice"), buffer);

        let input_samples_needed =
            output_resampler_input_samples_needed(state.resampler_ratio, state.mix_buffer.len());
        assert_eq!(input_samples_needed, MAX_MIX_BUFFER_SAMPLES);

        assert!(state.mix_and_drain(input_samples_needed));
        for sample in &mut state.mix_buffer[..input_samples_needed] {
            *sample = soft_clip(*sample);
        }

        let mut resampler = OutputResampler::new(8_000, channels).expect("create resampler");
        resampler
            .process(&state.mix_buffer[..input_samples_needed])
            .expect("process low-rate resampler")
    }

    #[test]
    fn test_low_rate_mono_output_resampler_uses_clamped_mix_slice() {
        let output = process_low_rate_output_resampler(MONO as usize);

        assert!(!output.is_empty());
    }

    #[test]
    fn test_low_rate_stereo_output_resampler_uses_clamped_mix_slice() {
        let output = process_low_rate_output_resampler(STEREO as usize);

        assert!(!output.is_empty());
        assert_eq!(output.len() % STEREO as usize, 0);
    }

    #[test]
    fn test_mixer_state_rename_user_rekeys_buffer_and_mute() {
        let mut state = MixerState::new(0.0);
        let mut buffer = UserAudioBuffer::new();
        buffer.samples.extend_from_slice(&[0.25, 0.5]);
        state.user_buffers.insert(fold_name("Alice"), buffer);
        state.muted.insert(fold_name("Alice"));

        state.rename_user("Alice", "Alicia", true);

        assert!(!state.user_buffers.contains_key(&fold_name("Alice")));
        assert!(state.user_buffers.contains_key(&fold_name("Alicia")));
        assert!(!state.muted.contains(&fold_name("Alice")));
        assert!(state.muted.contains(&fold_name("Alicia")));
    }

    #[test]
    fn test_mixer_state_rename_user_keeps_existing_new_buffer_on_collision() {
        let mut state = MixerState::new(0.0);
        let mut old_buffer = UserAudioBuffer::new();
        old_buffer.samples.push(0.25);
        let mut new_buffer = UserAudioBuffer::new();
        new_buffer.samples.push(0.75);
        state.user_buffers.insert(fold_name("Alice"), old_buffer);
        state.user_buffers.insert(fold_name("Alicia"), new_buffer);

        state.rename_user("Alice", "Alicia", false);

        assert_eq!(state.user_buffers.len(), 1);
        assert_eq!(state.user_buffers[&fold_name("Alicia")].samples, vec![0.75]);
    }

    #[test]
    fn test_mixer_state_rename_user_preserves_existing_new_mute() {
        let mut state = MixerState::new(0.0);
        state.muted.insert(fold_name("Alicia"));

        state.rename_user("Alice", "Alicia", false);

        assert!(state.muted.contains(&fold_name("Alicia")));
        assert!(!state.muted.contains(&fold_name("Alice")));
    }

    #[test]
    fn test_mixer_state_remove_user_state_clears_audio_and_mute() {
        let mut state = MixerState::new(0.0);
        state
            .user_buffers
            .insert(fold_name("Alice"), UserAudioBuffer::new());
        state.muted.insert(fold_name("Alice"));

        state.remove_user_state(&fold_name("Alice"));

        assert!(!state.user_buffers.contains_key(&fold_name("Alice")));
        assert!(!state.muted.contains(&fold_name("Alice")));
    }

    #[test]
    fn test_mixer_state_mute_user_and_clear_mutes_and_clears_audio() {
        let mut state = MixerState::new(0.0);
        state
            .user_buffers
            .insert(fold_name("Alice"), UserAudioBuffer::new());

        state.mute_user_and_clear("Alice");

        assert!(!state.user_buffers.contains_key(&fold_name("Alice")));
        assert!(state.muted.contains(&fold_name("Alice")));
    }

    #[test]
    fn test_mixer_state_muted_user_cannot_buffer_new_audio() {
        let mut state = MixerState::new(0.0);
        state.mute_user_and_clear("Alice");

        state.queue_audio(&fold_name("Alice"), &[0.25, 0.5]);
        state.queue_audio(&fold_name("Bob"), &[0.75]);

        assert!(!state.user_buffers.contains_key(&fold_name("Alice")));
        assert_eq!(state.user_buffers[&fold_name("Bob")].samples, vec![0.75]);
    }

    // =========================================================================
    // RingBuffer Tests
    // =========================================================================

    #[test]
    fn test_ring_buffer_new() {
        let rb = RingBuffer::new(10);
        assert_eq!(rb.len(), 0);
        assert_eq!(rb.data.len(), 10);
    }

    #[test]
    fn test_ring_buffer_push_and_read() {
        let mut rb = RingBuffer::new(10);

        // Push some samples
        rb.push(&[1.0, 2.0, 3.0]);
        assert_eq!(rb.len(), 3);

        // Read them back
        assert_eq!(rb.read_sample(), 1.0);
        assert_eq!(rb.read_sample(), 2.0);
        assert_eq!(rb.read_sample(), 3.0);
        assert_eq!(rb.len(), 0);
    }

    #[test]
    fn test_ring_buffer_read_empty_returns_silence() {
        let mut rb = RingBuffer::new(10);

        // Reading from empty buffer should return 0.0 (silence)
        assert_eq!(rb.read_sample(), 0.0);
        assert_eq!(rb.read_sample(), 0.0);
        assert_eq!(rb.len(), 0);
    }

    #[test]
    fn test_ring_buffer_wrap_around() {
        let mut rb = RingBuffer::new(4);

        // Fill buffer
        rb.push(&[1.0, 2.0, 3.0, 4.0]);
        assert_eq!(rb.len(), 4);

        // Read two samples (advances read_idx to 2)
        assert_eq!(rb.read_sample(), 1.0);
        assert_eq!(rb.read_sample(), 2.0);
        assert_eq!(rb.len(), 2);

        // Push two more (should wrap around)
        rb.push(&[5.0, 6.0]);
        assert_eq!(rb.len(), 4);

        // Read all - should get 3, 4, 5, 6 in order
        assert_eq!(rb.read_sample(), 3.0);
        assert_eq!(rb.read_sample(), 4.0);
        assert_eq!(rb.read_sample(), 5.0);
        assert_eq!(rb.read_sample(), 6.0);
        assert_eq!(rb.len(), 0);
    }

    #[test]
    fn test_ring_buffer_overflow_drops_oldest() {
        let mut rb = RingBuffer::new(4);

        // Fill buffer
        rb.push(&[1.0, 2.0, 3.0, 4.0]);
        assert_eq!(rb.len(), 4);

        // Push more - should drop oldest
        rb.push(&[5.0, 6.0]);
        assert_eq!(rb.len(), 4); // Still at capacity

        // Should read 3, 4, 5, 6 (1 and 2 were dropped)
        assert_eq!(rb.read_sample(), 3.0);
        assert_eq!(rb.read_sample(), 4.0);
        assert_eq!(rb.read_sample(), 5.0);
        assert_eq!(rb.read_sample(), 6.0);
    }

    #[test]
    fn test_ring_buffer_large_overflow() {
        let mut rb = RingBuffer::new(4);

        // Push way more than capacity
        rb.push(&[1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0]);
        assert_eq!(rb.len(), 4);

        // Should only have the last 4 samples
        assert_eq!(rb.read_sample(), 5.0);
        assert_eq!(rb.read_sample(), 6.0);
        assert_eq!(rb.read_sample(), 7.0);
        assert_eq!(rb.read_sample(), 8.0);
    }
}
