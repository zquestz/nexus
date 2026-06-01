//! Voice chat message handlers
//!
//! Handles server messages for voice chat:
//! - VoiceJoinResponse - Response to VoiceJoin request
//! - VoiceLeaveResponse - Response to VoiceLeave request
//! - VoiceUserJoined - Notification when another user joins voice
//! - VoiceUserLeft - Notification when another user leaves voice

use std::net::ToSocketAddrs;

use iced::Task;
use nexus_common::address::resolve_host_for_connection;
use nexus_common::names::fold_name;
use uuid::Uuid;

use crate::NexusApp;
use crate::config::events::EventType;
use crate::events::{EventContext, emit_event};
use crate::i18n::{t, t_args};
use crate::network::DNS_LOOKUP_TIMEOUT;
use crate::types::{ChatMessage, Message, VoiceState};
use crate::voice::manager::{VoiceSessionConfig, VoiceSessionHandle};

use crate::voice::subscription::register_voice_receiver_sync;

async fn resolve_voice_socket_addr(
    address: String,
    port: u16,
) -> Result<Option<std::net::SocketAddr>, String> {
    let resolved = resolve_host_for_connection(&address).map_err(|e| e.to_string())?;
    let lookup = tokio::task::spawn_blocking(move || {
        (resolved.as_str(), port)
            .to_socket_addrs()
            .map(|iter| iter.collect::<Vec<_>>())
    });

    let addrs = tokio::time::timeout(DNS_LOOKUP_TIMEOUT, lookup)
        .await
        .map_err(|_| t_args("err-dns-lookup-timeout", &[("address", &address)]))?
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())?;

    Ok(addrs.into_iter().next())
}

impl NexusApp {
    /// Handle response to VoiceJoin request
    ///
    /// On success: Create voice session with token and participants
    /// On error: Show error in active tab
    pub fn handle_voice_join_response(
        &mut self,
        connection_id: usize,
        success: bool,
        token: Option<Uuid>,
        target: Option<String>,
        participants: Option<Vec<String>>,
        error: Option<String>,
    ) -> Task<Message> {
        if !success {
            // Clear the placeholder voice session on failure
            if let Some(conn) = self.connections.get_mut(&connection_id) {
                conn.voice_session = None;
            }

            let error_msg = error.unwrap_or_else(|| t("err-unknown"));
            return self.add_active_tab_message(
                connection_id,
                ChatMessage::error(t_args("err-voice-join", &[("error", &error_msg)])),
            );
        }

        let Some(token) = token else {
            // Clear the placeholder voice session - no token means failure
            if let Some(conn) = self.connections.get_mut(&connection_id) {
                conn.voice_session = None;
            }
            return self.add_active_tab_message(
                connection_id,
                ChatMessage::error(t("err-voice-no-token")),
            );
        };

        // Use target from server, fall back to placeholder target if not provided
        let target = match target {
            Some(t) => t,
            None => {
                let Some(conn) = self.connections.get(&connection_id) else {
                    return Task::none();
                };
                conn.voice_session
                    .as_ref()
                    .map(|s| s.target.clone())
                    .unwrap_or_default()
            }
        };

        // Emit event for our own join (is_from_self suppresses notification but allows sound)
        emit_event(
            self,
            EventType::VoiceJoined,
            EventContext::new()
                .with_connection_id(connection_id)
                .with_channel(&target)
                .with_is_from_self(true),
        );

        let Some(conn) = self.connections.get_mut(&connection_id) else {
            return Task::none();
        };

        // Create the voice session
        let participants = participants.unwrap_or_default();
        conn.voice_session = Some(VoiceState::new_with_token(
            target.clone(),
            participants.clone(),
            token,
        ));

        // Track that this connection has the active voice session
        self.active_voice_connection = Some(connection_id);

        let server_address = conn.connection_info.address.clone();
        let server_port = conn.connection_info.port;

        Task::perform(
            resolve_voice_socket_addr(server_address, server_port),
            move |result| Message::VoiceAddressResolved {
                connection_id,
                target,
                participants,
                token,
                result,
            },
        )
    }

