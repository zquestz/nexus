//! Audio settings for voice chat
//!
//! Configuration for voice chat audio including device selection,
//! voice quality, and push-to-talk settings.

use nexus_common::voice::VoiceQuality;
use serde::{Deserialize, Serialize};

// =============================================================================
// Localized Voice Quality
// =============================================================================

/// Wrapper for VoiceQuality that implements Display with i18n
///
/// This is needed because VoiceQuality is in nexus-common which doesn't
/// have access to the client's i18n system. The pick_list widget uses
/// Display to render options.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LocalizedVoiceQuality(pub VoiceQuality);

impl LocalizedVoiceQuality {
    /// Get all quality options as localized wrappers
    pub fn all() -> Vec<LocalizedVoiceQuality> {
        VoiceQuality::all()
            .iter()
            .map(|&q| LocalizedVoiceQuality(q))
            .collect()
    }
}

impl std::fmt::Display for LocalizedVoiceQuality {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let key = match self.0 {
            VoiceQuality::Low => "voice-quality-low",
            VoiceQuality::Medium => "voice-quality-medium",
            VoiceQuality::High => "voice-quality-high",
            VoiceQuality::VeryHigh => "voice-quality-very-high",
        };
        write!(f, "{}", crate::i18n::t(key))
    }
}

impl From<VoiceQuality> for LocalizedVoiceQuality {
    fn from(q: VoiceQuality) -> Self {
        LocalizedVoiceQuality(q)
    }
}

impl From<LocalizedVoiceQuality> for VoiceQuality {
    fn from(lq: LocalizedVoiceQuality) -> Self {
        lq.0
    }
}

// =============================================================================
// Constants
// =============================================================================

/// Default PTT key (backtick)
pub const DEFAULT_PTT_KEY: &str = "`";

/// System default device identifier
pub const SYSTEM_DEFAULT_DEVICE: &str = "";

// =============================================================================
// PTT Mode
// =============================================================================

/// Push-to-talk activation mode
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum PttMode {
    /// Hold key to talk, release to stop
    #[default]
    Hold,
    /// Press to start talking, press again to stop
    Toggle,
}

impl PttMode {
    /// All PTT modes for the picker
    pub const ALL: &'static [PttMode] = &[PttMode::Hold, PttMode::Toggle];

    /// Get the translation key for this mode
    pub fn translation_key(self) -> &'static str {
        match self {
            PttMode::Hold => "ptt-mode-hold",
            PttMode::Toggle => "ptt-mode-toggle",
        }
    }
}

impl std::fmt::Display for PttMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", crate::i18n::t(self.translation_key()))
    }
}

// =============================================================================
// PTT Release Delay
// =============================================================================

/// Delay before stopping transmission after PTT key release
///
/// Prevents cutting off the end of words/sentences when releasing PTT key.
/// Continues transmitting for a short delay after key release.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum PttReleaseDelay {
    /// No delay - stop immediately on release
    #[default]
    Off,
    /// 100 millisecond delay
    Ms100,
    /// 300 millisecond delay
    Ms300,
    /// 500 millisecond delay
    Ms500,
}

impl PttReleaseDelay {
    /// All delay options for the picker
    pub const ALL: &'static [PttReleaseDelay] = &[
        PttReleaseDelay::Off,
        PttReleaseDelay::Ms100,
        PttReleaseDelay::Ms300,
        PttReleaseDelay::Ms500,
    ];

    /// Get the translation key for this delay
    pub fn translation_key(self) -> &'static str {
        match self {
            PttReleaseDelay::Off => "ptt-delay-off",
            PttReleaseDelay::Ms100 => "ptt-delay-100ms",
            PttReleaseDelay::Ms300 => "ptt-delay-300ms",
            PttReleaseDelay::Ms500 => "ptt-delay-500ms",
        }
    }

    /// Get the delay duration in milliseconds (0 for Off)
    pub fn as_millis(self) -> u64 {
        match self {
            PttReleaseDelay::Off => 0,
            PttReleaseDelay::Ms100 => 100,
            PttReleaseDelay::Ms300 => 300,
            PttReleaseDelay::Ms500 => 500,
        }
    }
}

impl std::fmt::Display for PttReleaseDelay {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", crate::i18n::t(self.translation_key()))
    }
}

// =============================================================================
// Noise Suppression Level
// =============================================================================

/// Noise suppression aggressiveness level
///
/// Higher levels remove more noise but may introduce speech distortion.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum NoiseSuppressionLevel {
    /// Disabled
    Off,
    /// Minimal suppression, least distortion
    Low,
    /// Balanced suppression and quality (default)
    #[default]
    Moderate,
    /// Aggressive suppression, some distortion possible
    High,
    /// Maximum suppression, most distortion risk
    VeryHigh,
}

