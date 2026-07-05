//! Sound playback for event notifications
//!
//! Uses cpal directly for audio output, with lewton for OGG/Vorbis decoding.
//! Provides a simple queue-based system for playing notification sounds.

use std::io::Cursor;
use std::sync::Mutex;
use std::sync::mpsc::{self, Sender};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, Sample, SampleFormat, Stream, StreamConfig};
use lewton::inside_ogg::OggStreamReader;
use once_cell::sync::Lazy;

use crate::config::events::SoundChoice;
use crate::constants::{
    ERR_SOUND_BUILD_OUTPUT_STREAM, ERR_SOUND_DECODE_OGG, ERR_SOUND_DECODE_PACKET,
    ERR_SOUND_ENUMERATE_DEVICES, ERR_SOUND_INVALID_CHANNEL_COUNT, ERR_SOUND_INVALID_SAMPLE_RATE,
    ERR_SOUND_NO_DEFAULT_OUTPUT_DEVICE, ERR_SOUND_OUTPUT_CONFIG, ERR_SOUND_START_PLAYBACK,
    ERR_SOUND_STATE_LOCK_POISONED, ERR_SOUND_UNSUPPORTED_SAMPLE_FORMAT,
};

// =============================================================================
// Embedded Sounds
// =============================================================================

/// Alert sound - synth notification (CC0: Freesound #651629 "Notify" by Martcraft)
const SOUND_ALERT: &[u8] = include_bytes!("../sounds/alert.ogg");
/// Bell sound - UI approval chime (CC0: Freesound #625174 "UI Sound Approval" by GabFitzgerald)
const SOUND_BELL: &[u8] = include_bytes!("../sounds/bell.ogg");
/// Chime sound - xylophone tone (CC0: Freesound #536748 "Phone" by egomassive)
const SOUND_CHIME: &[u8] = include_bytes!("../sounds/chime.ogg");
/// Ding sound - hand bell (CC0: Freesound #804740 "Bell Hand Ding" by DesignersChoice)
const SOUND_DING: &[u8] = include_bytes!("../sounds/ding.ogg");
/// Pop sound - short blip (CC0: Freesound #757175 "Blip 1" by Henri Kähkönen)
const SOUND_POP: &[u8] = include_bytes!("../sounds/pop.ogg");

// =============================================================================
// Audio Thread
// =============================================================================

/// Request to play a sound
struct SoundRequest {
    /// Sound data to play
    data: &'static [u8],
    /// Volume level (0.0 - 1.0)
    volume: f32,
    /// Output device name (empty string = system default)
    device_name: String,
}

/// State for the sound system
struct SoundState {
    /// Sender to the audio thread
    sender: Sender<SoundRequest>,
}

/// Global sound state
static SOUND_STATE: Lazy<Mutex<Option<SoundState>>> = Lazy::new(|| Mutex::new(None));

/// Initialize the audio thread if not already running
fn ensure_audio_thread() -> bool {
    let mut state = SOUND_STATE.lock().expect(ERR_SOUND_STATE_LOCK_POISONED);

    if state.is_some() {
        return true;
    }

    let (tx, rx) = mpsc::channel::<SoundRequest>();

    // Spawn the persistent audio thread
    std::thread::spawn(move || {
        run_audio_thread(rx);
    });

    *state = Some(SoundState { sender: tx });
    true
}

/// Run the audio thread - handles sound playback requests
fn run_audio_thread(rx: mpsc::Receiver<SoundRequest>) {
    for request in rx {
        // Sound playback errors are silently dropped — the user
        // already perceives "no sound" as the failure mode, and
        // this is a GUI app where stderr is invisible.
        let _ = play_sound_blocking(&request);
    }
}

