//! Sound playback for event notifications

use std::io::Cursor;

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
// Public API
// =============================================================================

/// Play a sound at the given volume (0.0 - 1.0)
///
/// Spawns a thread to avoid blocking the UI. If no audio device is available
/// or the sound fails to play, the error is silently ignored.
pub fn play_sound(sound: &SoundChoice, volume: f32) {
    let data = get_sound_data(sound);

    // Spawn a thread so we don't block the UI
    std::thread::spawn(move || {
        // Try to get the default output stream
        let Ok((_stream, handle)) = OutputStream::try_default() else {
            return;
        };

        // Try to create a sink for playback
        let Ok(sink) = Sink::try_new(&handle) else {
            return;
        };

        // Try to decode the audio data
        let Ok(source) = Decoder::new(Cursor::new(data)) else {
            return;
        };

        // Set volume and play
        sink.set_volume(volume);
        sink.append(source);

        // Block until the sound finishes playing
        sink.sleep_until_end();
    });
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
