//! Voice chat manager
//!
//! Orchestrates all voice chat components: DTLS connection, audio capture/playback,
//! Opus codec, jitter buffer, and push-to-talk.

use std::collections::HashSet;
use std::net::SocketAddr;
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
use std::thread::JoinHandle;
use std::time::Duration;

use tokio::sync::mpsc;
use uuid::Uuid;

use nexus_common::names::fold_name;
use nexus_common::voice::{VOICE_SAMPLES_PER_FRAME, VoiceQuality};

use crate::config::audio::PttMode;
use crate::constants::ERR_VOICE_THREAD_TOKIO_RUNTIME;

use super::audio::{AudioCapture, AudioMixer};
use super::codec::{DecoderPool, VoiceEncoder};
use super::dtls::{VoiceDtlsCommand, VoiceDtlsEvent, run_voice_client};
use super::jitter::JitterBufferPool;
use super::processor::{AudioProcessor, AudioProcessorSettings};

// =============================================================================
// Voice Session Configuration
// =============================================================================

/// Configuration for starting a voice session
pub struct VoiceSessionConfig {
    /// Server address for DTLS connection
    pub server_addr: SocketAddr,
    /// Voice session token from VoiceJoinResponse
    pub token: Uuid,
    /// Input device name (empty for default)
    pub input_device: String,
    /// Output device name (empty for default)
    pub output_device: String,
    /// Voice quality preset (Opus bitrate)
    pub quality: VoiceQuality,
    /// Audio processing settings (noise suppression, AEC, AGC)
    pub processor_settings: AudioProcessorSettings,
    /// Push-to-talk mode (hold or toggle)
    pub ptt_mode: PttMode,
    /// Shared mic level for VU meter display (f32 stored as bits, written by manager)
    pub mic_level: Arc<AtomicU32>,
}

// =============================================================================
// Constants
// =============================================================================

/// Interval for processing audio frames (10ms = 100 frames/second)
const AUDIO_PROCESS_INTERVAL_MS: u64 = 10;

/// Scaling factor for RMS to UI level conversion (provides headroom for typical speech)
const RMS_DISPLAY_SCALE: f64 = 2.0;

// =============================================================================
// Helper Functions
// =============================================================================

/// Calculate RMS level from audio samples for VU meter display
///
/// Returns a value from 0.0 to 1.0 representing the audio level.
fn calculate_rms_level(samples: &[f32]) -> f32 {
    if samples.is_empty() {
        return 0.0;
    }

    let sum_squares: f64 = samples.iter().map(|&s| (s as f64) * (s as f64)).sum();
    let rms = (sum_squares / samples.len() as f64).sqrt();

    // Convert to 0-1 range with some headroom
    (rms * RMS_DISPLAY_SCALE).min(1.0) as f32
}

// =============================================================================
// Voice Events
// =============================================================================

/// Events emitted by the voice manager
#[derive(Debug, Clone)]
pub enum VoiceEvent {
    /// DTLS connection established
    Connected,
    /// DTLS connection failed
    ConnectionFailed(String),
    /// DTLS connection lost
    Disconnected(Option<String>),
    /// A user started speaking
    SpeakingStarted(String),
    /// A user stopped speaking
    SpeakingStopped(String),
    /// Audio device error (should leave voice)
    AudioError(String),
    /// Local speaking state changed
    LocalSpeakingChanged(bool),
    /// Audio processor failed to initialize (voice works, but no noise suppression/AGC)
    AudioProcessorDisabled(String),
    /// Voice quality change failed
    QualityChangeFailed(String),
}

/// Commands to control the voice manager
#[derive(Debug)]
pub enum VoiceCommand {
    /// Start PTT (begin transmitting)
    StartTransmitting,
    /// Stop PTT (stop transmitting)
    StopTransmitting,
    /// Mute a user
    MuteUser(String),
    /// Unmute a user
    UnmuteUser(String),
    /// Set deafened state (mute all incoming audio)
    SetDeafened(bool),
    /// Update voice quality (bitrate) dynamically
    SetQuality(VoiceQuality),
    /// Update audio processor settings
    SetProcessorSettings(AudioProcessorSettings),
    /// Clean up resources for a user who left voice
    UserLeft(String),
    /// Stop voice session
    Stop,
}

// =============================================================================
// Voice Session Runner
// =============================================================================

