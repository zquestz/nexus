//! Voice chat UI handlers
//!
//! Handles user interactions with voice chat controls:
//! - VoiceJoinPressed - User clicks to join voice for a channel or user message
//! - VoiceLeavePressed - User clicks to leave current voice session
//! - VoiceSessionEvent - Events from the voice session (connected, speaking, etc.)
//! - VoicePttEvent - Raw PTT hotkey events
//! - VoicePttReleaseDelayExpired - PTT release delay timer expired
//! - VoiceUserMute/VoiceUserUnmute - Mute/unmute a user (client-side)

use std::time::Duration;

use crate::network::{FEATURE_VOICE, tls::should_bypass_proxy};

use global_hotkey::GlobalHotKeyEvent;
use iced::Task;
use nexus_common::names::fold_name;
use nexus_common::protocol::ClientMessage;

use crate::NexusApp;
use crate::config::settings::ProxySettings;
use crate::i18n::{t, t_args};
use crate::types::{ChatMessage, ChatTab, Message, VoiceState};
use crate::views::constants::PERMISSION_VOICE_LISTEN;
use crate::voice::manager::VoiceEvent;
use crate::voice::ptt::PttState;

/// Voice uses direct UDP that can't traverse SOCKS5, so a join must be blocked
/// when a non-bypassed proxy is active — unless the user opted into the bypass
/// (which exposes their real IP). Isolated as the privacy boundary for testing.
fn voice_join_blocked_by_proxy(proxy: &ProxySettings, address: &str) -> bool {
    proxy.enabled && !proxy.allow_voice_bypass && !should_bypass_proxy(address)
}

impl NexusApp {
    /// Handle voice join button pressed
    ///
    /// Sends a VoiceJoin request to the server for the specified target.
    /// The target is a channel name (e.g., "#general") or nickname for user message voice.
    pub fn handle_voice_join_pressed(&mut self, target: String) -> Task<Message> {
        let Some(connection_id) = self.active_connection else {
            return Task::none();
        };

        let existing_voice_connection = self
            .connections
            .iter()
            .find_map(|(id, conn)| conn.voice_session.is_some().then_some(*id));

        if let Some(existing_connection_id) = existing_voice_connection {
            if existing_connection_id == connection_id {
                return Task::none();
            }

            return self.add_active_tab_message(
                connection_id,
                ChatMessage::error(t("err-voice-already-active")),
            );
        }

        let Some(conn) = self.connections.get_mut(&connection_id) else {
            return Task::none();
        };

        if target.starts_with('#')
            && conn
                .pending_channel_leave
                .as_ref()
                .is_some_and(|channel| fold_name(channel) == fold_name(&target))
        {
            return self.add_active_tab_message(
                connection_id,
                ChatMessage::error(t("err-leave-already-pending")),
            );
        }

        if !conn.has_feature(FEATURE_VOICE) {
            return self.add_active_tab_message(
                connection_id,
                ChatMessage::error(t("err-voice-feature-not-enabled")),
            );
        }

        if voice_join_blocked_by_proxy(&self.config.settings.proxy, &conn.connection_info.address) {
            return self.add_active_tab_message(
                connection_id,
                ChatMessage::error(t("err-voice-proxy-not-supported")),
            );
        }

        // `voice_listen` gates joining/listening. `voice_talk` only gates
        // microphone transmit once a session is active.
        if !conn.has_permission(PERMISSION_VOICE_LISTEN) {
            return self.add_active_tab_message(
                connection_id,
                ChatMessage::error(t("err-voice-no-permission")),
            );
        }

        // For channels, check if we're a member
        // Note: conn.channels is keyed by lowercase channel name WITH the # prefix
        if target.starts_with('#') {
            let channel_lower = fold_name(&target);
            if !conn.channels.contains_key(&channel_lower) {
                return self.add_active_tab_message(
                    connection_id,
                    ChatMessage::error(t("err-voice-not-in-channel")),
                );
            }
        }

        // Store the target in a pending voice session so we can use it in the response
        // We create a placeholder session that will be replaced on success
        conn.voice_session = Some(VoiceState::new(target.clone(), Vec::new()));

        // Send the VoiceJoin request
        if let Err(e) = conn.send(ClientMessage::VoiceJoin { target }) {
            // Clear the pending session on send failure
            conn.voice_session = None;
            return self.add_active_tab_message(connection_id, ChatMessage::error(e));
        }

        Task::none()
    }