/// Play a sound synchronously (blocks until playback completes)
fn play_sound_blocking(request: &SoundRequest) -> Result<(), String> {
    // Decode the OGG/Vorbis data
    let cursor = Cursor::new(request.data);
    let mut reader =
        OggStreamReader::new(cursor).map_err(|e| format!("{ERR_SOUND_DECODE_OGG}: {e}"))?;

    let source_sample_rate = reader.ident_hdr.audio_sample_rate;
    let source_channels = reader.ident_hdr.audio_channels as u16;

    // Collect all samples (sounds are short, so this is fine)
    let mut samples: Vec<f32> = Vec::new();
    while let Some(packet) = reader
        .read_dec_packet_itl()
        .map_err(|e| format!("{ERR_SOUND_DECODE_PACKET}: {e}"))?
    {
        // Convert i16 samples to f32 and apply volume
        for sample in packet {
            samples.push((sample as f32 / 32768.0) * request.volume);
        }
    }

    if samples.is_empty() {
        return Ok(());
    }

    // Get the output device
    let device = get_output_device(&request.device_name)?;

    let output_config = device
        .default_output_config()
        .map_err(|e| format!("{ERR_SOUND_OUTPUT_CONFIG}: {e}"))?;
    let sample_format = output_config.sample_format();
    let config = output_config.config();
    let samples = convert_samples_for_output(
        &samples,
        source_sample_rate,
        source_channels,
        config.sample_rate,
        config.channels,
    )?;

    // Create a channel to signal when playback is done
    let (done_tx, done_rx) = mpsc::channel::<()>();

    let stream = build_output_stream(&device, &config, sample_format, samples, done_tx)?;

    // Start playback
    stream
        .play()
        .map_err(|e| format!("{ERR_SOUND_START_PLAYBACK}: {e}"))?;

    // Wait for playback to complete (with timeout)
    let _ = done_rx.recv_timeout(std::time::Duration::from_secs(10));

    // Stream is dropped here, stopping playback
    Ok(())
}

fn build_output_stream(
    device: &cpal::Device,
    config: &StreamConfig,
    sample_format: SampleFormat,
    samples: Vec<f32>,
    done_tx: mpsc::Sender<()>,
) -> Result<Stream, String> {
    match sample_format {
        SampleFormat::F32 => build_output_stream_typed::<f32>(device, config, samples, done_tx),
        SampleFormat::F64 => build_output_stream_typed::<f64>(device, config, samples, done_tx),
        SampleFormat::I8 => build_output_stream_typed::<i8>(device, config, samples, done_tx),
        SampleFormat::I16 => build_output_stream_typed::<i16>(device, config, samples, done_tx),
        SampleFormat::I32 => build_output_stream_typed::<i32>(device, config, samples, done_tx),
        SampleFormat::I64 => build_output_stream_typed::<i64>(device, config, samples, done_tx),
        SampleFormat::U8 => build_output_stream_typed::<u8>(device, config, samples, done_tx),
        SampleFormat::U16 => build_output_stream_typed::<u16>(device, config, samples, done_tx),
        SampleFormat::U32 => build_output_stream_typed::<u32>(device, config, samples, done_tx),
        SampleFormat::U64 => build_output_stream_typed::<u64>(device, config, samples, done_tx),
        _ => Err(format!(
            "{ERR_SOUND_UNSUPPORTED_SAMPLE_FORMAT}: {sample_format:?}"
        )),
    }
}

fn build_output_stream_typed<T>(
    device: &cpal::Device,
    config: &StreamConfig,
    samples: Vec<f32>,
    done_tx: mpsc::Sender<()>,
) -> Result<Stream, String>
where
    T: Sample + cpal::SizedSample + FromSample<f32>,
{
    let samples_len = samples.len();
    let position = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let position_clone = position.clone();

    device
        .build_output_stream(
            *config,
            move |output: &mut [T], _: &cpal::OutputCallbackInfo| {
                let pos = position_clone.load(std::sync::atomic::Ordering::Relaxed);
                for (i, sample) in output.iter_mut().enumerate() {
                    let idx = pos + i;
                    let value = samples.get(idx).copied().unwrap_or(0.0).clamp(-1.0, 1.0);
                    *sample = T::from_sample(value);
                }
                let new_pos = pos + output.len();
                position_clone.store(new_pos, std::sync::atomic::Ordering::Relaxed);
                if new_pos >= samples_len {
                    let _ = done_tx.send(());
                }
            },
            // Audio stream errors during playback (buffer underrun
            // etc.) are silently dropped — transient and user-audible
            // anyway as a glitch. Fatal errors propagate via the
            // `done_tx` close path.
            |_err| (),
            None,
        )
        .map_err(|e| format!("{ERR_SOUND_BUILD_OUTPUT_STREAM}: {e}"))
}