/// Run a voice session
///
/// This function runs the voice session in the background, handling:
/// - DTLS connection to server
/// - Audio capture from microphone
/// - Audio playback to speakers
/// - Opus encoding/decoding
/// - Jitter buffering
///
/// # Arguments
/// * `config` - Voice session configuration
/// * `event_tx` - Channel to send voice events
/// * `command_rx` - Channel to receive voice commands
async fn run_voice_session(
    config: VoiceSessionConfig,
    event_tx: mpsc::UnboundedSender<VoiceEvent>,
    mut command_rx: mpsc::UnboundedReceiver<VoiceCommand>,
) {
    // Create channels for DTLS client
    let (dtls_event_tx, mut dtls_event_rx) = mpsc::unbounded_channel();
    let (dtls_command_tx, dtls_command_rx) = mpsc::unbounded_channel();

    // Spawn DTLS client task
    let dtls_handle = tokio::spawn(run_voice_client(
        config.server_addr,
        config.token,
        dtls_event_tx,
        dtls_command_rx,
    ));

    // Wait for DTLS connection
    let connected = loop {
        tokio::select! {
            event = dtls_event_rx.recv() => {
                match event {
                    Some(VoiceDtlsEvent::Connected) => {
                        let _ = event_tx.send(VoiceEvent::Connected);
                        break true;
                    }
                    Some(VoiceDtlsEvent::Error(e)) => {
                        let _ = event_tx.send(VoiceEvent::ConnectionFailed(e));
                        break false;
                    }
                    Some(VoiceDtlsEvent::Disconnected) => {
                        let _ = event_tx.send(VoiceEvent::ConnectionFailed("Connection closed".to_string()));
                        break false;
                    }
                    _ => continue,
                }
            }
            cmd = command_rx.recv() => {
                if matches!(cmd, Some(VoiceCommand::Stop) | None) {
                    let _ = dtls_command_tx.send(VoiceDtlsCommand::Disconnect);
                    break false;
                }
            }
        }
    };

    if !connected {
        dtls_handle.abort();
        return;
    }

    // Initialize audio components
    let capture = match AudioCapture::new(&config.input_device) {
        Ok(c) => c,
        Err(e) => {
            let _ = event_tx.send(VoiceEvent::AudioError(format!("Input device error: {}", e)));
            let _ = dtls_command_tx.send(VoiceDtlsCommand::Disconnect);
            dtls_handle.abort();
            return;
        }
    };

    let mut mixer = match AudioMixer::new(&config.output_device) {
        Ok(m) => m,
        Err(e) => {
            let _ = event_tx.send(VoiceEvent::AudioError(format!(
                "Output device error: {}",
                e
            )));
            let _ = dtls_command_tx.send(VoiceDtlsCommand::Disconnect);
            dtls_handle.abort();
            return;
        }
    };

    // Start audio playback
    if let Err(e) = mixer.start() {
        let _ = event_tx.send(VoiceEvent::AudioError(format!(
            "Failed to start playback: {}",
            e
        )));
        let _ = dtls_command_tx.send(VoiceDtlsCommand::Disconnect);
        dtls_handle.abort();
        return;
    }

    // Initialize codec
    let mut encoder = match VoiceEncoder::new(config.quality) {
        Ok(e) => e,
        Err(e) => {
            let _ = event_tx.send(VoiceEvent::AudioError(format!("Encoder error: {}", e)));
            let _ = dtls_command_tx.send(VoiceDtlsCommand::Disconnect);
            dtls_handle.abort();
            return;
        }
    };

    let mut decoder_pool = DecoderPool::new();
    let mut jitter_pool = JitterBufferPool::new();

    // Initialize audio processor for noise suppression, echo cancellation, and AGC
    let mut processor = match AudioProcessor::new(config.processor_settings) {
        Ok(p) => Some(p),
        Err(e) => {
            let _ = event_tx.send(VoiceEvent::AudioProcessorDisabled(e));
            None
        }
    };

    // State tracking
    let mut transmitting = false;
    let mut deafened = false;
    let mut muted_users: HashSet<String> = HashSet::new();
    let ptt_mode = config.ptt_mode;

    // Pre-allocated buffer for mixing render audio (AEC reference).
    // Reused each tick to avoid per-frame allocation.
    let mut render_mix = vec![0.0f32; VOICE_SAMPLES_PER_FRAME as usize];

    // Audio processing interval
    let mut audio_interval =
        tokio::time::interval(Duration::from_millis(AUDIO_PROCESS_INTERVAL_MS));

    loop {
        tokio::select! {
            // Process audio at regular intervals
            _ = audio_interval.tick() => {
                // Check for audio device errors
                if let Some(err) = capture.check_error() {
                    let _ = event_tx.send(VoiceEvent::AudioError(err));
                    let _ = dtls_command_tx.send(VoiceDtlsCommand::Disconnect);
                    break;
                }
                if let Some(err) = mixer.check_error() {
                    let _ = event_tx.send(VoiceEvent::AudioError(err));
                    let _ = dtls_command_tx.send(VoiceDtlsCommand::Disconnect);
                    break;
                }

                // Process jitter buffers and play audio FIRST, so the AEC
                // has an up-to-date render reference before we process the
                // capture frame. The AEC needs to know what's playing
                // through the speakers to subtract it from the mic signal.
                let mut has_render_audio = false;
                render_mix.fill(0.0);

                for (sender, buffer) in jitter_pool.iter_mut() {
                    // Skip muted users
                    if muted_users.contains(sender) {
                        continue;
                    }

                    // Check for packet loss and use PLC
                    if buffer.has_loss() {
                        if let Ok(samples) = decoder_pool.decode_lost(sender) {
                            for (i, &s) in samples.iter().enumerate() {
                                render_mix[i] += s;
                            }
                            has_render_audio = true;
                            mixer.queue_audio(sender, &samples);
                        }
                        // Pop to advance the jitter buffer
                        let _ = buffer.pop();
                    } else if let Some(samples) = buffer.pop() {
                        for (i, &s) in samples.iter().enumerate() {
                            render_mix[i] += s;
                        }
                        has_render_audio = true;
                        mixer.queue_audio(sender, &samples);
                    }
                }

                // Feed the mixed render output to the AEC so it can
                // subtract speaker echo from the microphone signal.
                // This MUST happen before process_capture_frame so the
                // adaptive filter has the current speaker reference.
                // Skip when deafened (no speaker output means no echo).
                if has_render_audio && !deafened
                    && let Some(ref proc) = processor
                {
                    let _ = proc.analyze_render_frame(&render_mix);
                }

                // Now capture and send audio with AEC properly primed
                if transmitting && capture.is_active()
                    && let Some(mut samples) = capture.take_frame()
                {
                    // Apply audio processing (noise suppression, AGC, AEC) to capture
                    if let Some(ref mut proc) = processor {
                        let _ = proc.process_capture_frame(&mut samples);

                        // In toggle mode, use VAD to gate transmission
                        // This prevents sending silence/noise when mic is "open"
                        if ptt_mode == PttMode::Toggle && !proc.has_voice(&samples) {
                            continue;
                        }
                    }

                    // Calculate mic level after processing so the VU meter
                    // reflects what others actually hear (post-AGC/NS)
                    let level = calculate_rms_level(&samples);
                    config.mic_level.store(level.to_bits(), Ordering::Relaxed);
                    if let Ok(encoded) = encoder.encode(&samples) {
                        let _ = dtls_command_tx.send(VoiceDtlsCommand::SendVoice(encoded));
                    }
                } else if transmitting {
                    // Still transmitting but no frame ready - clear level
                    config.mic_level.store(0f32.to_bits(), Ordering::Relaxed);
                }
            }

            // Handle DTLS events
            event = dtls_event_rx.recv() => {
                match event {
                    Some(VoiceDtlsEvent::VoiceReceived { sender, sequence, timestamp, payload }) => {
                        // Decode and buffer the audio
                        if let Ok(samples) = decoder_pool.decode(&sender, &payload) {
                            jitter_pool.push(&sender, sequence, timestamp, samples);
                        }
                    }
                    Some(VoiceDtlsEvent::SpeakingStarted { sender }) => {
                        let _ = event_tx.send(VoiceEvent::SpeakingStarted(sender));
                    }
                    Some(VoiceDtlsEvent::SpeakingStopped { sender }) => {
                        let _ = event_tx.send(VoiceEvent::SpeakingStopped(sender));
                    }
                    Some(VoiceDtlsEvent::Error(e)) => {
                        let _ = event_tx.send(VoiceEvent::Disconnected(Some(e)));
                        break;
                    }
                    Some(VoiceDtlsEvent::Disconnected) => {
                        let _ = event_tx.send(VoiceEvent::Disconnected(None));
                        break;
                    }
                    Some(VoiceDtlsEvent::Connected) => {
                        // Already handled above
                    }
                    None => {
                        let _ = event_tx.send(VoiceEvent::Disconnected(None));
                        break;
                    }
                }
            }

            // Handle commands
            cmd = command_rx.recv() => {
                match cmd {
                    Some(VoiceCommand::StartTransmitting) => {
                        if !transmitting {
                            transmitting = true;
                            // Hint to transient suppressor that PTT key was pressed
                            if let Some(ref proc) = processor {
                                proc.set_stream_key_pressed(true);
                            }
                            if let Err(e) = capture.start() {
                                let _ = event_tx.send(VoiceEvent::AudioError(format!("Capture error: {}", e)));
                            } else {
                                let _ = dtls_command_tx.send(VoiceDtlsCommand::SendSpeakingStarted);
                                let _ = event_tx.send(VoiceEvent::LocalSpeakingChanged(true));
                            }
                        }
                    }
                    Some(VoiceCommand::StopTransmitting) => {
                        if transmitting {
                            transmitting = false;
                            // Hint to transient suppressor that PTT key was released
                            if let Some(ref proc) = processor {
                                proc.set_stream_key_pressed(false);
                            }
                            capture.stop();
                            // Clear mic level when stopping
                            config.mic_level.store(0f32.to_bits(), Ordering::Relaxed);
                            let _ = dtls_command_tx.send(VoiceDtlsCommand::SendSpeakingStopped);
                            let _ = event_tx.send(VoiceEvent::LocalSpeakingChanged(false));
                        }
                    }
                    Some(VoiceCommand::MuteUser(nickname)) => {
                        let key = fold_name(&nickname);
                        muted_users.insert(key.clone());
                        mixer.mute_user(&nickname);
                        // Clear their jitter buffer
                        jitter_pool.remove(&nickname);
                        decoder_pool.remove(&nickname);
                    }
                    Some(VoiceCommand::UnmuteUser(nickname)) => {
                        let key = fold_name(&nickname);
                        muted_users.remove(&key);
                        mixer.unmute_user(&nickname);
                    }
                    Some(VoiceCommand::UserLeft(nickname)) => {
                        // Clean up decoder and jitter buffer for the user who left
                        jitter_pool.remove(&nickname);
                        decoder_pool.remove(&nickname);
                    }
                    Some(VoiceCommand::SetDeafened(is_deafened)) => {
                        deafened = is_deafened;
                        mixer.set_deafened(is_deafened);
                        // Hint to AEC/AGC: no speaker output when deafened, so no echo
                        if let Some(ref proc) = processor {
                            proc.set_output_will_be_muted(is_deafened);
                        }
                    }
                    Some(VoiceCommand::SetQuality(quality)) => {
                        if let Err(e) = encoder.set_quality(quality) {
                            let _ = event_tx.send(VoiceEvent::QualityChangeFailed(e));
                        }
                    }
                    Some(VoiceCommand::SetProcessorSettings(settings)) => {
                        if let Some(ref mut proc) = processor {
                            proc.update_settings(settings);
                        }
                    }
                    Some(VoiceCommand::Stop) | None => {
                        // Clean shutdown
                        if transmitting {
                            capture.stop();
                            let _ = dtls_command_tx.send(VoiceDtlsCommand::SendSpeakingStopped);
                        }
                        let _ = dtls_command_tx.send(VoiceDtlsCommand::Disconnect);
                        break;
                    }
                }
            }
        }
    }

    // Cleanup
    mixer.stop();
    dtls_handle.abort();
    let _ = event_tx.send(VoiceEvent::Disconnected(None));
}