    pub fn handle_voice_address_resolved(
        &mut self,
        connection_id: usize,
        target: String,
        participants: Vec<String>,
        token: Uuid,
        result: Result<Option<std::net::SocketAddr>, String>,
    ) -> Task<Message> {
        let Some(conn) = self.connections.get(&connection_id) else {
            return Task::none();
        };

        if self.active_voice_connection != Some(connection_id)
            || conn
                .voice_session
                .as_ref()
                .is_none_or(|session| session.target != target || session.token != Some(token))
        {
            return Task::none();
        }

        let socket_addr = match result {
            Ok(Some(addr)) => addr,
            Ok(None) => {
                return self.add_active_tab_message(
                    connection_id,
                    ChatMessage::error(t("err-voice-resolve-address")),
                );
            }
            Err(e) => {
                return self.add_active_tab_message(
                    connection_id,
                    ChatMessage::error(t_args("err-voice-resolve", &[("error", &e)])),
                );
            }
        };

        // Start voice session with audio settings
        let (handle, event_rx) = VoiceSessionHandle::start(VoiceSessionConfig {
            server_addr: socket_addr,
            token,
            participants,
            input_device: self.config.settings.audio.input_device.clone(),
            output_device: self.config.settings.audio.output_device.clone(),
            quality: self.config.settings.audio.voice_quality,
            processor_settings: crate::voice::processor::AudioProcessorSettings {
                noise_suppression: self.config.settings.audio.noise_suppression,
                noise_suppression_level: self.config.settings.audio.noise_suppression_level,
                echo_cancellation: self.config.settings.audio.echo_cancellation,
                agc: self.config.settings.audio.agc,
                transient_suppression: self.config.settings.audio.transient_suppression,
                mic_boost: self.config.settings.audio.mic_boost,
            },
            ptt_mode: self.config.settings.audio.ptt_mode,
            mic_level: self.mic_level.clone(),
        });

        // Store the handle
        self.voice_session_handle = Some(handle);

        // Register the event receiver in the global registry for the subscription
        // Must be synchronous to avoid race with subscription starting
        register_voice_receiver_sync(connection_id, event_rx);

        // Lazily create PTT manager on first voice join (not at startup).
        // This ensures the native event loop is active when the hotkey system initializes.
        if self.ptt_manager.is_none() {
            match crate::voice::ptt::PttManager::new() {
                Ok(ptt) => self.ptt_manager = Some(ptt),
                Err(e) => {
                    return self.add_active_tab_message(
                        connection_id,
                        ChatMessage::error(t_args("err-voice-ptt-failed", &[("error", &e)])),
                    );
                }
            }
        }

        // Register PTT hotkey and enable it for voice
        if let Some(ref mut ptt) = self.ptt_manager {
            // Set mode from settings
            ptt.set_mode(self.config.settings.audio.ptt_mode);

            // Register the hotkey and show error if it fails
            if let Err(e) = ptt.register_hotkey(&self.config.settings.audio.ptt_key) {
                // PTT won't work, but voice chat still functions
                ptt.set_in_voice(true);
                return self.add_active_tab_message(
                    connection_id,
                    ChatMessage::error(t_args("err-voice-ptt-failed", &[("error", &e)])),
                );
            }

            // Enable PTT for voice
            ptt.set_in_voice(true);
        }

        // Update tray icon state (Windows/Linux only)
        #[cfg(not(target_os = "macos"))]
        self.update_tray_state();

        // Voice bar appearing provides visual feedback - no console message needed
        Task::none()
    }

    /// Handle response to VoiceLeave request
    ///
    /// On success: Clear voice session
    /// On error: Show error in console (but still clear local state)
    pub fn handle_voice_leave_response(
        &mut self,
        connection_id: usize,
        success: bool,
        error: Option<String>,
    ) -> Task<Message> {
        // Get the target before cleanup (for event emission)
        let target = self
            .connections
            .get(&connection_id)
            .and_then(|conn| conn.voice_session.as_ref())
            .map(|session| session.target.clone());

        // Clear local voice state regardless of success
        // (if server says we're not in voice, we should clear our state too)
        self.cleanup_voice_session(connection_id);

        if !success {
            let error_msg = error.unwrap_or_else(|| t("err-unknown"));
            return self.add_active_tab_message(
                connection_id,
                ChatMessage::error(t_args("err-voice-leave", &[("error", &error_msg)])),
            );
        }

        // Emit event for our own leave (is_from_self suppresses notification but allows sound)
        if let Some(target) = target {
            emit_event(
                self,
                EventType::VoiceLeft,
                EventContext::new()
                    .with_connection_id(connection_id)
                    .with_channel(&target)
                    .with_is_from_self(true),
            );
        }

        // Voice bar disappearing provides visual feedback - no console message needed
        Task::none()
    }

