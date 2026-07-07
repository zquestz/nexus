//! WebRTC audio processor for voice enhancement
//!
//! Wraps the webrtc-audio-processing crate to provide:
//! - Automatic Gain Control (AGC)
//! - Noise Suppression (NS)
//! - Echo Cancellation (AEC)

use nexus_common::voice::{VOICE_SAMPLE_RATE, VOICE_SAMPLES_PER_FRAME};
use webrtc_audio_processing::Processor;
use webrtc_audio_processing::config::{
    CaptureAmplifier, CaptureLevelAdjustment, Config, EchoCanceller, GainController,
    GainController2, HighPassFilter, NoiseSuppression, NoiseSuppressionLevel as WebrtcNsLevel,
};
use webrtc_audio_processing::experimental::EchoCanceller3Config;

use crate::config::audio::{MicBoost, NoiseSuppressionLevel};
use crate::constants::{
    ERR_VOICE_CAPTURE_PROCESSING, ERR_VOICE_PROCESSOR_CREATE, ERR_VOICE_RENDER_ANALYSIS,
};

// =============================================================================
// Silence Gate Constants
// =============================================================================

/// RMS threshold below which a processed frame is considered silence.
/// Post-AGC speech typically has RMS 0.05–0.3; post-NS silence is below 0.005.
const SILENCE_RMS_THRESHOLD: f32 = 0.01;

/// Number of consecutive silent frames before gating transmission in toggle mode.
/// At 10ms per frame, 20 frames = 200ms holdover after last detected speech.
/// This prevents clipping word endings without transmitting too much silence.
const SILENCE_HOLDOVER_FRAMES: u32 = 20;

// =============================================================================
// Audio Processor Settings
// =============================================================================

/// Settings for audio processing features
///
/// Default values:
/// - Noise suppression ON: Removes background noise with minimal latency cost
/// - Echo cancellation ON: Cancels speaker echo for users not on headphones; small latency/CPU cost
/// - AGC ON: Normalizes volume levels across different microphones
/// - Transient suppression OFF: Can occasionally clip word beginnings; enable if typing while talking
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AudioProcessorSettings {
    /// Enable noise suppression (default: true)
    /// Removes steady-state background noise (fans, AC, etc.)
    pub noise_suppression: bool,
    /// Noise suppression aggressiveness (default: Moderate)
    pub noise_suppression_level: NoiseSuppressionLevel,
    /// Enable echo cancellation (default: true)
    /// Cancels speaker echo for users not on headphones.
    /// Adds some latency and CPU overhead.
    pub echo_cancellation: bool,
    /// Enable automatic gain control (default: true)
    /// Normalizes microphone volume to consistent levels.
    pub agc: bool,
    /// Enable transient suppression (default: false)
    /// Reduces keyboard clicks, mouse clicks, and other sudden noises.
    /// Can occasionally clip the start of words, so disabled by default.
    pub transient_suppression: bool,
    /// Microphone pre-gain boost (default: Off)
    /// Amplifies quiet mics before processing so AGC/NS have more signal.
    pub mic_boost: MicBoost,
}

impl Default for AudioProcessorSettings {
    fn default() -> Self {
        Self {
            noise_suppression: true,
            noise_suppression_level: NoiseSuppressionLevel::default(),
            echo_cancellation: true,
            agc: true,
            transient_suppression: false,
            mic_boost: MicBoost::default(),
        }
    }
}

// =============================================================================
// Audio Processor
// =============================================================================

/// WebRTC audio processor for voice enhancement
///
/// Processes audio frames to improve voice quality through AGC,
/// noise suppression, and echo cancellation.
pub struct AudioProcessor {
    /// The WebRTC audio processor instance
    processor: Processor,
    /// Current settings
    settings: AudioProcessorSettings,
    /// Consecutive frames below silence threshold (for RMS-based silence gate)
    silence_frames: u32,
}

impl AudioProcessor {
    /// Create a new audio processor with the given settings
    ///
    /// # Arguments
    /// * `settings` - Initial processor settings
    ///
    /// # Returns
    /// * `Ok(AudioProcessor)` - Processor ready for use
    /// * `Err(String)` - Error message if initialization failed
    pub fn new(settings: AudioProcessorSettings) -> Result<Self, String> {
        // Construct with an explicit AEC3 config exporting the linear AEC
        // output. `analyze_linear_aec_output` in set_config is only honored
        // when the Processor was built with this flag (the library forces it
        // off otherwise) because the linear-output framer must exist from
        // construction. The default config matches what a plain
        // `Processor::new` uses, so the export flag is the only delta.
        let mut aec3_config = EchoCanceller3Config::default();
        aec3_config.filter.export_linear_aec_output = true;

        let processor = Processor::with_aec3_config(VOICE_SAMPLE_RATE, aec3_config)
            .map_err(|e| format!("{ERR_VOICE_PROCESSOR_CREATE}: {e}"))?;

        // Apply initial settings
        let config = Self::build_config(&settings);
        processor.set_config(config);

        Ok(Self {
            processor,
            settings,
            silence_frames: 0,
        })
    }

