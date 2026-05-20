//! Voice chat UI handlers
//!
//! Handles user interactions with voice chat controls:
//! - VoiceJoinPressed - User clicks to join voice for a channel or user message
//! - VoiceLeavePressed - User clicks to leave current voice session
//! - VoiceSessionEvent - Events from the voice session (connected, speaking, etc.)
//! - VoicePttStateChanged - PTT hotkey pressed/released
//! - VoicePttReleaseDelayExpired - PTT release delay timer expired
//! - VoiceUserMute/VoiceUserUnmute - Mute/unmute a user (client-side)

use std::time::Duration;

use crate::network::tls::should_bypass_proxy;

use global_hotkey::GlobalHotKeyEvent;
use iced::Task;
use nexus_common::protocol::ClientMessage;

use crate::NexusApp;
use crate::config::settings::ProxySettings;
use crate::i18n::{t, t_args};
use crate::types::{ChatMessage, ChatTab, Message, VoiceState};
use crate::views::constants::{PERMISSION_VOICE_LISTEN, PERMISSION_VOICE_TALK};
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

        // Check if we already have a voice session on another connection
        if let Some(active_voice_conn) = self.active_voice_connection
            && active_voice_conn != connection_id
        {
            // Show error - can only have one voice session at a time
            return self.add_active_tab_message(
                connection_id,
                ChatMessage::error(t("err-voice-already-active")),
            );
        }

        let Some(conn) = self.connections.get_mut(&connection_id) else {
            return Task::none();
        };

        if voice_join_blocked_by_proxy(&self.config.settings.proxy, &conn.connection_info.address) {
            return self.add_active_tab_message(
                connection_id,
                ChatMessage::error(t("err-voice-proxy-not-supported")),
            );
        }

        // Check permissions - need at least voice_listen to join
        if !conn.has_permission(PERMISSION_VOICE_LISTEN)
            && !conn.has_permission(PERMISSION_VOICE_TALK)
        {
            return self.add_active_tab_message(
                connection_id,
                ChatMessage::error(t("err-voice-no-permission")),
            );
        }

        // For channels, check if we're a member
        // Note: conn.channels is keyed by lowercase channel name WITH the # prefix
        if target.starts_with('#') {
            let channel_lower = target.to_lowercase();
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

    /// Handle voice leave button pressed
    ///
    /// Sends a VoiceLeave request to the server to leave the current voice session.
    pub fn handle_voice_leave_pressed(&mut self) -> Task<Message> {
        let Some(connection_id) = self.active_connection else {
            return Task::none();
        };

        let Some(conn) = self.connections.get_mut(&connection_id) else {
            return Task::none();
        };

        // Check if we're actually in a voice session
        if conn.voice_session.is_none() {
            return self.add_active_tab_message(
                connection_id,
                ChatMessage::error(t("err-voice-not-in-session")),
            );
        }

        // Send the VoiceLeave request
        if let Err(e) = conn.send(ClientMessage::VoiceLeave) {
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

    /// Check if the current tab matches the active voice session target
    #[allow(dead_code)] // Available for UI state checks
    pub fn is_voice_target_current_tab(&self) -> bool {
        let Some(connection_id) = self.active_connection else {
            return false;
        };

        let Some(conn) = self.connections.get(&connection_id) else {
            return false;
        };

        let Some(ref session) = conn.voice_session else {
            return false;
        };

        match &conn.active_chat_tab {
            // Channel name already includes the # prefix
            ChatTab::Channel(channel) => session.target.to_lowercase() == channel.to_lowercase(),
            ChatTab::UserMessage(nickname) => {
                session.target.to_lowercase() == nickname.to_lowercase()
            }
            ChatTab::Console => false,
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
                if let Some(conn) = self.connections.get_mut(&connection_id)
                    && let Some(ref mut session) = conn.voice_session
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
        // Send VoiceLeave to server if we still have an active session
        if let Some(conn) = self.connections.get_mut(&connection_id)
            && conn.voice_session.is_some()
        {
            let _ = conn.send(ClientMessage::VoiceLeave);
        }

        // Clean up local state
        self.cleanup_voice_session(connection_id);
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
    use super::*;

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
