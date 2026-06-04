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
use tokio::time::{self, MissedTickBehavior};
use uuid::Uuid;

use nexus_common::names::fold_name;
use nexus_common::voice::{VOICE_SAMPLES_PER_FRAME, VoiceQuality};

use crate::config::audio::PttMode;
use crate::constants::ERR_VOICE_THREAD_TOKIO_RUNTIME;

use super::audio::{AudioCapture, AudioMixer, soft_clip};
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
    /// Nicknames currently in the voice session when the DTLS client starts
    pub participants: Vec<String>,
    /// Whether the local user is allowed to capture and transmit microphone audio.
    pub can_transmit: bool,
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

fn soft_clip_render_reference(frame: &mut [f32]) {
    for sample in frame {
        *sample = soft_clip(*sample);
    }
}

fn initialize_transmit_resources(
    input_device: &str,
    quality: VoiceQuality,
    processor_settings: AudioProcessorSettings,
    event_tx: &mpsc::UnboundedSender<VoiceEvent>,
) -> Result<(AudioCapture, VoiceEncoder, Option<AudioProcessor>), String> {
    let capture =
        AudioCapture::new(input_device).map_err(|e| format!("Input device error: {}", e))?;
    let encoder = VoiceEncoder::new(quality).map_err(|e| format!("Encoder error: {}", e))?;
    let processor = match AudioProcessor::new(processor_settings) {
        Ok(processor) => Some(processor),
        Err(e) => {
            let _ = event_tx.send(VoiceEvent::AudioProcessorDisabled(e));
            None
        }
    };

    Ok((capture, encoder, processor))
}

#[derive(Debug)]
struct VoiceParticipantState {
    known_participants: HashSet<String>,
    speaking_users: HashSet<String>,
    muted_users: HashSet<String>,
}

impl VoiceParticipantState {
    fn new(participants: Vec<String>) -> Self {
        Self {
            known_participants: participants.into_iter().map(|n| fold_name(&n)).collect(),
            speaking_users: HashSet::new(),
            muted_users: HashSet::new(),
        }
    }

    fn is_known(&self, nickname: &str) -> bool {
        self.known_participants.contains(&fold_name(nickname))
    }

    fn is_known_key(&self, key: &str) -> bool {
        self.known_participants.contains(key)
    }

    fn is_muted_key(&self, key: &str) -> bool {
        self.muted_users.contains(key)
    }

    fn add_user(&mut self, nickname: &str) {
        self.known_participants.insert(fold_name(nickname));
    }

    fn remove_user(&mut self, nickname: &str) {
        let key = fold_name(nickname);
        self.known_participants.remove(&key);
        self.muted_users.remove(&key);
    }

    fn set_muted(&mut self, nickname: &str, muted: bool) -> bool {
        let key = fold_name(nickname);
        let is_known = self.known_participants.contains(&key);
        if muted && is_known {
            self.muted_users.insert(key);
        } else {
            self.muted_users.remove(&key);
        }
        is_known
    }

    // Speaking state is cleared by the command handler so it can emit UI events.
    fn rename_user(&mut self, old: &str, new: &str, muted: bool) -> bool {
        let old_key = fold_name(old);
        let new_key = fold_name(new);
        let old_was_known = if old_key == new_key {
            self.known_participants.contains(&old_key)
        } else {
            self.known_participants.remove(&old_key)
        };

        if old_was_known {
            self.known_participants.insert(new_key.clone());
        }

        // Only inherit the old mute when the old participant was actually known;
        // duplicate rename signals must not resurrect stale old-name mutes.
        let should_mute_new = muted
            || self.muted_users.contains(&new_key)
            || (old_was_known && self.muted_users.contains(&old_key));
        let new_is_known = self.known_participants.contains(&new_key);
        self.muted_users.remove(&old_key);
        if should_mute_new && new_is_known {
            self.muted_users.insert(new_key);
        } else {
            self.muted_users.remove(&new_key);
        }

        old_was_known
    }

    fn mark_speaking(&mut self, nickname: &str) -> bool {
        self.speaking_users.insert(fold_name(nickname))
    }