    pub(crate) fn active_voice_can_transmit(&self) -> bool {
        let Some(connection_id) = self.active_voice_connection else {
            return false;
        };

        self.connections
            .get(&connection_id)
            .and_then(|conn| conn.voice_session.as_ref())
            .is_some_and(|session| session.can_transmit)
    }

    pub(crate) fn enable_voice_transmit_controls(&mut self) -> Result<(), String> {
        // Lazily create PTT manager on first transmit-capable voice join (not at
        // startup). This keeps listen-only sessions from grabbing mic hotkeys.
        if self.ptt_manager.is_none() {
            self.ptt_manager = Some(crate::voice::ptt::PttManager::new()?);
        }

        if let Some(ref mut ptt) = self.ptt_manager {
            ptt.set_mode(self.config.settings.audio.ptt_mode);
            ptt.register_hotkey(&self.config.settings.audio.ptt_key)?;
            ptt.set_in_voice(true);
        }

        Ok(())
    }

    pub(crate) fn disable_voice_transmit_controls(&mut self) {
        if let Some(ref handle) = self.voice_session_handle {
            handle.stop_transmitting();
        }

        if let Some(ref mut ptt) = self.ptt_manager {
            ptt.set_in_voice(false);
            ptt.unregister_hotkey();
        }

        self.ptt_release_delay_generation += 1;
        self.is_local_speaking = false;
        self.mic_level
            .store(0f32.to_bits(), std::sync::atomic::Ordering::Relaxed);

        #[cfg(not(target_os = "macos"))]
        self.update_tray_state();
    }

    pub(crate) fn set_active_voice_transmit_allowed(
        &mut self,
        connection_id: usize,
        can_transmit: bool,
    ) -> Task<Message> {
        if self.active_voice_connection != Some(connection_id) {
            return Task::none();
        }

        let Some(conn) = self.connections.get_mut(&connection_id) else {
            return Task::none();
        };
        let Some(session) = conn.voice_session.as_mut() else {
            return Task::none();
        };

        if session.can_transmit == can_transmit {
            return Task::none();
        }

        let enable_worker_transmit = can_transmit && !session.transmit_initialized;
        session.can_transmit = can_transmit;
        if can_transmit {
            session.transmit_initialized = true;
        }

        if !can_transmit {
            self.disable_voice_transmit_controls();
            return Task::none();
        }

        if enable_worker_transmit && let Some(ref handle) = self.voice_session_handle {
            handle.enable_transmit();
        }

        match self.enable_voice_transmit_controls() {
            Ok(()) => Task::none(),
            Err(e) => self.add_active_tab_message(
                connection_id,
                ChatMessage::error(t_args("err-voice-ptt-failed", &[("error", &e)])),
            ),
        }
    }

    /// Handle voice leave button pressed
    ///
    /// Sends a VoiceLeave request to the server to leave the current voice session.
    pub fn handle_voice_leave_pressed(&mut self) -> Task<Message> {
        let Some(connection_id) = self.active_connection else {
            return Task::none();
        };

        let in_voice = self
            .connections
            .get(&connection_id)
            .is_some_and(|conn| conn.voice_session.is_some());
        if !in_voice {
            return self.add_active_tab_message(
                connection_id,
                ChatMessage::error(t("err-voice-not-in-session")),
            );
        }

        if let Err(e) = self.send_voice_leave_once(connection_id) {
            return self.add_active_tab_message(connection_id, ChatMessage::error(e));
        }

        Task::none()
    }

    /// Get the voice target for the current chat tab
    ///
    /// Returns the appropriate voice target based on the active chat tab:
    /// - Channel tab: Returns the channel name (e.g., "#general")
    /// - UserMessage tab: Returns the other user's nickname
    /// - Console tab: Returns None (can't join voice from console)
    pub fn get_voice_target_for_current_tab(&self) -> Option<String> {
        let connection_id = self.active_connection?;
        let conn = self.connections.get(&connection_id)?;

        match &conn.active_chat_tab {
            // Channel name already includes the # prefix
            ChatTab::Channel(channel) => Some(channel.clone()),
            ChatTab::UserMessage(nickname) => Some(nickname.clone()),
            ChatTab::Console => None,
        }
    }