    /// Handle VoiceUserJoined - notification when another user joins voice
    ///
    /// Adds the user to our local participants list if we're in the same voice session.
    /// Also tracks voice users per channel for UI indicators (even when not in voice).
    /// Shows notification in the target tab (channel or user message) if join/leave events are enabled.
    pub fn handle_voice_user_joined(
        &mut self,
        connection_id: usize,
        nickname: String,
        target: String,
    ) -> Task<Message> {
        let mut joined_active_voice_session = false;

        // Scope conn borrow so it's dropped before emit_event
        {
            let Some(conn) = self.connections.get_mut(&connection_id) else {
                return Task::none();
            };

            // Track voiced nicknames per channel (even when we're not in voice)
            // Use lowercase for consistency with ChatJoinResponse population
            if target.starts_with('#') {
                conn.channel_voiced
                    .entry(fold_name(&target))
                    .or_default()
                    .insert(fold_name(&nickname));
            }

            // Update voice session participants if we're in the same session
            if let Some(ref mut session) = conn.voice_session
                && fold_name(&session.target) == fold_name(&target)
            {
                session.add_participant(nickname.clone());
                joined_active_voice_session = true;
            }
        }

        if joined_active_voice_session
            && self.active_voice_connection == Some(connection_id)
            && let Some(ref handle) = self.voice_session_handle
        {
            handle.user_joined(&nickname);
        }

        // Emit VoiceJoined event for notifications
        emit_event(
            self,
            EventType::VoiceJoined,
            EventContext::new()
                .with_connection_id(connection_id)
                .with_username(&nickname)
                .with_channel(&target),
        );

        // Show notification in target tab if events are enabled
        if self.config.settings.show_join_leave_events {
            let message =
                ChatMessage::system(t_args("msg-voice-user-joined", &[("nickname", &nickname)]));

            // Route to channel or user message tab based on target
            if target.starts_with('#') {
                return self.add_channel_message(connection_id, &target, message);
            } else {
                return self.add_user_message(connection_id, &target, message);
            }
        }

        Task::none()
    }