    /// Build a Config from our settings
    fn build_config(settings: &AudioProcessorSettings) -> Config {
        let echo_cancellation = settings.echo_cancellation;

        Config {
            capture_amplifier: if settings.mic_boost != MicBoost::Off {
                Some(CaptureAmplifier::CaptureLevelAdjustment(
                    CaptureLevelAdjustment {
                        pre_gain_factor: settings.mic_boost.gain_factor(),
                        ..CaptureLevelAdjustment::default()
                    },
                ))
            } else {
                None
            },
            echo_canceller: if echo_cancellation {
                Some(EchoCanceller::Full {
                    stream_delay_ms: None,
                })
            } else {
                None
            },
            gain_controller: if settings.agc {
                Some(GainController::GainController2(GainController2 {
                    adaptive_digital: Some(Default::default()),
                    ..GainController2::default()
                }))
            } else {
                None
            },
            // Gate on both the bool and the level for backward compat:
            // old configs may have noise_suppression=false with level
            // defaulting to Moderate (serde default). The bool ensures
            // those users don't get NS silently re-enabled on upgrade.
            noise_suppression: match settings.noise_suppression_level {
                NoiseSuppressionLevel::Off => None,
                _ if !settings.noise_suppression => None,
                level => Some(NoiseSuppression {
                    level: match level {
                        NoiseSuppressionLevel::Low => WebrtcNsLevel::Low,
                        NoiseSuppressionLevel::Moderate => WebrtcNsLevel::Moderate,
                        NoiseSuppressionLevel::High => WebrtcNsLevel::High,
                        NoiseSuppressionLevel::VeryHigh => WebrtcNsLevel::VeryHigh,
                        // Off is filtered by the outer arm, so this isn't
                        // expected to fire. Fall back to the project default
                        // (Moderate) rather than panicking — a future
                        // refactor should not be able to crash voice
                        // processor init.
                        NoiseSuppressionLevel::Off => WebrtcNsLevel::Moderate,
                    },
                    // NS analyzes the echo-cancelled linear AEC signal instead
                    // of the post-suppression capture, keeping its noise
                    // estimate stable during far-end speech. Safe to request
                    // unconditionally: the library activates it only when the
                    // echo canceller is enabled and the Processor was
                    // constructed with `export_linear_aec_output`.
                    analyze_linear_aec_output: true,
                }),
            },
            high_pass_filter: Some(HighPassFilter::default()),
            transient_suppression: settings.transient_suppression,
            ..Config::default()
        }
    }

    /// Update processor settings dynamically
    ///
    /// # Arguments
    /// * `settings` - New processor settings
    pub fn update_settings(&mut self, settings: AudioProcessorSettings) {
        if settings != self.settings {
            let config = Self::build_config(&settings);
            self.processor.set_config(config);
            self.settings = settings;
        }
    }

    /// Hint to AEC/AGC that output audio is muted (e.g., user is deafened).
    ///
    /// When muted, there is no speaker output and thus no echo to cancel.
    /// This lets the processor skip unnecessary work and adapt parameters.
    pub fn set_output_will_be_muted(&self, muted: bool) {
        self.processor.set_output_will_be_muted(muted);
    }

    /// Hint to the transient suppressor that a key press is occurring.
    ///
    /// Called on PTT key press/release to help identify keyboard transients.
    pub fn set_stream_key_pressed(&self, pressed: bool) {
        self.processor.set_stream_key_pressed(pressed);
    }

    /// Check if the processed capture frame contains speech.
    ///
    /// Uses an RMS-based silence gate since WebRTC 2.0 removed runtime VAD.
    /// Returns true if the frame is above the silence threshold, or if we're
    /// still within the holdover period after the last speech frame.
    /// Used for VAD-gated transmission in toggle PTT mode.
    pub fn has_voice(&mut self, frame: &[f32]) -> bool {
        let sum_squares: f64 = frame.iter().map(|&s| (s as f64) * (s as f64)).sum();
        let rms = (sum_squares / frame.len() as f64).sqrt() as f32;

        if rms > SILENCE_RMS_THRESHOLD {
            self.silence_frames = 0;
            true
        } else {
            self.silence_frames = self.silence_frames.saturating_add(1);
            self.silence_frames <= SILENCE_HOLDOVER_FRAMES
        }
    }