    /// Handle voice session events from the DTLS client
    ///
    /// These events come from the voice session thread and update the UI
    /// with connection status, speaking indicators, and errors.
    pub fn handle_voice_session_event(
        &mut self,
        connection_id: usize,
        event: VoiceEvent,
    ) -> Task<Message> {
        match event {
            VoiceEvent::Connected => {
                // DTLS connection established - voice is now active
                // The voice bar already shows, so no additional feedback needed
                Task::none()
            }

            VoiceEvent::ConnectionFailed(error) => {
                // DTLS connection failed - notify server and clean up
                self.leave_voice_session(connection_id);
                self.add_active_tab_message(
                    connection_id,
                    ChatMessage::error(t_args("err-voice-connection-failed", &[("error", &error)])),
                )
            }

            VoiceEvent::Disconnected(reason) => {
                // DTLS connection lost - notify server and clean up
                self.leave_voice_session(connection_id);
                if let Some(reason) = reason {
                    self.add_active_tab_message(
                        connection_id,
                        ChatMessage::error(t_args(
                            "err-voice-disconnected",
                            &[("reason", &reason)],
                        )),
                    )
                } else {
                    Task::none()
                }
            }

            VoiceEvent::SpeakingStarted(nickname) => {
                // A user started speaking - update speaking set
                let folded_nickname = fold_name(&nickname);
                if let Some(conn) = self.connections.get_mut(&connection_id)
                    && let Some(ref mut session) = conn.voice_session
                    // The audio thread also gates unknown speakers; this UI-side
                    // gate keeps queued events from reintroducing stale rename state.
                    && session
                        .participants
                        .iter()
                        .any(|n| fold_name(n) == folded_nickname)
                {
                    session.set_speaking(&nickname);
                }
                Task::none()
            }

            VoiceEvent::SpeakingStopped(nickname) => {
                // A user stopped speaking - update speaking set
                if let Some(conn) = self.connections.get_mut(&connection_id)
                    && let Some(ref mut session) = conn.voice_session
                {
                    session.set_not_speaking(&nickname);
                }
                Task::none()
            }

            VoiceEvent::AudioError(error) => {
                // Audio device error - notify server and clean up
                self.leave_voice_session(connection_id);
                self.add_active_tab_message(
                    connection_id,
                    ChatMessage::error(t_args("err-voice-audio", &[("error", &error)])),
                )
            }

            VoiceEvent::TransmitEnableFailed(error) => {
                // Enabling talk failed after a listen-only join; stay in voice.
                if self.active_voice_connection == Some(connection_id) {
                    if let Some(conn) = self.connections.get_mut(&connection_id)
                        && let Some(session) = conn.voice_session.as_mut()
                    {
                        session.can_transmit = false;
                        session.transmit_initialized = false;
                    }
                    self.disable_voice_transmit_controls();
                }
                self.add_active_tab_message(
                    connection_id,
                    ChatMessage::error(t_args("err-voice-audio", &[("error", &error)])),
                )
            }

            VoiceEvent::AudioProcessorDisabled(error) => {
                // Audio processor failed - voice works but no noise suppression/AGC
                self.add_active_tab_message(
                    connection_id,
                    ChatMessage::error(t_args(
                        "warn-voice-processor-disabled",
                        &[("error", &error)],
                    )),
                )
            }

            VoiceEvent::QualityChangeFailed(error) => {
                // Voice quality change failed - continue with previous quality
                self.add_active_tab_message(
                    connection_id,
                    ChatMessage::error(t_args("warn-voice-quality-failed", &[("error", &error)])),
                )
            }

            VoiceEvent::LocalSpeakingChanged(speaking) => {
                // Local user started/stopped speaking - update PTT indicator and speaking set
                self.is_local_speaking = speaking;

                // Also update the speaking set so the user list shows the mic icon
                if let Some(conn) = self.connections.get_mut(&connection_id)
                    && let Some(ref mut session) = conn.voice_session
                {
                    if speaking {
                        session.set_speaking(&conn.nickname);
                    } else {
                        session.set_not_speaking(&conn.nickname);
                    }
                }

                // Update tray icon state (Windows/Linux only)
                #[cfg(not(target_os = "macos"))]
                self.update_tray_state();

                Task::none()
            }
        }
    }

    /// Leave voice session: notify server and clean up local state
    ///
    /// Sends VoiceLeave to the server (if still in a session) and cleans up
    /// all local voice state. Safe to call multiple times - only sends once.
    fn leave_voice_session(&mut self, connection_id: usize) {
        let _ = self.send_voice_leave_once(connection_id);

        // Clean up local state
        self.cleanup_voice_session(connection_id);
    }