fn convert_samples_for_output(
    samples: &[f32],
    source_sample_rate: u32,
    source_channels: u16,
    output_sample_rate: u32,
    output_channels: u16,
) -> Result<Vec<f32>, String> {
    if source_sample_rate == 0 || output_sample_rate == 0 {
        return Err(ERR_SOUND_INVALID_SAMPLE_RATE.to_string());
    }
    if source_channels == 0 || output_channels == 0 {
        return Err(ERR_SOUND_INVALID_CHANNEL_COUNT.to_string());
    }

    let source_channels = source_channels as usize;
    let output_channels = output_channels as usize;
    let source_frames = samples.len() / source_channels;

    if source_frames == 0 {
        return Ok(Vec::new());
    }

    let output_frames = ((source_frames as u64 * output_sample_rate as u64)
        .div_ceil(source_sample_rate as u64)) as usize;
    let mut output = Vec::with_capacity(output_frames * output_channels);

    for output_frame in 0..output_frames {
        let source_pos =
            output_frame as f64 * source_sample_rate as f64 / output_sample_rate as f64;
        let frame_a = source_pos.floor() as usize;
        let frame_b = (frame_a + 1).min(source_frames - 1);
        let mix = (source_pos - frame_a as f64) as f32;

        for output_channel in 0..output_channels {
            let a = frame_sample_for_output_channel(
                samples,
                source_channels,
                output_channels,
                frame_a,
                output_channel,
            );
            let b = frame_sample_for_output_channel(
                samples,
                source_channels,
                output_channels,
                frame_b,
                output_channel,
            );
            output.push(a + (b - a) * mix);
        }
    }

    Ok(output)
}

fn frame_sample_for_output_channel(
    samples: &[f32],
    source_channels: usize,
    output_channels: usize,
    frame: usize,
    output_channel: usize,
) -> f32 {
    let base = frame * source_channels;

    if source_channels == output_channels {
        samples[base + output_channel]
    } else if source_channels == 1 {
        samples[base]
    } else if output_channels == 1 {
        let sum: f32 = samples[base..base + source_channels].iter().sum();
        sum / source_channels as f32
    } else if output_channel < source_channels {
        samples[base + output_channel]
    } else {
        let sum: f32 = samples[base..base + source_channels].iter().sum();
        sum / source_channels as f32
    }
}

/// Get an output device by name, or the default device if name is empty
fn get_output_device(device_name: &str) -> Result<cpal::Device, String> {
    let host = cpal::default_host();

    if device_name.is_empty() {
        host.default_output_device()
            .ok_or_else(|| ERR_SOUND_NO_DEFAULT_OUTPUT_DEVICE.to_string())
    } else {
        let devices = host
            .output_devices()
            .map_err(|e| format!("{ERR_SOUND_ENUMERATE_DEVICES}: {e}"))?;

        select_named_or_default_device(
            device_name,
            devices,
            host.default_output_device(),
            |device| {
                device
                    .description()
                    .ok()
                    .map(|desc| desc.name().to_string())
            },
        )
        .ok_or_else(|| ERR_SOUND_NO_DEFAULT_OUTPUT_DEVICE.to_string())
    }
}

fn select_named_or_default_device<T, I, F>(
    device_name: &str,
    devices: I,
    default_device: Option<T>,
    mut describe: F,
) -> Option<T>
where
    I: IntoIterator<Item = T>,
    F: FnMut(&T) -> Option<String>,
{
    for device in devices {
        if describe(&device).as_deref() == Some(device_name) {
            return Some(device);
        }
    }

    default_device
}