// =============================================================================
// Voice Session Handle
// =============================================================================

/// Handle for controlling an active voice session
pub struct VoiceSessionHandle {
    /// Command sender
    command_tx: mpsc::UnboundedSender<VoiceCommand>,
    /// Join handle for the session thread
    /// Using std::thread instead of tokio::spawn because cpal's Stream is not Send
    handle: Option<JoinHandle<()>>,
}

impl VoiceSessionHandle {
    /// Start a new voice session
    ///
    /// Returns a handle for controlling the session and a receiver for events.
    ///
    /// Note: This spawns a dedicated OS thread because cpal's audio streams
    /// are not Send-safe and cannot be used across async task boundaries.
    pub fn start(config: VoiceSessionConfig) -> (Self, mpsc::UnboundedReceiver<VoiceEvent>) {
        let (event_tx, event_rx) = mpsc::unbounded_channel();
        let (command_tx, command_rx) = mpsc::unbounded_channel();

        // Spawn on a dedicated thread because cpal's Stream is not Send
        // The thread runs its own tokio runtime for async operations
        let handle = std::thread::spawn(move || {
            // Create a new tokio runtime for this thread
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect(ERR_VOICE_THREAD_TOKIO_RUNTIME);

            rt.block_on(run_voice_session(config, event_tx, command_rx));
        });

        (
            Self {
                command_tx,
                handle: Some(handle),
            },
            event_rx,
        )
    }