    pub(crate) fn send_voice_leave_once(&mut self, connection_id: usize) -> Result<bool, String> {
        let Some(conn) = self.connections.get_mut(&connection_id) else {
            return Ok(false);
        };

        let should_send = match conn.voice_session.as_mut() {
            Some(session) if !session.leave_sent => {
                session.leave_sent = true;
                true
            }
            _ => false,
        };

        if !should_send {
            return Ok(false);
        }

        if let Err(e) = conn.send(ClientMessage::VoiceLeave) {
            if let Some(session) = conn.voice_session.as_mut() {
                session.leave_sent = false;
            }
            return Err(e);
        }

        Ok(true)
    }

    /// Clean up voice session state (local only, does not notify server)
    ///
    /// Used when the server already knows we're leaving (e.g., VoiceLeaveResponse,
    /// VoiceUserLeft for self, or TCP disconnect).
    pub fn cleanup_voice_session(&mut self, connection_id: usize) {
        // Clear voice session from connection
        if let Some(conn) = self.connections.get_mut(&connection_id) {
            conn.voice_session = None;
        }

        // Clear active voice connection if it was this one
        if self.active_voice_connection == Some(connection_id) {
            self.active_voice_connection = None;

            // Stop the voice session handle
            if let Some(mut handle) = self.voice_session_handle.take() {
                handle.stop();
            }

            // Clean up the voice event receiver from the registry
            crate::voice::subscription::unregister_voice_receiver_sync(connection_id);

            // Stop PTT and unregister hotkey
            if let Some(ref mut ptt) = self.ptt_manager {
                ptt.set_in_voice(false);
                ptt.unregister_hotkey();
            }

            // Recreate the PttManager to force the X11 connection to close,
            // which releases all key grabs. This works around a bug in the
            // global-hotkey crate where ungrab_key doesn't reliably release grabs.
            self.ptt_manager = crate::voice::ptt::PttManager::new().ok();

            // Cancel any pending PTT release delay timer by incrementing generation
            self.ptt_release_delay_generation += 1;

            // Clear local speaking and deafened state
            self.is_local_speaking = false;
            self.is_deafened = false;

            // Update tray icon state (Windows/Linux only)
            #[cfg(not(target_os = "macos"))]
            self.update_tray_state();
        }
    }

    /// Handle PTT state changed (from global hotkey event)
    ///
    /// Called when the PTT hotkey is pressed or released. Starts or stops
    /// audio transmission based on the new state.
    ///
    /// If PTT release delay is configured, stopping transmission is delayed
    /// to prevent cutting off the end of words/sentences.
    pub fn handle_voice_ptt_state_changed(&mut self, state: PttState) -> Task<Message> {
        // Only act if we have an active voice session
        let Some(ref handle) = self.voice_session_handle else {
            return Task::none();
        };
        if !self.active_voice_can_transmit() {
            return Task::none();
        }

        match state {
            PttState::Transmitting => {
                // Cancel any pending release delay by incrementing generation
                // (the delayed task will see a mismatched generation and do nothing)
                self.ptt_release_delay_generation += 1;

                handle.start_transmitting();
                Task::none()
            }
            PttState::Idle => {
                let delay = self.config.settings.audio.ptt_release_delay;
                let delay_ms = delay.as_millis();

                if delay_ms == 0 {
                    // No delay - stop immediately
                    handle.stop_transmitting();
                    Task::none()
                } else {
                    // Start a delayed stop
                    self.ptt_release_delay_generation += 1;
                    let generation = self.ptt_release_delay_generation;

                    Task::future(async move {
                        tokio::time::sleep(Duration::from_millis(delay_ms)).await;
                        Message::VoicePttReleaseDelayExpired(generation)
                    })
                }
            }
        }
    }

    /// Handle PTT release delay timer expired
    ///
    /// Called when the PTT release delay timer fires. Only stops transmission
    /// if the generation matches (i.e., PTT wasn't pressed again during the delay).
    pub fn handle_voice_ptt_release_delay_expired(&mut self, generation: u64) -> Task<Message> {
        // Check if this timer is still current
        if generation != self.ptt_release_delay_generation {
            // PTT was pressed again during the delay - ignore this expired timer
            return Task::none();
        }
        if !self.active_voice_can_transmit() {
            return Task::none();
        }

        // Timer is current - stop transmitting
        if let Some(ref handle) = self.voice_session_handle {
            handle.stop_transmitting();
        }

        Task::none()
    }