/// Get the sender for sound requests
fn get_sound_sender() -> Option<Sender<SoundRequest>> {
    if !ensure_audio_thread() {
        return None;
    }

    let state = SOUND_STATE.lock().expect(ERR_SOUND_STATE_LOCK_POISONED);
    state.as_ref().map(|s| s.sender.clone())
}

// =============================================================================
// Public API
// =============================================================================

/// Play a sound at the given volume (0.0 - 1.0) on the specified output device
///
/// Sounds are queued and played by a persistent audio thread.
/// If the audio system is unavailable, the request is silently ignored.
///
/// # Arguments
/// * `sound` - Which sound to play
/// * `volume` - Volume level (0.0 - 1.0)
/// * `device_name` - Output device name, or empty string for system default
pub fn play_sound_on_device(sound: &SoundChoice, volume: f32, device_name: &str) {
    let data = get_sound_data(sound);

    if let Some(sender) = get_sound_sender() {
        // Send is non-blocking - if channel is full or disconnected, we just ignore
        let _ = sender.send(SoundRequest {
            data,
            volume,
            device_name: device_name.to_string(),
        });
    }
}

// =============================================================================
// Internal Helpers
// =============================================================================

/// Get the raw audio data for a sound choice
fn get_sound_data(sound: &SoundChoice) -> &'static [u8] {
    match sound {
        SoundChoice::Alert => SOUND_ALERT,
        SoundChoice::Bell => SOUND_BELL,
        SoundChoice::Chime => SOUND_CHIME,
        SoundChoice::Ding => SOUND_DING,
        SoundChoice::Pop => SOUND_POP,
    }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_sound_data_alert() {
        let data = get_sound_data(&SoundChoice::Alert);
        // Verify it's a valid OGG file (starts with "OggS")
        assert!(data.len() > 4);
        assert_eq!(&data[0..4], b"OggS");
    }

    #[test]
    fn test_get_sound_data_bell() {
        let data = get_sound_data(&SoundChoice::Bell);
        assert_eq!(&data[0..4], b"OggS");
    }

    #[test]
    fn test_get_sound_data_chime() {
        let data = get_sound_data(&SoundChoice::Chime);
        assert_eq!(&data[0..4], b"OggS");
    }

    #[test]
    fn test_get_sound_data_ding() {
        let data = get_sound_data(&SoundChoice::Ding);
        assert_eq!(&data[0..4], b"OggS");
    }

    #[test]
    fn test_get_sound_data_pop() {
        let data = get_sound_data(&SoundChoice::Pop);
        assert_eq!(&data[0..4], b"OggS");
    }

    #[test]
    fn select_named_or_default_device_prefers_exact_match() {
        let selected = select_named_or_default_device(
            "Headphones",
            ["Speakers", "Headphones"],
            Some("Default"),
            |name| Some((*name).to_string()),
        );

        assert_eq!(selected, Some("Headphones"));
    }

    #[test]
    fn select_named_or_default_device_falls_back_to_default() {
        let selected = select_named_or_default_device(
            "Missing",
            ["Speakers", "Headphones"],
            Some("Default"),
            |name| Some((*name).to_string()),
        );

        assert_eq!(selected, Some("Default"));
    }

    #[test]
    fn convert_samples_duplicates_mono_to_stereo() {
        let converted = convert_samples_for_output(&[0.25, -0.5], 48_000, 1, 48_000, 2)
            .expect("convert samples");

        assert_eq!(converted, vec![0.25, 0.25, -0.5, -0.5]);
    }

    #[test]
    fn convert_samples_downmixes_stereo_to_mono() {
        let converted = convert_samples_for_output(&[1.0, -1.0, 0.25, 0.75], 48_000, 2, 48_000, 1)
            .expect("convert samples");

        assert_eq!(converted, vec![0.0, 0.5]);
    }

    #[test]
    fn convert_samples_resamples_with_linear_interpolation() {
        let converted =
            convert_samples_for_output(&[0.0, 1.0], 24_000, 1, 48_000, 1).expect("convert samples");

        assert_eq!(converted, vec![0.0, 0.5, 1.0, 1.0]);
    }
}