    /// Process a capture (microphone) frame
    ///
    /// This should be called on microphone input before encoding.
    /// The frame is modified in place.
    ///
    /// # Arguments
    /// * `frame` - Audio frame (must be VOICE_SAMPLES_PER_FRAME samples)
    pub fn process_capture_frame(&mut self, frame: &mut [f32]) -> Result<(), String> {
        if frame.len() != VOICE_SAMPLES_PER_FRAME as usize {
            return Err(format!(
                "Expected {} samples, got {}",
                VOICE_SAMPLES_PER_FRAME,
                frame.len()
            ));
        }

        self.processor
            .process_capture_frame([frame])
            .map_err(|e| format!("{ERR_VOICE_CAPTURE_PROCESSING}: {e}"))
    }

    /// Analyze a render (speaker) frame for echo cancellation reference
    ///
    /// This should be called on audio before playback. Required for
    /// echo cancellation to work - the processor needs to know what
    /// audio is being played to remove it from the microphone signal.
    ///
    /// Unlike `process_render_frame`, this does not modify the audio data.
    ///
    /// # Arguments
    /// * `frame` - Audio frame (must be VOICE_SAMPLES_PER_FRAME samples)
    pub fn analyze_render_frame(&self, frame: &[f32]) -> Result<(), String> {
        if frame.len() != VOICE_SAMPLES_PER_FRAME as usize {
            return Err(format!(
                "Expected {} samples, got {}",
                VOICE_SAMPLES_PER_FRAME,
                frame.len()
            ));
        }

        self.processor
            .analyze_render_frame([frame])
            .map_err(|e| format!("{ERR_VOICE_RENDER_ANALYSIS}: {e}"))
    }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use serial_test::serial;

    impl AudioProcessor {
        fn settings(&self) -> AudioProcessorSettings {
            self.settings
        }
    }

    /// Temporary Windows arm64 FFI diagnostic: checkpoints each stage so the
    /// serialized CI diagnostic step shows which one crashes — null-config
    /// construction, full-config DSP (NEON/pffft paths), the
    /// `EchoCanceller3Config` FFI crossing, or AEC3-specific processing.
    /// Named to sort before the `test_*` tests. Remove once the arm64
    /// access violation is resolved.
    #[test]
    fn a_probe_ffi_call_sequence() {
        let settings = AudioProcessorSettings::default();
        let full_config = AudioProcessor::build_config(&settings);
        let render = vec![0.0f32; VOICE_SAMPLES_PER_FRAME as usize];
        let mut capture = vec![0.0f32; VOICE_SAMPLES_PER_FRAME as usize];

        eprintln!("probe 1: Processor::new (null aec3 config)");
        let processor = Processor::new(VOICE_SAMPLE_RATE).expect("construct processor");

        eprintln!("probe 2: set_config (NS + AGC2 + AEC)");
        processor.set_config(full_config);

        eprintln!("probe 3: frames through the full-config DSP (NEON/pffft paths)");
        for _ in 0..5 {
            let _ = processor.analyze_render_frame([render.as_slice()]);
            let _ = processor.process_capture_frame([capture.as_mut_slice()]);
        }
        drop(processor);

        eprintln!("probe 4: Processor::with_aec3_config");
        let mut aec3_config = EchoCanceller3Config::default();
        aec3_config.filter.export_linear_aec_output = true;
        let processor = Processor::with_aec3_config(VOICE_SAMPLE_RATE, aec3_config)
            .expect("construct processor with aec3 config");

        eprintln!("probe 5: set_config + frames on the aec3-config processor");
        processor.set_config(full_config);
        for _ in 0..5 {
            let _ = processor.analyze_render_frame([render.as_slice()]);
            let _ = processor.process_capture_frame([capture.as_mut_slice()]);
        }

        eprintln!("probe: done");
    }

    #[test]
    fn test_default_settings() {
        let settings = AudioProcessorSettings::default();
        assert!(settings.noise_suppression);
        assert_eq!(
            settings.noise_suppression_level,
            NoiseSuppressionLevel::Moderate
        );
        assert!(settings.echo_cancellation);
        assert!(settings.agc);
        assert_eq!(settings.mic_boost, MicBoost::Off);
    }

    // Serialize WebRTC processor tests - the library has global state that isn't
    // thread-safe when creating multiple Processor instances concurrently.
    #[test]
    #[serial]
    fn test_processor_creation() {
        let processor = AudioProcessor::new(AudioProcessorSettings::default());
        assert!(processor.is_ok());
    }