impl NoiseSuppressionLevel {
    /// All levels for the picker
    pub const ALL: &'static [NoiseSuppressionLevel] = &[
        NoiseSuppressionLevel::Off,
        NoiseSuppressionLevel::Low,
        NoiseSuppressionLevel::Moderate,
        NoiseSuppressionLevel::High,
        NoiseSuppressionLevel::VeryHigh,
    ];

    /// Whether noise suppression is enabled at this level
    pub fn is_enabled(self) -> bool {
        self != NoiseSuppressionLevel::Off
    }

    /// Get the translation key for this level
    pub fn translation_key(self) -> &'static str {
        match self {
            NoiseSuppressionLevel::Off => "noise-level-off",
            NoiseSuppressionLevel::Low => "noise-level-low",
            NoiseSuppressionLevel::Moderate => "noise-level-moderate",
            NoiseSuppressionLevel::High => "noise-level-high",
            NoiseSuppressionLevel::VeryHigh => "noise-level-very-high",
        }
    }
}

impl std::fmt::Display for NoiseSuppressionLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", crate::i18n::t(self.translation_key()))
    }
}

// =============================================================================
// Microphone Boost
// =============================================================================

/// Microphone pre-gain boost level
///
/// Applies a fixed gain multiplier to the mic signal before any processing.
/// Useful for quiet microphones that AGC alone can't bring to usable levels.
/// Each step is +6 dB (2x amplification).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum MicBoost {
    /// No boost (1.0x)
    #[default]
    Off,
    /// +6 dB (2.0x)
    Plus6dB,
    /// +12 dB (4.0x)
    Plus12dB,
    /// +18 dB (8.0x)
    Plus18dB,
}

impl MicBoost {
    /// All boost levels for the picker
    pub const ALL: &'static [MicBoost] = &[
        MicBoost::Off,
        MicBoost::Plus6dB,
        MicBoost::Plus12dB,
        MicBoost::Plus18dB,
    ];

    /// Get the translation key for this level
    pub fn translation_key(self) -> &'static str {
        match self {
            MicBoost::Off => "mic-boost-off",
            MicBoost::Plus6dB => "mic-boost-6db",
            MicBoost::Plus12dB => "mic-boost-12db",
            MicBoost::Plus18dB => "mic-boost-18db",
        }
    }

    /// Get the linear gain factor
    pub fn gain_factor(self) -> f32 {
        match self {
            MicBoost::Off => 1.0,
            MicBoost::Plus6dB => 2.0,
            MicBoost::Plus12dB => 4.0,
            MicBoost::Plus18dB => 8.0,
        }
    }
}

impl std::fmt::Display for MicBoost {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", crate::i18n::t(self.translation_key()))
    }
}

// =============================================================================
// Audio Settings
// =============================================================================

/// Audio settings for voice chat
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AudioSettings {
    /// Output device name (empty string = system default)
    #[serde(default)]
    pub output_device: String,

    /// Input device name (empty string = system default)
    #[serde(default)]
    pub input_device: String,

    /// Voice quality preset (affects Opus bitrate)
    #[serde(default)]
    pub voice_quality: VoiceQuality,

    /// Push-to-talk key binding
    #[serde(default = "default_ptt_key")]
    pub ptt_key: String,

    /// Push-to-talk mode (hold or toggle)
    #[serde(default)]
    pub ptt_mode: PttMode,

    /// Push-to-talk release delay
    #[serde(default)]
    pub ptt_release_delay: PttReleaseDelay,

    /// Enable noise suppression (default: true)
    #[serde(default = "default_true")]
    pub noise_suppression: bool,

    /// Noise suppression aggressiveness level (default: Moderate)
    #[serde(default)]
    pub noise_suppression_level: NoiseSuppressionLevel,

    /// Enable echo cancellation (default: false, for headphone users)
    #[serde(default)]
    pub echo_cancellation: bool,

    /// Enable automatic gain control (default: true)
    #[serde(default = "default_true")]
    pub agc: bool,

    /// Enable transient suppression (default: false)
    /// Reduces keyboard clicks, mouse clicks, and other sudden noises
    #[serde(default)]
    pub transient_suppression: bool,

    /// Microphone boost level (default: Off)
    /// Pre-gain applied before all processing for quiet microphones
    #[serde(default)]
    pub mic_boost: MicBoost,
}

fn default_true() -> bool {
    true
}

impl Default for AudioSettings {
    fn default() -> Self {
        Self {
            output_device: SYSTEM_DEFAULT_DEVICE.to_string(),
            input_device: SYSTEM_DEFAULT_DEVICE.to_string(),
            voice_quality: VoiceQuality::default(),
            ptt_key: default_ptt_key(),
            ptt_mode: PttMode::default(),
            ptt_release_delay: PttReleaseDelay::default(),
            noise_suppression: true,
            noise_suppression_level: NoiseSuppressionLevel::default(),
            echo_cancellation: false,
            agc: true,
            transient_suppression: false,
            mic_boost: MicBoost::default(),
        }
    }
}

