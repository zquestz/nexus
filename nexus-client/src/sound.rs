//! Sound playback for event notifications
//!
//! Uses a persistent audio thread with a queue to ensure all sounds play reliably,
//! even when multiple sounds are requested simultaneously.

use std::io::Cursor;
use std::sync::OnceLock;
use std::sync::mpsc::{self, Sender};

use rodio::{Decoder, OutputStream, Sink};

use crate::config::events::SoundChoice;

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
}

/// Global sender for sound requests
static SOUND_SENDER: OnceLock<Sender<SoundRequest>> = OnceLock::new();

/// Initialize the audio thread if not already running
fn get_sound_sender() -> Option<&'static Sender<SoundRequest>> {
    SOUND_SENDER.get_or_init(|| {
        let (tx, rx) = mpsc::channel::<SoundRequest>();

        // Spawn the persistent audio thread
        std::thread::spawn(move || {
            // Try to get the default output stream
            // If this fails, the thread will just exit and sounds won't play
            let Ok((_stream, handle)) = OutputStream::try_default() else {
                // Drain the channel so senders don't block
                for _ in rx {}
                return;
            };

            // Keep track of active sinks so they stay alive until finished
            let mut active_sinks: Vec<Sink> = Vec::new();

            // Process sound requests forever
            for request in rx {
                // Clean up finished sinks
                active_sinks.retain(|sink| !sink.empty());

                // Create a new sink for this sound
                let Ok(sink) = Sink::try_new(&handle) else {
                    continue;
                };

                // Try to decode the audio data
                let Ok(source) = Decoder::new(Cursor::new(request.data)) else {
                    continue;
                };

                // Set volume and play (non-blocking)
                sink.set_volume(request.volume);
                sink.append(source);

                // Keep sink alive until it finishes playing
                active_sinks.push(sink);
            }
        });

        tx
    });

    SOUND_SENDER.get()
}

// =============================================================================
// Public API
// =============================================================================

/// Play a sound at the given volume (0.0 - 1.0)
///
/// Sounds are queued and played sequentially by a persistent audio thread.
/// If the audio system is unavailable, the request is silently ignored.
pub fn play_sound(sound: &SoundChoice, volume: f32) {
    let data = get_sound_data(sound);

    if let Some(sender) = get_sound_sender() {
        // Send is non-blocking - if channel is full or disconnected, we just ignore
        let _ = sender.send(SoundRequest { data, volume });
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
}