    fn clear_speaking(&mut self, nickname: &str) -> bool {
        self.speaking_users.remove(&fold_name(nickname))
    }
}

fn emit_speaking_started(
    state: &mut VoiceParticipantState,
    nickname: &str,
    event_tx: &mpsc::UnboundedSender<VoiceEvent>,
) {
    if state.mark_speaking(nickname) {
        let _ = event_tx.send(VoiceEvent::SpeakingStarted(nickname.to_string()));
    }
}

fn emit_speaking_stopped(
    state: &mut VoiceParticipantState,
    nickname: &str,
    event_tx: &mpsc::UnboundedSender<VoiceEvent>,
) {
    if state.clear_speaking(nickname) {
        let _ = event_tx.send(VoiceEvent::SpeakingStopped(nickname.to_string()));
    }
}

fn unknown_voice_sender_keys<'a, I>(participants: &VoiceParticipantState, senders: I) -> Vec<String>
where
    I: IntoIterator<Item = &'a String>,
{
    senders
        .into_iter()
        .filter(|sender| !participants.is_known_key(sender))
        .cloned()
        .collect()
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
    /// Enabling transmit during a live listen-only session failed.
    TransmitEnableFailed(String),
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
    /// Enable capture/encoding after voice_talk is granted during a live session.
    EnableTransmit,
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
    /// Track a user who joined the active voice session
    UserJoined(String),
    /// Clean up resources for a user who left voice
    UserLeft(String),
    /// Re-key a user after a nickname rename
    UserRenamed {
        old: String,
        new: String,
        muted: bool,
    },
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

    let mut decoder_pool = DecoderPool::new();
    let mut jitter_pool = JitterBufferPool::new();

    let mut current_quality = config.quality;
    let mut current_processor_settings = config.processor_settings;
    let mut transmit_allowed = config.can_transmit;

    let (mut capture, mut encoder, mut processor) = if transmit_allowed {
        match initialize_transmit_resources(
            &config.input_device,
            current_quality,
            current_processor_settings,
            &event_tx,
        ) {
            Ok((capture, encoder, processor)) => (Some(capture), Some(encoder), processor),
            Err(error) => {
                let _ = event_tx.send(VoiceEvent::AudioError(error));
                let _ = dtls_command_tx.send(VoiceDtlsCommand::Disconnect);
                dtls_handle.abort();
                return;
            }
        }
    } else {
        (None, None, None)
    };

    // State tracking
    let mut transmitting = false;
    let mut deafened = false;
    let mut participants = VoiceParticipantState::new(config.participants);
    let ptt_mode = config.ptt_mode;

    // Pre-allocated buffer for mixing render audio (AEC reference).
    // Reused each tick to avoid per-frame allocation.
    let mut render_mix = vec![0.0f32; VOICE_SAMPLES_PER_FRAME as usize];

    // Audio processing interval
    let mut audio_interval = time::interval(Duration::from_millis(AUDIO_PROCESS_INTERVAL_MS));
    audio_interval.set_missed_tick_behavior(MissedTickBehavior::Skip);

    loop {
        tokio::select! {
            // Process audio at regular intervals
            _ = audio_interval.tick() => {
                // Check for audio device errors
                if let Some(capture) = capture.as_ref()
                    && let Some(err) = capture.check_error()
                {
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
                let stale_senders =
                    unknown_voice_sender_keys(&participants, jitter_pool.iter_mut().map(|(s, _)| s));

                for (sender, buffer) in jitter_pool.iter_mut() {
                    if !participants.is_known_key(sender) {
                        continue;
                    }

                    if participants.is_muted_key(sender) {
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
                for sender in stale_senders {
                    jitter_pool.remove(&sender);
                    decoder_pool.remove(&sender);
                    mixer.remove_user_state(&sender);
                }

                // Feed the mixed render output to the AEC so it can
                // subtract speaker echo from the microphone signal.
                // This MUST happen before process_capture_frame so the
                // adaptive filter has the current speaker reference.
                // Skip when deafened (no speaker output means no echo).
                if has_render_audio && !deafened
                    && let Some(ref proc) = processor
                {
                    soft_clip_render_reference(&mut render_mix);
                    let _ = proc.analyze_render_frame(&render_mix);
                }

                // Now capture and send audio with AEC properly primed
                if transmitting
                    && let Some(capture) = capture.as_ref()
                    && capture.is_active()
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
                    if let Some(encoder) = encoder.as_mut()
                        && let Ok(encoded) = encoder.encode(&samples)
                    {
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
                        if !participants.is_known(&sender) {
                            decoder_pool.remove(&sender);
                            jitter_pool.remove(&sender);
                            continue;
                        }
                        // Decode and buffer the audio
                        if let Ok(samples) = decoder_pool.decode(&sender, &payload)
                            && jitter_pool.push(&sender, sequence, timestamp, samples)
                        {
                            emit_speaking_started(&mut participants, &sender, &event_tx);
                        }
                    }
                    Some(VoiceDtlsEvent::SpeakingStarted { sender }) => {
                        if participants.is_known(&sender) {
                            emit_speaking_started(&mut participants, &sender, &event_tx);
                        }
                    }
                    Some(VoiceDtlsEvent::SpeakingStopped { sender }) => {
                        if participants.is_known(&sender) {
                            emit_speaking_stopped(&mut participants, &sender, &event_tx);
                        }
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
                    Some(VoiceCommand::EnableTransmit) => {
                        if transmit_allowed {
                            continue;
                        }

                        match initialize_transmit_resources(
                            &config.input_device,
                            current_quality,
                            current_processor_settings,
                            &event_tx,
                        ) {
                            Ok((new_capture, new_encoder, new_processor)) => {
                                capture = Some(new_capture);
                                encoder = Some(new_encoder);
                                processor = new_processor;
                                transmit_allowed = true;
                            }
                            Err(error) => {
                                let _ = event_tx.send(VoiceEvent::TransmitEnableFailed(error));
                            }
                        }
                    }
                    Some(VoiceCommand::StartTransmitting) => {
                        if transmit_allowed && !transmitting {
                            transmitting = true;
                            // Hint to transient suppressor that PTT key was pressed
                            if let Some(ref proc) = processor {
                                proc.set_stream_key_pressed(true);
                            }
                            let Some(capture) = capture.as_ref() else {
                                continue;
                            };
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
                            if let Some(capture) = capture.as_ref() {
                                capture.stop();
                            }
                            // Clear mic level when stopping
                            config.mic_level.store(0f32.to_bits(), Ordering::Relaxed);
                            let _ = dtls_command_tx.send(VoiceDtlsCommand::SendSpeakingStopped);
                            let _ = event_tx.send(VoiceEvent::LocalSpeakingChanged(false));
                        }
                    }
                    Some(VoiceCommand::MuteUser(nickname)) => {
                        // Stop any already queued or codec-buffered audio immediately.
                        if participants.set_muted(&nickname, true) {
                            mixer.mute_user_and_clear(&nickname);
                        } else {
                            mixer.remove_user_state(&nickname);
                        }
                        jitter_pool.remove(&nickname);
                        decoder_pool.remove(&nickname);
                    }
                    Some(VoiceCommand::UnmuteUser(nickname)) => {
                        participants.set_muted(&nickname, false);
                        mixer.unmute_user(&nickname);
                    }
                    Some(VoiceCommand::UserJoined(nickname)) => {
                        participants.add_user(&nickname);
                    }
                    Some(VoiceCommand::UserLeft(nickname)) => {
                        participants.remove_user(&nickname);
                        emit_speaking_stopped(&mut participants, &nickname, &event_tx);
                        // Clean up decoder, jitter, and queued mixer buffers for the user who left.
                        mixer.remove_user_state(&nickname);
                        jitter_pool.remove(&nickname);
                        decoder_pool.remove(&nickname);
                    }
                    Some(VoiceCommand::UserRenamed { old, new, muted }) => {
                        let old_was_known = participants.rename_user(&old, &new, muted);
                        emit_speaking_stopped(&mut participants, &old, &event_tx);
                        if old_was_known && fold_name(&old) != fold_name(&new) {
                            emit_speaking_stopped(&mut participants, &new, &event_tx);
                        }

                        if old_was_known {
                            decoder_pool.rename_user(&old, &new);
                            jitter_pool.rename_user(&old, &new);
                            mixer.rename_user(&old, &new, muted);
                        } else {
                            decoder_pool.remove(&old);
                            jitter_pool.remove(&old);
                            mixer.remove_user_state(&old);
                            let new_key = fold_name(&new);
                            if participants.is_known_key(&new_key)
                                && participants.is_muted_key(&new_key)
                            {
                                mixer.mute_user(&new);
                            } else {
                                mixer.unmute_user(&new);
                            }
                        }
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
                        current_quality = quality;
                        if let Some(encoder) = encoder.as_mut()
                            && let Err(e) = encoder.set_quality(quality)
                        {
                            let _ = event_tx.send(VoiceEvent::QualityChangeFailed(e));
                        }
                    }
                    Some(VoiceCommand::SetProcessorSettings(settings)) => {
                        current_processor_settings = settings;
                        if let Some(ref mut proc) = processor {
                            proc.update_settings(settings);
                        }
                    }
                    Some(VoiceCommand::Stop) | None => {
                        // Clean shutdown
                        if transmitting {
                            if let Some(capture) = capture.as_ref() {
                                capture.stop();
                            }
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
    #[cfg(test)]
    pub(crate) fn test_handle() -> (Self, mpsc::UnboundedReceiver<VoiceCommand>) {
        let (command_tx, command_rx) = mpsc::unbounded_channel();
        (
            Self {
                command_tx,
                handle: None,
            },
            command_rx,
        )
    }

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

    /// Enable capture/encoding for a live session after voice_talk is granted.
    pub fn enable_transmit(&self) {
        let _ = self.command_tx.send(VoiceCommand::EnableTransmit);
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

    /// Track a user who joined the active voice session.
    pub fn user_joined(&self, nickname: &str) {
        let _ = self
            .command_tx
            .send(VoiceCommand::UserJoined(nickname.to_string()));
    }

    /// Clean up resources for a user who left voice
    ///
    /// Removes the user's decoder and jitter buffer to free memory.
    pub fn user_left(&self, nickname: &str) {
        let _ = self
            .command_tx
            .send(VoiceCommand::UserLeft(nickname.to_string()));
    }

    /// Re-key voice state after a nickname rename.
    pub fn user_renamed(&self, old: &str, new: &str, muted: bool) {
        let _ = self.command_tx.send(VoiceCommand::UserRenamed {
            old: old.to_string(),
            new: new.to_string(),
            muted,
        });
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
        let _ = VoiceEvent::TransmitEnableFailed("test".to_string());
        let _ = VoiceEvent::LocalSpeakingChanged(true);
        let _ = VoiceEvent::AudioProcessorDisabled("test".to_string());
        let _ = VoiceEvent::QualityChangeFailed("test".to_string());
    }

    #[test]
    fn test_voice_command_variants() {
        // Verify enum variants compile
        let _ = VoiceCommand::EnableTransmit;
        let _ = VoiceCommand::StartTransmitting;
        let _ = VoiceCommand::StopTransmitting;
        let _ = VoiceCommand::MuteUser("Alice".to_string());
        let _ = VoiceCommand::UnmuteUser("Alice".to_string());
        let _ = VoiceCommand::UserJoined("Alice".to_string());
        let _ = VoiceCommand::UserLeft("Alice".to_string());
        let _ = VoiceCommand::UserRenamed {
            old: "Alice".to_string(),
            new: "Alicia".to_string(),
            muted: false,
        };
        let _ = VoiceCommand::Stop;
    }

    #[test]
    fn participant_state_gates_and_rekeys_voice_rename() {
        let mut state = VoiceParticipantState::new(vec!["Alice".to_string()]);

        assert!(state.is_known("alice"));
        assert!(!state.is_known("Bob"));

        assert!(state.mark_speaking("Alice"));
        state.set_muted("Alice", true);

        assert!(state.rename_user("Alice", "Alicia", true));

        assert!(!state.is_known("Alice"));
        assert!(state.is_known("Alicia"));
        assert!(state.is_muted_key(&fold_name("Alicia")));
        assert!(!state.is_muted_key(&fold_name("Alice")));
        assert!(state.clear_speaking("Alice"));
        assert!(!state.clear_speaking("Alicia"));
    }

    #[test]
    fn unknown_voice_sender_keys_selects_stale_buffers() {
        let state = VoiceParticipantState::new(vec!["Alice".to_string(), "Bob".to_string()]);
        let senders = [
            fold_name("Alice"),
            fold_name("Bob"),
            fold_name("Charlie"),
            fold_name("Alicia"),
        ];

        let stale = unknown_voice_sender_keys(&state, senders.iter());

        assert_eq!(stale, vec![fold_name("Charlie"), fold_name("Alicia")]);
    }

    #[test]
    fn render_reference_uses_playback_soft_clip_curve() {
        let mut frame = vec![0.0, 0.5, 2.0, -2.0];
        let expected: Vec<f32> = frame.iter().copied().map(soft_clip).collect();

        soft_clip_render_reference(&mut frame);

        for (actual, expected) in frame.iter().zip(expected) {
            assert!((actual - expected).abs() < f32::EPSILON);
        }
        assert!((frame[1] - 0.5).abs() < 0.1);
        assert!(frame[2] < 2.0);
        assert!(frame[2] > 0.0);
        assert!(frame[3] > -2.0);
        assert!(frame[3] < 0.0);
    }

    #[test]
    fn participant_state_rename_collision_keeps_new_known_and_unions_mute() {
        let mut state = VoiceParticipantState::new(vec!["Alice".to_string(), "Alicia".to_string()]);
        state.set_muted("Alice", true);

        assert!(state.rename_user("Alice", "Alicia", true));

        assert!(!state.is_known("Alice"));
        assert!(state.is_known("Alicia"));
        assert_eq!(state.known_participants.len(), 1);
        assert!(state.is_muted_key(&fold_name("Alicia")));
    }

    #[test]
    fn participant_state_rename_preserves_existing_new_mute() {
        let mut state = VoiceParticipantState::new(vec!["Alicia".to_string()]);
        state.set_muted("Alicia", true);

        assert!(!state.rename_user("Alice", "Alicia", false));

        assert!(state.is_muted_key(&fold_name("Alicia")));
        assert!(!state.is_muted_key(&fold_name("Alice")));
    }

    #[test]
    fn participant_state_remove_user_clears_stale_mute() {
        let mut state = VoiceParticipantState::new(vec!["Alice".to_string()]);
        state.set_muted("Alice", true);

        state.remove_user("Alice");

        assert!(!state.is_known("Alice"));
        assert!(!state.is_muted_key(&fold_name("Alice")));
    }

    #[test]
    fn participant_state_mute_unknown_user_does_not_persist() {
        let mut state = VoiceParticipantState::new(vec!["Alice".to_string()]);
        state.muted_users.insert(fold_name("Bob"));

        assert!(!state.set_muted("Bob", true));

        assert!(!state.is_muted_key(&fold_name("Bob")));
    }

    #[test]
    fn duplicate_rename_does_not_clear_rebuilt_new_speaking() {
        let (event_tx, mut event_rx) = mpsc::unbounded_channel();
        let mut state = VoiceParticipantState::new(vec!["Alicia".to_string()]);
        assert!(state.mark_speaking("Alicia"));

        let old_was_known = state.rename_user("Alice", "Alicia", false);
        emit_speaking_stopped(&mut state, "Alice", &event_tx);
        if old_was_known && fold_name("Alice") != fold_name("Alicia") {
            emit_speaking_stopped(&mut state, "Alicia", &event_tx);
        }

        assert!(!old_was_known);
        assert!(state.speaking_users.contains(&fold_name("Alicia")));
        assert!(event_rx.try_recv().is_err());
    }
}