    /// Handle voice user mute (client-side)
    ///
    /// Mutes a user so the local client no longer hears their audio.
    /// This is purely client-side and doesn't affect other users.
    pub fn handle_voice_user_mute(&mut self, nickname: String) -> Task<Message> {
        let Some(connection_id) = self.active_voice_connection else {
            return Task::none();
        };

        // Update local session state
        if let Some(conn) = self.connections.get_mut(&connection_id)
            && let Some(ref mut session) = conn.voice_session
        {
            session.mute_user(&nickname);
        }

        // Tell the voice manager to stop playing this user's audio
        if let Some(ref handle) = self.voice_session_handle {
            handle.mute_user(&nickname);
        }

        Task::none()
    }

    /// Handle voice user unmute (client-side)
    ///
    /// Unmutes a previously muted user so the local client can hear them again.
    pub fn handle_voice_user_unmute(&mut self, nickname: String) -> Task<Message> {
        let Some(connection_id) = self.active_voice_connection else {
            return Task::none();
        };

        // Update local session state
        if let Some(conn) = self.connections.get_mut(&connection_id)
            && let Some(ref mut session) = conn.voice_session
        {
            session.unmute_user(&nickname);
        }

        // Tell the voice manager to resume playing this user's audio
        if let Some(ref handle) = self.voice_session_handle {
            handle.unmute_user(&nickname);
        }

        Task::none()
    }

    /// Handle voice deafen toggle
    ///
    /// Toggles the deafened state - when deafened, all incoming voice audio is muted.
    /// The user remains in voice and can still transmit if they use PTT.
    pub fn handle_voice_deafen_toggle(&mut self) -> Task<Message> {
        // Toggle deafened state
        self.is_deafened = !self.is_deafened;

        // Tell the voice manager to update audio output
        if let Some(ref handle) = self.voice_session_handle {
            handle.set_deafened(self.is_deafened);
        }

        // Update tray icon state (Windows/Linux only)
        #[cfg(not(target_os = "macos"))]
        self.update_tray_state();

        Task::none()
    }