    /// Start transmitting (PTT pressed)
    pub fn start_transmitting(&self) {
        let _ = self.command_tx.send(VoiceCommand::StartTransmitting);
    }

    /// Stop transmitting (PTT released)
    pub fn stop_transmitting(&self) {
        let _ = self.command_tx.send(VoiceCommand::StopTransmitting);
    }

    /// Mute a user
    pub fn mute_user(&self, nickname: &str) {
        let _ = self
            .command_tx
            .send(VoiceCommand::MuteUser(nickname.to_string()));
    }

    /// Unmute a user
    pub fn unmute_user(&self, nickname: &str) {
        let _ = self
            .command_tx
            .send(VoiceCommand::UnmuteUser(nickname.to_string()));
    }

    /// Set deafened state (mute all incoming audio)
    pub fn set_deafened(&self, deafened: bool) {
        let _ = self.command_tx.send(VoiceCommand::SetDeafened(deafened));
    }

    /// Update voice quality (bitrate) dynamically
    ///
    /// Can be called while in a voice session to change quality without
    /// needing to leave and rejoin.
    pub fn set_quality(&self, quality: VoiceQuality) {
        let _ = self.command_tx.send(VoiceCommand::SetQuality(quality));
    }

    /// Update audio processor settings dynamically
    ///
    /// Can be called while in a voice session to toggle noise suppression,
    /// echo cancellation, or AGC without needing to leave and rejoin.
    pub fn set_processor_settings(&self, settings: AudioProcessorSettings) {
        let _ = self
            .command_tx
            .send(VoiceCommand::SetProcessorSettings(settings));
    }