    #[test]
    #[serial]
    fn test_process_capture_frame() {
        let mut processor = AudioProcessor::new(AudioProcessorSettings::default()).unwrap();
        let mut frame = vec![0.0f32; VOICE_SAMPLES_PER_FRAME as usize];

        let result = processor.process_capture_frame(&mut frame);
        assert!(result.is_ok());
    }

    #[test]
    #[serial]
    fn test_analyze_render_frame() {
        let processor = AudioProcessor::new(AudioProcessorSettings::default()).unwrap();
        let frame = vec![0.0f32; VOICE_SAMPLES_PER_FRAME as usize];

        let result = processor.analyze_render_frame(&frame);
        assert!(result.is_ok());
    }

    #[test]
    #[serial]
    fn test_wrong_frame_size() {
        let mut processor = AudioProcessor::new(AudioProcessorSettings::default()).unwrap();
        let mut frame = vec![0.0f32; 100]; // Wrong size

        assert!(processor.process_capture_frame(&mut frame).is_err());
        assert!(processor.analyze_render_frame(&frame).is_err());
    }

    #[test]
    #[serial]
    fn test_update_settings() {
        let mut processor = AudioProcessor::new(AudioProcessorSettings::default()).unwrap();

        let new_settings = AudioProcessorSettings {
            noise_suppression: false,
            noise_suppression_level: NoiseSuppressionLevel::High,
            echo_cancellation: true,
            agc: false,
            transient_suppression: true,
            mic_boost: MicBoost::Plus6dB,
        };

        processor.update_settings(new_settings);
        assert_eq!(processor.settings(), new_settings);
    }

    #[test]
    #[serial]
    fn test_signal_processing() {
        let mut processor = AudioProcessor::new(AudioProcessorSettings {
            noise_suppression: true,
            noise_suppression_level: NoiseSuppressionLevel::default(),
            echo_cancellation: false,
            agc: true,
            transient_suppression: false,
            mic_boost: MicBoost::default(),
        })
        .unwrap();

        // Create a test signal (sine wave with some noise)
        let mut frame: Vec<f32> = (0..VOICE_SAMPLES_PER_FRAME)
            .map(|i| {
                let t = i as f32 / VOICE_SAMPLE_RATE as f32;
                let signal = f32::sin(2.0 * std::f32::consts::PI * 440.0 * t) * 0.3;
                let noise = ((i * 12345) % 100) as f32 / 1000.0 - 0.05;
                signal + noise
            })
            .collect();

        // Process should succeed
        let result = processor.process_capture_frame(&mut frame);
        assert!(result.is_ok());

        // Frame should still have reasonable values
        let max_val = frame.iter().map(|&x| x.abs()).fold(0.0f32, f32::max);
        assert!(max_val <= 1.5, "Output should be reasonably bounded");
    }

    #[test]
    fn test_has_voice_detects_speech() {
        let mut processor = AudioProcessor::new(AudioProcessorSettings::default()).unwrap();

        // A loud sine wave should be detected as speech
        let frame: Vec<f32> = (0..VOICE_SAMPLES_PER_FRAME)
            .map(|i| {
                let t = i as f32 / VOICE_SAMPLE_RATE as f32;
                f32::sin(2.0 * std::f32::consts::PI * 440.0 * t) * 0.3
            })
            .collect();

        assert!(processor.has_voice(&frame));
    }

    #[test]
    fn test_has_voice_silence_gate() {
        let mut processor = AudioProcessor::new(AudioProcessorSettings::default()).unwrap();

        let silence = vec![0.0f32; VOICE_SAMPLES_PER_FRAME as usize];

        // First silent frames should return true (holdover period)
        assert!(processor.has_voice(&silence));

        // After enough silent frames, should return false
        for _ in 0..SILENCE_HOLDOVER_FRAMES + 1 {
            processor.has_voice(&silence);
        }
        assert!(!processor.has_voice(&silence));
    }

    #[test]
    fn test_has_voice_holdover_resets_on_speech() {
        let mut processor = AudioProcessor::new(AudioProcessorSettings::default()).unwrap();

        let silence = vec![0.0f32; VOICE_SAMPLES_PER_FRAME as usize];
        let speech: Vec<f32> = (0..VOICE_SAMPLES_PER_FRAME)
            .map(|i| {
                let t = i as f32 / VOICE_SAMPLE_RATE as f32;
                f32::sin(2.0 * std::f32::consts::PI * 440.0 * t) * 0.3
            })
            .collect();

        // Drain holdover with silence
        for _ in 0..SILENCE_HOLDOVER_FRAMES + 5 {
            processor.has_voice(&silence);
        }
        assert!(!processor.has_voice(&silence));

        // Speech resets the gate
        assert!(processor.has_voice(&speech));

        // Holdover starts fresh — silence should return true again
        assert!(processor.has_voice(&silence));
    }
}
