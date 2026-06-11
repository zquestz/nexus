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
use crate::views::constants::PERMISSION_VOICE_TALK;
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
            let should_report = if let Some(conn) = self.connections.get_mut(&connection_id) {
                match conn.voice_session.as_ref() {
                    Some(session) if session.token.is_none() => {
                        conn.voice_session = None;
                        true
                    }
                    _ => false,
                }
            } else {
                false
            };

            if !should_report {
                return Task::none();
            }

            let error_msg = error.unwrap_or_else(|| t("err-unknown"));
            return self.add_active_tab_message(
                connection_id,
                ChatMessage::error(t_args("err-voice-join", &[("error", &error_msg)])),
            );
        }

        let Some(token) = token else {
            let should_report = if let Some(conn) = self.connections.get_mut(&connection_id) {
                match conn.voice_session.as_ref() {
                    Some(session) if session.token.is_none() => {
                        conn.voice_session = None;
                        true
                    }
                    _ => false,
                }
            } else {
                false
            };

            if !should_report {
                return Task::none();
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

        let participants = participants.unwrap_or_default();

        let (server_address, server_port) = {
            let Some(conn) = self.connections.get_mut(&connection_id) else {
                return Task::none();
            };

            if conn
                .voice_session
                .as_ref()
                .is_some_and(|session| session.leave_sent)
            {
                return Task::none();
            }

            let can_transmit = conn.has_permission(PERMISSION_VOICE_TALK);
            if !target.starts_with('#') {
                let self_nickname = fold_name(&conn.nickname);
                let target_nickname = fold_name(&target);
                let peer_is_present = participants.iter().any(|participant| {
                    let participant = fold_name(participant);
                    participant != self_nickname && participant == target_nickname
                });
                if peer_is_present {
                    conn.user_message_voiced.insert(target_nickname);
                } else {
                    conn.user_message_voiced.remove(&target_nickname);
                }
            }
            conn.voice_session = Some(VoiceState::new_with_token(
                target.clone(),
                participants,
                token,
                can_transmit,
            ));

            (
                conn.connection_info.address.clone(),
                conn.connection_info.port,
            )
        };

        // Track that this connection has the active voice session
        self.active_voice_connection = Some(connection_id);

        // Emit event for our own join (is_from_self suppresses notification but allows sound)
        emit_event(
            self,
            EventType::VoiceJoined,
            EventContext::new()
                .with_connection_id(connection_id)
                .with_channel(&target)
                .with_is_from_self(true),
        );

        Task::perform(
            resolve_voice_socket_addr(server_address, server_port),
            move |result| Message::VoiceAddressResolved {
                connection_id,
                token,
                result,
            },
        )
    }

    pub fn handle_voice_address_resolved(
        &mut self,
        connection_id: usize,
        token: Uuid,
        result: Result<Option<std::net::SocketAddr>, String>,
    ) -> Task<Message> {
        let (participants, can_transmit) = {
            let Some(conn) = self.connections.get(&connection_id) else {
                return Task::none();
            };
            let Some(session) = conn.voice_session.as_ref() else {
                return Task::none();
            };

            if self.active_voice_connection != Some(connection_id)
                || session.token != Some(token)
                || session.leave_sent
            {
                return Task::none();
            }

            (session.participants.clone(), session.can_transmit)
        };

        let socket_addr = match result {
            Ok(Some(addr)) => addr,
            Ok(None) => {
                let _ = self.send_voice_leave_once(connection_id);
                self.cleanup_voice_session(connection_id);
                return self.add_active_tab_message(
                    connection_id,
                    ChatMessage::error(t("err-voice-resolve-address")),
                );
            }
            Err(e) => {
                let _ = self.send_voice_leave_once(connection_id);
                self.cleanup_voice_session(connection_id);
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
            can_transmit,
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

        if can_transmit && let Err(e) = self.enable_voice_transmit_controls() {
            // PTT won't work, but voice chat still functions.
            return self.add_active_tab_message(
                connection_id,
                ChatMessage::error(t_args("err-voice-ptt-failed", &[("error", &e)])),
            );
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
            } else {
                conn.user_message_voiced.insert(fold_name(&nickname));
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
            } else if !target.starts_with('#') {
                conn.user_message_voiced.remove(&fold_name(&nickname));
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

    use nexus_common::framing::MessageId;
    use nexus_common::protocol::ClientMessage;
    use nexus_common::validators::PasswordStrength;
    use tokio::sync::{Mutex, mpsc};

    use super::*;
    use crate::types::{ConnectionInfo, ServerConnection, ServerConnectionParams, VoiceState};
    use crate::voice::manager::{VoiceCommand, VoiceSessionHandle};

    fn test_connection(connection_id: usize, nickname: &str, target: &str) -> ServerConnection {
        test_connection_with_receiver(connection_id, nickname, target).0
    }

    fn test_connection_with_receiver(
        connection_id: usize,
        nickname: &str,
        target: &str,
    ) -> (
        ServerConnection,
        mpsc::UnboundedReceiver<(MessageId, ClientMessage)>,
    ) {
        let (tx, rx) = mpsc::unbounded_channel();
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
            features: Vec::new(),
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
        (conn, rx)
    }

    fn expect_voice_leave(rx: &mut mpsc::UnboundedReceiver<(MessageId, ClientMessage)>) {
        match rx.try_recv() {
            Ok((_, ClientMessage::VoiceLeave)) => {}
            other => panic!("expected VoiceLeave, got {other:?}"),
        }
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

    #[test]
    fn dm_voice_join_tracks_peer_when_not_in_voice() {
        let mut app = NexusApp::default();
        app.config.settings.show_join_leave_events = false;
        let (mut conn, _rx) = test_connection_with_receiver(1, "me", "Alice");
        conn.voice_session = None;
        app.connections.insert(1, conn);

        let _ = app.handle_voice_user_joined(1, "Alice".to_string(), "Alice".to_string());

        assert!(
            app.connections[&1]
                .user_message_voiced
                .contains(&fold_name("Alice"))
        );
    }

    #[test]
    fn dm_voice_left_clears_peer_when_not_in_voice() {
        let mut app = NexusApp::default();
        app.config.settings.show_join_leave_events = false;
        let (mut conn, _rx) = test_connection_with_receiver(1, "me", "Alice");
        conn.voice_session = None;
        conn.user_message_voiced.insert(fold_name("Alice"));
        app.connections.insert(1, conn);

        let _ = app.handle_voice_user_left(1, "Alice".to_string(), "Alice".to_string());

        assert!(
            !app.connections[&1]
                .user_message_voiced
                .contains(&fold_name("Alice"))
        );
    }

    #[test]
    fn dm_voice_join_response_seeds_peer_presence() {
        let mut app = NexusApp::default();
        let (conn, _rx) = test_connection_with_receiver(1, "me", "Alice");
        app.connections.insert(1, conn);

        let token = Uuid::new_v4();
        let _ = app.handle_voice_join_response(
            1,
            true,
            Some(token),
            Some("Alice".to_string()),
            Some(vec!["me".to_string(), "Alice".to_string()]),
            None,
        );

        assert!(
            app.connections[&1]
                .user_message_voiced
                .contains(&fold_name("Alice"))
        );
        assert!(
            !app.connections[&1]
                .user_message_voiced
                .contains(&fold_name("me"))
        );
    }

    #[test]
    fn dm_voice_join_response_clears_stale_peer_presence() {
        let mut app = NexusApp::default();
        let (mut conn, _rx) = test_connection_with_receiver(1, "me", "Alice");
        conn.user_message_voiced.insert(fold_name("Alice"));
        app.connections.insert(1, conn);

        let token = Uuid::new_v4();
        let _ = app.handle_voice_join_response(
            1,
            true,
            Some(token),
            Some("Alice".to_string()),
            Some(vec!["me".to_string()]),
            None,
        );

        assert!(
            !app.connections[&1]
                .user_message_voiced
                .contains(&fold_name("Alice"))
        );
    }

    #[test]
    fn voice_join_response_after_pending_leave_does_not_start_voice() {
        let mut app = NexusApp::default();
        let (mut conn, mut rx) = test_connection_with_receiver(1, "me", "#general");
        conn.voice_session.as_mut().unwrap().leave_sent = true;
        app.connections.insert(1, conn);

        let token = Uuid::new_v4();
        let _ = app.handle_voice_join_response(
            1,
            true,
            Some(token),
            Some("#general".to_string()),
            Some(vec!["me".to_string()]),
            None,
        );

        assert_eq!(app.active_voice_connection, None);
        let session = app.connections[&1].voice_session.as_ref().unwrap();
        assert_eq!(session.token, None);
        assert!(session.leave_sent);
        assert!(rx.try_recv().is_err());
    }

    #[test]
    fn voice_join_response_records_voice_talk_transmit_capability() {
        let mut app = NexusApp::default();
        let (mut conn, _rx) = test_connection_with_receiver(1, "me", "#general");
        conn.permissions.push(PERMISSION_VOICE_TALK.to_string());
        app.connections.insert(1, conn);

        let token = Uuid::new_v4();
        let _ = app.handle_voice_join_response(
            1,
            true,
            Some(token),
            Some("#general".to_string()),
            Some(vec!["me".to_string()]),
            None,
        );

        let session = app.connections[&1].voice_session.as_ref().unwrap();
        assert_eq!(session.token, Some(token));
        assert!(session.can_transmit);
        assert!(session.transmit_initialized);
    }

    #[test]
    fn voice_join_response_without_voice_talk_is_listen_only() {
        let mut app = NexusApp::default();
        let (conn, _rx) = test_connection_with_receiver(1, "me", "#general");
        app.connections.insert(1, conn);

        let token = Uuid::new_v4();
        let _ = app.handle_voice_join_response(
            1,
            true,
            Some(token),
            Some("#general".to_string()),
            Some(vec!["me".to_string()]),
            None,
        );

        let session = app.connections[&1].voice_session.as_ref().unwrap();
        assert_eq!(session.token, Some(token));
        assert!(!session.can_transmit);
        assert!(!session.transmit_initialized);
    }

    #[test]
    fn stale_voice_join_failure_does_not_clear_accepted_session() {
        let mut app = NexusApp::default();
        let (mut conn, _rx) = test_connection_with_receiver(1, "me", "#general");
        let token = Uuid::new_v4();
        conn.voice_session = Some(VoiceState::new_with_token(
            "#general".to_string(),
            vec!["me".to_string()],
            token,
            true,
        ));
        app.active_voice_connection = Some(1);
        app.connections.insert(1, conn);

        let _ = app.handle_voice_join_response(
            1,
            false,
            None,
            None,
            None,
            Some("already joined".to_string()),
        );

        let session = app.connections[&1].voice_session.as_ref().unwrap();
        assert_eq!(session.token, Some(token));
        assert_eq!(app.active_voice_connection, Some(1));
        assert!(app.connections[&1].console_messages.is_empty());
    }

    #[test]
    fn voice_address_resolve_failure_sends_leave_once_and_cleans_up() {
        let mut app = NexusApp::default();
        let (mut conn, mut rx) = test_connection_with_receiver(1, "me", "#general");
        let token = Uuid::new_v4();
        conn.voice_session = Some(VoiceState::new_with_token(
            "#general".to_string(),
            vec!["me".to_string()],
            token,
            true,
        ));
        app.active_voice_connection = Some(1);
        app.connections.insert(1, conn);

        let _ = app.handle_voice_address_resolved(1, token, Err("resolve failed".to_string()));

        expect_voice_leave(&mut rx);
        assert!(rx.try_recv().is_err());
        assert!(app.connections[&1].voice_session.is_none());
        assert_eq!(app.active_voice_connection, None);
    }

    #[test]
    fn voice_address_resolve_after_leave_sent_does_not_send_again() {
        let mut app = NexusApp::default();
        let (mut conn, mut rx) = test_connection_with_receiver(1, "me", "#general");
        let token = Uuid::new_v4();
        conn.voice_session = Some(VoiceState::new_with_token(
            "#general".to_string(),
            vec!["me".to_string()],
            token,
            true,
        ));
        app.active_voice_connection = Some(1);
        app.connections.insert(1, conn);

        assert_eq!(app.send_voice_leave_once(1), Ok(true));
        expect_voice_leave(&mut rx);

        let _ = app.handle_voice_address_resolved(1, token, Err("resolve failed".to_string()));

        assert!(rx.try_recv().is_err());
        assert!(app.connections[&1].voice_session.is_some());
        assert_eq!(app.active_voice_connection, Some(1));
    }
}