    /// Clean up resources for a user who left voice
    ///
    /// Removes the user's decoder and jitter buffer to free memory.
    pub fn user_left(&self, nickname: &str) {
        let _ = self
            .command_tx
            .send(VoiceCommand::UserLeft(nickname.to_string()));
    }

    /// Stop the voice session
    ///
    /// Sends the stop command to the voice thread. The thread will clean up
    /// audio devices and DTLS connection on its own. We don't wait for it
    /// to avoid blocking the UI if audio drivers are unresponsive.
    pub fn stop(&mut self) {
        let _ = self.command_tx.send(VoiceCommand::Stop);
        self.handle.take(); // Release handle without blocking
    }
}

impl Drop for VoiceSessionHandle {
    fn drop(&mut self) {
        // Ensure the voice thread is stopped when the handle is dropped
        // This prevents orphaned threads if stop() wasn't called explicitly
        if self.handle.is_some() {
            self.stop();
        }
    }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_voice_event_variants() {
        // Verify enum variants compile
        let _ = VoiceEvent::Connected;
        let _ = VoiceEvent::ConnectionFailed("test".to_string());
        let _ = VoiceEvent::Disconnected(Some("test".to_string()));
        let _ = VoiceEvent::SpeakingStarted("Alice".to_string());
        let _ = VoiceEvent::SpeakingStopped("Alice".to_string());
        let _ = VoiceEvent::AudioError("test".to_string());
        let _ = VoiceEvent::LocalSpeakingChanged(true);
        let _ = VoiceEvent::AudioProcessorDisabled("test".to_string());
        let _ = VoiceEvent::QualityChangeFailed("test".to_string());
    }

    #[test]
    fn test_voice_command_variants() {
        // Verify enum variants compile
        let _ = VoiceCommand::StartTransmitting;
        let _ = VoiceCommand::StopTransmitting;
        let _ = VoiceCommand::MuteUser("Alice".to_string());
        let _ = VoiceCommand::UnmuteUser("Alice".to_string());
        let _ = VoiceCommand::UserLeft("Alice".to_string());
        let _ = VoiceCommand::Stop;
    }
}