    /// Handle raw PTT hotkey event from global hotkey subscription
    ///
    /// Forwards the event to the PttManager to determine if it's our hotkey
    /// and what state change (if any) occurred.
    pub fn handle_voice_ptt_event(&mut self, event: GlobalHotKeyEvent) -> Task<Message> {
        // Only process if we have a PTT manager
        let Some(ref mut ptt) = self.ptt_manager else {
            return Task::none();
        };

        // Let the PTT manager handle the event
        if let Some(state) = ptt.handle_event(event) {
            // State changed, forward to the state change handler
            return self.handle_voice_ptt_state_changed(state);
        }

        Task::none()
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use nexus_common::framing::MessageId;
    use nexus_common::validators::PasswordStrength;
    use tokio::sync::{Mutex, mpsc};
    use uuid::Uuid;

    use super::*;
    use crate::types::{ChannelState, ConnectionInfo, ServerConnection, ServerConnectionParams};
    use crate::views::constants::PERMISSION_VOICE_TALK;

    fn proxy(enabled: bool, allow_voice_bypass: bool) -> ProxySettings {
        ProxySettings {
            enabled,
            address: "proxy.example.com".to_string(),
            port: 9050,
            username: None,
            password: None,
            allow_voice_bypass,
        }
    }

    fn test_connection_with_receiver(
        connection_id: usize,
    ) -> (
        ServerConnection,
        mpsc::UnboundedReceiver<(MessageId, ClientMessage)>,
    ) {
        let (tx, rx) = mpsc::unbounded_channel();
        let conn = ServerConnection::new(ServerConnectionParams {
            bookmark_id: None,
            user_id: None,
            nickname: "me".to_string(),
            connection_info: ConnectionInfo {
                server_name: String::new(),
                address: String::new(),
                port: 0,
                transfer_port: 0,
                certificate_fingerprint: String::new(),
                username: "me".to_string(),
                password: String::new(),
                nickname: "me".to_string(),
            },
            display_name: String::new(),
            connection_id,
            is_admin: false,
            permissions: Vec::new(),
            features: vec![FEATURE_VOICE.to_string()],
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
        (conn, rx)
    }

    #[test]
    fn voice_join_pressed_ignores_existing_session_on_same_connection() {
        let mut app = NexusApp {
            active_connection: Some(1),
            ..NexusApp::default()
        };
        let (mut conn, mut rx) = test_connection_with_receiver(1);
        conn.voice_session = Some(VoiceState::new("#general".to_string(), Vec::new()));
        app.connections.insert(1, conn);

        let _ = app.handle_voice_join_pressed("#general".to_string());

        assert!(rx.try_recv().is_err());
        assert!(app.connections[&1].voice_session.is_some());
    }

    #[test]
    fn voice_join_pressed_blocks_pending_session_on_another_connection() {
        let mut app = NexusApp {
            active_connection: Some(2),
            ..NexusApp::default()
        };
        let (mut first_conn, mut first_rx) = test_connection_with_receiver(1);
        first_conn.voice_session = Some(VoiceState::new("#general".to_string(), Vec::new()));
        let (second_conn, mut second_rx) = test_connection_with_receiver(2);
        app.connections.insert(1, first_conn);
        app.connections.insert(2, second_conn);

        let _ = app.handle_voice_join_pressed("#random".to_string());

        assert!(first_rx.try_recv().is_err());
        assert!(second_rx.try_recv().is_err());
        assert_eq!(app.connections[&2].console_messages.len(), 1);
    }

    #[test]
    fn voice_join_pressed_blocks_channel_pending_leave() {
        let mut app = NexusApp {
            active_connection: Some(1),
            ..NexusApp::default()
        };
        let (mut conn, mut rx) = test_connection_with_receiver(1);
        conn.permissions.push(PERMISSION_VOICE_LISTEN.to_string());
        conn.channels.insert(
            fold_name("#general"),
            ChannelState::new(None, None, false, vec!["me".to_string()]),
        );
        conn.pending_channel_leave = Some("#General".to_string());
        app.connections.insert(1, conn);

        let _ = app.handle_voice_join_pressed("#general".to_string());

        assert!(rx.try_recv().is_err());
        assert!(app.connections[&1].voice_session.is_none());
        assert_eq!(app.connections[&1].console_messages.len(), 1);
    }

    #[test]
    fn voice_join_pressed_requires_voice_feature() {
        let mut app = NexusApp {
            active_connection: Some(1),
            ..NexusApp::default()
        };
        let (mut conn, mut rx) = test_connection_with_receiver(1);
        conn.features.clear();
        conn.permissions.push(PERMISSION_VOICE_LISTEN.to_string());
        conn.channels.insert(
            fold_name("#general"),
            ChannelState::new(None, None, false, vec!["me".to_string()]),
        );
        app.connections.insert(1, conn);

        let _ = app.handle_voice_join_pressed("#general".to_string());

        assert!(rx.try_recv().is_err());
        assert!(app.connections[&1].voice_session.is_none());
        assert_eq!(app.connections[&1].console_messages.len(), 1);
    }

    #[test]
    fn voice_join_pressed_requires_voice_listen_not_voice_talk() {
        let mut app = NexusApp {
            active_connection: Some(1),
            ..NexusApp::default()
        };
        let (mut conn, mut rx) = test_connection_with_receiver(1);
        conn.permissions.push(PERMISSION_VOICE_TALK.to_string());
        conn.channels.insert(
            fold_name("#general"),
            ChannelState::new(None, None, false, vec!["me".to_string()]),
        );
        app.connections.insert(1, conn);

        let _ = app.handle_voice_join_pressed("#general".to_string());

        assert!(rx.try_recv().is_err());
        assert!(app.connections[&1].voice_session.is_none());
        assert_eq!(app.connections[&1].console_messages.len(), 1);
    }

    #[test]
    fn voice_join_pressed_allows_voice_listen_without_voice_talk() {
        let mut app = NexusApp {
            active_connection: Some(1),
            ..NexusApp::default()
        };
        let (mut conn, mut rx) = test_connection_with_receiver(1);
        conn.permissions.push(PERMISSION_VOICE_LISTEN.to_string());
        conn.channels.insert(
            fold_name("#general"),
            ChannelState::new(None, None, false, vec!["me".to_string()]),
        );
        app.connections.insert(1, conn);

        let _ = app.handle_voice_join_pressed("#general".to_string());

        match rx.try_recv() {
            Ok((_, ClientMessage::VoiceJoin { target })) => assert_eq!(target, "#general"),
            other => panic!("expected VoiceJoin, got {other:?}"),
        }
        assert!(
            app.connections[&1]
                .voice_session
                .as_ref()
                .is_some_and(|session| session.token.is_none() && !session.can_transmit)
        );
    }

    #[test]
    fn disabling_voice_transmit_stops_local_transmission() {
        let mut app = NexusApp::default();
        let (mut conn, _rx) = test_connection_with_receiver(1);
        conn.voice_session = Some(VoiceState::new_with_token(
            "#general".to_string(),
            vec!["me".to_string()],
            Uuid::new_v4(),
            true,
        ));
        app.active_voice_connection = Some(1);
        app.is_local_speaking = true;
        let (handle, mut commands) = crate::voice::manager::VoiceSessionHandle::test_handle();
        app.voice_session_handle = Some(handle);
        app.connections.insert(1, conn);

        let _ = app.set_active_voice_transmit_allowed(1, false);

        assert!(
            !app.connections[&1]
                .voice_session
                .as_ref()
                .unwrap()
                .can_transmit
        );
        assert!(!app.is_local_speaking);
        match commands.try_recv() {
            Ok(crate::voice::manager::VoiceCommand::StopTransmitting) => {}
            other => panic!("expected StopTransmitting, got {other:?}"),
        }
    }

    #[test]
    fn transmit_enable_failure_keeps_listening_but_disables_transmit() {
        let mut app = NexusApp::default();
        let (mut conn, _rx) = test_connection_with_receiver(1);
        conn.voice_session = Some(VoiceState::new_with_token(
            "#general".to_string(),
            vec!["me".to_string()],
            Uuid::new_v4(),
            true,
        ));
        app.active_voice_connection = Some(1);
        app.is_local_speaking = true;
        let (handle, mut commands) = crate::voice::manager::VoiceSessionHandle::test_handle();
        app.voice_session_handle = Some(handle);
        app.connections.insert(1, conn);

        let _ = app.handle_voice_session_event(
            1,
            VoiceEvent::TransmitEnableFailed("no input device".to_string()),
        );

        let session = app.connections[&1].voice_session.as_ref().unwrap();
        assert!(!session.can_transmit);
        assert!(!session.transmit_initialized);
        assert!(!app.is_local_speaking);
        match commands.try_recv() {
            Ok(crate::voice::manager::VoiceCommand::StopTransmitting) => {}
            other => panic!("expected StopTransmitting, got {other:?}"),
        }
    }

    #[test]
    fn granting_voice_talk_after_listen_only_join_enables_transmit() {
        let mut app = NexusApp::default();
        let (mut conn, _rx) = test_connection_with_receiver(1);
        conn.voice_session = Some(VoiceState::new_with_token(
            "#general".to_string(),
            vec!["me".to_string()],
            Uuid::new_v4(),
            false,
        ));
        app.active_voice_connection = Some(1);
        let (handle, mut commands) = crate::voice::manager::VoiceSessionHandle::test_handle();
        app.voice_session_handle = Some(handle);
        app.connections.insert(1, conn);

        let _ = app.set_active_voice_transmit_allowed(1, true);

        let session = app.connections[&1].voice_session.as_ref().unwrap();
        assert!(session.can_transmit);
        assert!(session.transmit_initialized);
        assert!(app.active_voice_can_transmit());
        match commands.try_recv() {
            Ok(crate::voice::manager::VoiceCommand::EnableTransmit) => {}
            other => panic!("expected EnableTransmit, got {other:?}"),
        }
    }

    #[test]
    fn not_blocked_when_proxy_disabled() {
        assert!(!voice_join_blocked_by_proxy(
            &proxy(false, false),
            "bbs.example.com"
        ));
    }

    #[test]
    fn blocked_when_proxy_active_and_not_opted_in() {
        assert!(voice_join_blocked_by_proxy(
            &proxy(true, false),
            "bbs.example.com"
        ));
    }

    #[test]
    fn not_blocked_when_voice_bypass_opted_in() {
        assert!(!voice_join_blocked_by_proxy(
            &proxy(true, true),
            "bbs.example.com"
        ));
    }

    #[test]
    fn not_blocked_for_proxy_bypassed_address() {
        // Loopback is auto-bypassed, so the proxy never applies — voice is fine.
        assert!(!voice_join_blocked_by_proxy(
            &proxy(true, false),
            "127.0.0.1"
        ));
    }
}