fn default_ptt_key() -> String {
    DEFAULT_PTT_KEY.to_string()
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    impl AudioSettings {
        fn is_default_output(&self) -> bool {
            self.output_device.is_empty()
        }

        fn is_default_input(&self) -> bool {
            self.input_device.is_empty()
        }
    }

    #[test]
    fn test_default_audio_settings() {
        let settings = AudioSettings::default();
        assert!(settings.is_default_output());
        assert!(settings.is_default_input());
        assert_eq!(settings.voice_quality, VoiceQuality::High);
        assert_eq!(settings.ptt_key, DEFAULT_PTT_KEY);
        assert_eq!(settings.ptt_mode, PttMode::Hold);
        assert_eq!(settings.ptt_release_delay, PttReleaseDelay::Off);
        assert!(settings.noise_suppression);
        assert_eq!(
            settings.noise_suppression_level,
            NoiseSuppressionLevel::Moderate
        );
        assert!(!settings.echo_cancellation);
        assert!(settings.agc);
        assert!(!settings.transient_suppression);
        assert_eq!(settings.mic_boost, MicBoost::Off);
    }

    #[test]
    fn test_audio_settings_serialization_roundtrip() {
        let settings = AudioSettings {
            output_device: "Headphones".to_string(),
            input_device: "USB Microphone".to_string(),
            voice_quality: VoiceQuality::VeryHigh,
            ptt_key: "F1".to_string(),
            ptt_mode: PttMode::Toggle,
            ptt_release_delay: PttReleaseDelay::Ms300,
            noise_suppression: false,
            noise_suppression_level: NoiseSuppressionLevel::High,
            echo_cancellation: true,
            agc: false,
            transient_suppression: true,
            mic_boost: MicBoost::Plus12dB,
        };

        let json = serde_json::to_string(&settings).expect("serialize");
        let deserialized: AudioSettings = serde_json::from_str(&json).expect("deserialize");

        assert_eq!(settings.output_device, deserialized.output_device);
        assert_eq!(settings.input_device, deserialized.input_device);
        assert_eq!(settings.voice_quality, deserialized.voice_quality);
        assert_eq!(settings.ptt_key, deserialized.ptt_key);
        assert_eq!(settings.ptt_mode, deserialized.ptt_mode);
        assert_eq!(settings.ptt_release_delay, deserialized.ptt_release_delay);
        assert_eq!(settings.noise_suppression, deserialized.noise_suppression);
        assert_eq!(
            settings.noise_suppression_level,
            deserialized.noise_suppression_level
        );
        assert_eq!(settings.echo_cancellation, deserialized.echo_cancellation);
        assert_eq!(settings.agc, deserialized.agc);
        assert_eq!(
            settings.transient_suppression,
            deserialized.transient_suppression
        );
        assert_eq!(settings.mic_boost, deserialized.mic_boost);
    }

    #[test]
    fn test_ptt_mode_all() {
        assert_eq!(PttMode::ALL.len(), 2);
        assert!(PttMode::ALL.contains(&PttMode::Hold));
        assert!(PttMode::ALL.contains(&PttMode::Toggle));
    }

    #[test]
    fn test_ptt_release_delay_all() {
        assert_eq!(PttReleaseDelay::ALL.len(), 4);
        assert!(PttReleaseDelay::ALL.contains(&PttReleaseDelay::Off));
        assert!(PttReleaseDelay::ALL.contains(&PttReleaseDelay::Ms100));
        assert!(PttReleaseDelay::ALL.contains(&PttReleaseDelay::Ms300));
        assert!(PttReleaseDelay::ALL.contains(&PttReleaseDelay::Ms500));
    }

    #[test]
    fn test_ptt_release_delay_as_millis() {
        assert_eq!(PttReleaseDelay::Off.as_millis(), 0);
        assert_eq!(PttReleaseDelay::Ms100.as_millis(), 100);
        assert_eq!(PttReleaseDelay::Ms300.as_millis(), 300);
        assert_eq!(PttReleaseDelay::Ms500.as_millis(), 500);
    }

    #[test]
    fn test_is_default_device() {
        let mut settings = AudioSettings::default();
        assert!(settings.is_default_output());
        assert!(settings.is_default_input());

        settings.output_device = "Some Device".to_string();
        assert!(!settings.is_default_output());
        assert!(settings.is_default_input());

        settings.input_device = "Another Device".to_string();
        assert!(!settings.is_default_output());
        assert!(!settings.is_default_input());
    }
}
