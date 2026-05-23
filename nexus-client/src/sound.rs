//! Sound playback for event notifications
//!
//! Uses cpal directly for audio output, with lewton for OGG/Vorbis decoding.
//! Provides a simple queue-based system for playing notification sounds.

use std::io::Cursor;
use std::sync::Mutex;
use std::sync::mpsc::{self, Sender};

use cpal::StreamConfig;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use lewton::inside_ogg::OggStreamReader;
use once_cell::sync::Lazy;

use crate::config::events::SoundChoice;
use crate::constants::ERR_SOUND_STATE_LOCK_POISONED;

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
        OggStreamReader::new(cursor).map_err(|e| format!("Failed to decode OGG: {}", e))?;

    let sample_rate = reader.ident_hdr.audio_sample_rate;
    let channels = reader.ident_hdr.audio_channels as u16;

    // Collect all samples (sounds are short, so this is fine)
    let mut samples: Vec<f32> = Vec::new();
    while let Some(packet) = reader
        .read_dec_packet_itl()
        .map_err(|e| format!("Decode error: {}", e))?
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

    // Build stream config
    let config = StreamConfig {
        channels,
        sample_rate,
        buffer_size: cpal::BufferSize::Default,
    };

    // Create a channel to signal when playback is done
    let (done_tx, done_rx) = mpsc::channel::<()>();

    // Track playback position
    let samples_len = samples.len();
    let position = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let position_clone = position.clone();

    // Build the output stream
    let stream = device
        .build_output_stream(
            &config,
            move |output: &mut [f32], _: &cpal::OutputCallbackInfo| {
                let pos = position_clone.load(std::sync::atomic::Ordering::Relaxed);
                for (i, sample) in output.iter_mut().enumerate() {
                    let idx = pos + i;
                    if idx < samples_len {
                        *sample = samples[idx];
                    } else {
                        *sample = 0.0;
                    }
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
        .map_err(|e| format!("Failed to build output stream: {}", e))?;

    // Start playback
    stream
        .play()
        .map_err(|e| format!("Failed to start playback: {}", e))?;

    // Wait for playback to complete (with timeout)
    let _ = done_rx.recv_timeout(std::time::Duration::from_secs(10));

    // Stream is dropped here, stopping playback
    Ok(())
}

/// Get an output device by name, or the default device if name is empty
fn get_output_device(device_name: &str) -> Result<cpal::Device, String> {
    let host = cpal::default_host();

    if device_name.is_empty() {
        host.default_output_device()
            .ok_or_else(|| "No default output device available".to_string())
    } else {
        let devices = host
            .output_devices()
            .map_err(|e| format!("Failed to enumerate devices: {}", e))?;

        for device in devices {
            if let Ok(desc) = device.description()
                && desc.name() == device_name
            {
                return Ok(device);
            }
        }

        // Device not found, fall back to default
        host.default_output_device()
            .ok_or_else(|| "No default output device available".to_string())
    }
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

/// Play a sound at the given volume (0.0 - 1.0) on the system default device
///
/// This is a convenience wrapper around `play_sound_on_device` for cases
/// where device selection is not needed.
///
/// # Arguments
/// * `sound` - Which sound to play
/// * `volume` - Volume level (0.0 - 1.0)
#[allow(dead_code)]
pub fn play_sound(sound: &SoundChoice, volume: f32) {
    play_sound_on_device(sound, volume, "");
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
}