    /// Handle VoiceUserLeft - notification when a user leaves voice
    ///
    /// If the leaving user is us (kicked due to permission revocation), clears our voice session.
    /// Otherwise, removes the user from our local participants list.
    /// Also updates per-channel voice tracking for UI indicators.
    /// Shows notification in the target tab (channel or user message) if join/leave events are enabled.
    pub fn handle_voice_user_left(
        &mut self,
        connection_id: usize,
        nickname: String,
        target: String,
    ) -> Task<Message> {
        // Check if we're the one who left (kicked due to permission revocation)
        let is_self = self
            .connections
            .get(&connection_id)
            .map(|conn| fold_name(&conn.nickname) == fold_name(&nickname))
            .unwrap_or(false);

        if is_self {
            // We left voice - clean up fully
            self.cleanup_voice_session(connection_id);

            // Also remove ourselves from channel voice tracking
            if let Some(conn) = self.connections.get_mut(&connection_id)
                && target.starts_with('#')
                && let Some(voiced) = conn.channel_voiced.get_mut(&fold_name(&target))
            {
                voiced.remove(&fold_name(&nickname));
            }

            // Show notification in target tab
            let message = ChatMessage::system(t("msg-voice-you-left"));

            if target.starts_with('#') {
                return self.add_channel_message(connection_id, &target, message);
            } else {
                return self.add_user_message(connection_id, &target, message);
            }
        }

        let mut left_active_voice_session = false;

        // Scope conn borrow so it's dropped before emit_event
        {
            let Some(conn) = self.connections.get_mut(&connection_id) else {
                return Task::none();
            };

            // Remove from per-channel voiced tracking (use lowercase for consistency)
            if target.starts_with('#')
                && let Some(voiced) = conn.channel_voiced.get_mut(&fold_name(&target))
            {
                voiced.remove(&fold_name(&nickname));
            }

            // Update voice session participants if we're in the same session
            if let Some(ref mut session) = conn.voice_session
                && fold_name(&session.target) == fold_name(&target)
            {
                session.remove_participant(&nickname);
                left_active_voice_session = true;
            }
        }

        // Clean up audio state only when the leave applies to our active voice session.
        if left_active_voice_session
            && self.active_voice_connection == Some(connection_id)
            && let Some(ref handle) = self.voice_session_handle
        {
            handle.user_left(&nickname);
        }

        // Emit VoiceLeft event for notifications
        emit_event(
            self,
            EventType::VoiceLeft,
            EventContext::new()
                .with_connection_id(connection_id)
                .with_username(&nickname)
                .with_channel(&target),
        );

        // Show notification in target tab if events are enabled
        if self.config.settings.show_join_leave_events {
            let message =
                ChatMessage::system(t_args("msg-voice-user-left", &[("nickname", &nickname)]));

            // Route to channel or user message tab based on target
            if target.starts_with('#') {
                return self.add_channel_message(connection_id, &target, message);
            } else {
                return self.add_user_message(connection_id, &target, message);
            }
        }

        Task::none()
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use nexus_common::validators::PasswordStrength;
    use tokio::sync::{Mutex, mpsc};

    use super::*;
    use crate::types::{ConnectionInfo, ServerConnection, ServerConnectionParams, VoiceState};
    use crate::voice::manager::{VoiceCommand, VoiceSessionHandle};

    fn test_connection(connection_id: usize, nickname: &str, target: &str) -> ServerConnection {
        let (tx, _rx) = mpsc::unbounded_channel();
        let mut conn = ServerConnection::new(ServerConnectionParams {
            bookmark_id: None,
            user_id: None,
            nickname: nickname.to_string(),
            connection_info: ConnectionInfo {
                server_name: String::new(),
                address: String::new(),
                port: 0,
                transfer_port: 0,
                certificate_fingerprint: String::new(),
                username: nickname.to_string(),
                password: String::new(),
                nickname: nickname.to_string(),
            },
            display_name: String::new(),
            connection_id,
            is_admin: false,
            permissions: Vec::new(),
            server_name: None,
            server_description: None,
            public_address: None,
            server_version: None,
            server_image: String::new(),
            cached_server_image: None,
            chat_burst_limit: None,
            chat_rate_limit: None,
            max_connections_per_ip: None,
            max_outbound_rate: None,
            max_transfers_per_ip: None,
            file_reindex_interval: None,
            persistent_channels: None,
            auto_join_channels: None,
            min_password_strength: PasswordStrength::Weak,
            log_level: None,
            scheduler_chunk_size: None,
            tx,
            shutdown_handle: Arc::new(Mutex::new(None)),
        });
        conn.voice_session = Some(VoiceState::new(
            target.to_string(),
            vec![nickname.to_string()],
        ));
        conn
    }

    #[test]
    fn voice_joined_left_only_notifies_active_connection_handle() {
        let mut app = NexusApp::default();
        app.config.settings.show_join_leave_events = false;
        app.connections
            .insert(1, test_connection(1, "me", "#general"));
        app.connections
            .insert(2, test_connection(2, "also-me", "#general"));
        app.active_voice_connection = Some(1);

        let (handle, mut commands) = VoiceSessionHandle::test_handle();
        app.voice_session_handle = Some(handle);

        let _ = app.handle_voice_user_joined(1, "Alice".to_string(), "#general".to_string());
        match commands.try_recv() {
            Ok(VoiceCommand::UserJoined(nickname)) => assert_eq!(nickname, "Alice"),
            other => panic!("expected active UserJoined command, got {other:?}"),
        }

        let _ = app.handle_voice_user_joined(2, "Bob".to_string(), "#general".to_string());
        assert!(commands.try_recv().is_err());

        let _ = app.handle_voice_user_left(1, "Alice".to_string(), "#general".to_string());
        match commands.try_recv() {
            Ok(VoiceCommand::UserLeft(nickname)) => assert_eq!(nickname, "Alice"),
            other => panic!("expected active UserLeft command, got {other:?}"),
        }

        let _ = app.handle_voice_user_left(2, "Bob".to_string(), "#general".to_string());
        assert!(commands.try_recv().is_err());
    }
}
