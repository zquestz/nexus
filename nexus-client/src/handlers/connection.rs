//! Connection and chat message handlers

use iced::Task;
use iced::widget::{Id, operation, scrollable};
use nexus_common::protocol::ClientMessage;
use nexus_common::validators::{self, MessageError};

use crate::commands::{
    self, ParseResult, complete_channel, complete_command, complete_nickname, last_word,
};
use crate::i18n::{get_locale, t, t_args};
use crate::network::{ConnectionParams, ProxyConfig};
use crate::types::{
    ActivePanel, ChatMessage, ChatTab, InputId, Message, PendingRequests, ResponseRouting,
    ScrollableId, TabCompletionState,
};
use crate::views::constants::{PERMISSION_CHAT_SEND, PERMISSION_USER_MESSAGE};
use crate::{NexusApp, network};

/// Threshold for considering scroll position "at bottom" (0.0 = top, 1.0 = bottom)
const SCROLL_BOTTOM_THRESHOLD: f32 = 0.99;

impl NexusApp {
    // ==================== Connection Form Fields ====================

    /// Handle add bookmark checkbox toggle
    pub fn handle_add_bookmark_toggled(&mut self, enabled: bool) -> Task<Message> {
        self.connection_form.add_bookmark = enabled;
        Task::none()
    }

    /// Handle password field change
    pub fn handle_password_changed(&mut self, password: String) -> Task<Message> {
        self.connection_form.password = password;
        self.focused_field = InputId::Password;
        Task::none()
    }

    /// Handle port field change
    pub fn handle_port_changed(&mut self, port: u16) -> Task<Message> {
        self.connection_form.port = port;
        self.focused_field = InputId::Port;
        Task::none()
    }

    /// Handle server address field change
    pub fn handle_server_address_changed(&mut self, addr: String) -> Task<Message> {
        self.connection_form.server_address = addr;
        self.focused_field = InputId::ServerAddress;
        Task::none()
    }

    /// Handle server name field change
    pub fn handle_server_name_changed(&mut self, name: String) -> Task<Message> {
        self.connection_form.server_name = name;
        self.focused_field = InputId::ServerName;
        Task::none()
    }

    /// Handle username field change
    pub fn handle_username_changed(&mut self, username: String) -> Task<Message> {
        self.connection_form.username = username;
        self.focused_field = InputId::Username;
        Task::none()
    }

    /// Handle nickname field change
    pub fn handle_nickname_changed(&mut self, nickname: String) -> Task<Message> {
        self.connection_form.nickname = nickname;
        self.focused_field = InputId::Nickname;
        Task::none()
    }

    /// Handle fingerprint pin field change
    pub fn handle_fingerprint_changed(&mut self, fingerprint: String) -> Task<Message> {
        self.connection_form.fingerprint = fingerprint;
        self.focused_field = InputId::Fingerprint;
        Task::none()
    }

    // ==================== Connection Actions ====================

    /// Handle connect button press
    pub fn handle_connect_pressed(&mut self) -> Task<Message> {
        if self.connection_form.is_connecting {
            return Task::none();
        }

        // Run full field-shape validation (required fields, address
        // shape, optional username/nickname shape, password length cap,
        // fingerprint canonical form). Shared with the bookmark form
        // via `server_form_errors` so the two stay in lockstep.
        if let Err(error) = super::server_form_errors::translate_server_form_errors(
            &self.connection_form.server_name,
            &self.connection_form.server_address,
            &self.connection_form.username,
            &self.connection_form.nickname,
            &self.connection_form.password,
            &self.connection_form.fingerprint,
        ) {
            self.connection_form.error = Some(error);
            return Task::none();
        }

        // If the user checked "Add bookmark", the connect path will
        // auto-create a new bookmark on success. Validate the dedup
        // contract upstream so a duplicate bookmark can't be created
        // silently. On collision, the user can uncheck the box and
        // retry — connecting and bookmarking are explicit user
        // intents that shouldn't conflict.
        if self.connection_form.add_bookmark
            && let Some(error) = super::bookmarks::check_bookmark_dedup(
                &self.connection_form.server_name,
                &self.connection_form.server_address,
                self.connection_form.port,
                &self.connection_form.username,
                &self.connection_form.nickname,
                &self.config.bookmarks,
                None,
            )
        {
            self.connection_form.error = Some(error);
            return Task::none();
        }

        self.connection_form.error = None;

        // Normalize whitespace on identifying fields so lookups don't miss due
        // to user-typed leading/trailing whitespace. Password is left as-typed
        // since users could have intentional whitespace there.
        self.connection_form.server_name = self.connection_form.server_name.trim().to_string();
        self.connection_form.server_address =
            self.connection_form.server_address.trim().to_string();
        self.connection_form.username = self.connection_form.username.trim().to_string();
        self.connection_form.nickname = self.connection_form.nickname.trim().to_string();
        self.connection_form.fingerprint = self.connection_form.fingerprint.trim().to_string();

        let port = self.connection_form.port;

        self.connection_form.is_connecting = true;

        let server_address = self.connection_form.server_address.clone();
        let username = self.connection_form.username.clone();
        let password = self.connection_form.password.clone();
        // Use nickname from form, falling back to settings default
        let nickname = if self.connection_form.nickname.is_empty() {
            self.config.settings.nickname.clone()
        } else {
            Some(self.connection_form.nickname.clone())
        };
        let locale = get_locale().to_string();
        let avatar = self.config.settings.avatar.clone();
        let connection_id = self.next_connection_id;
        self.next_connection_id += 1;

        // Build proxy config if enabled
        let proxy = if self.config.settings.proxy.enabled {
            Some(ProxyConfig {
                address: self.config.settings.proxy.address.clone(),
                port: self.config.settings.proxy.port,
                username: self.config.settings.proxy.username.clone(),
                password: self.config.settings.proxy.password.clone(),
            })
        } else {
            None
        };

        // Stage 1 fingerprint pin priority:
        //   1. Form-supplied fingerprint (validated above as canonical).
        //      Lets the user pin a fingerprint received out-of-band before
        //      the first connect, including for servers they aren't
        //      bookmarking.
        //   2. Matching bookmark's stored fingerprint.
        //   3. None — TOFU.
        let expected_fingerprint = if !self.connection_form.fingerprint.is_empty() {
            Some(self.connection_form.fingerprint.clone())
        } else {
            self.config
                .find_bookmark_matching(
                    &server_address,
                    port,
                    &username,
                    &self.connection_form.nickname,
                )
                .and_then(|b| b.certificate_fingerprint.clone())
        };

        let params = ConnectionParams {
            server_address,
            port,
            username,
            password,
            nickname,
            locale,
            avatar,
            connection_id,
            proxy,
            expected_fingerprint,
        };
        // Clone for the result handler so an accept-after-mismatch can
        // replay the original intent.
        let retry_params = params.clone();

        Task::perform(
            async move { network::connect_to_server(params).await },
            move |result| Message::ConnectionResult {
                result,
                params: retry_params,
            },
        )
    }

    /// Disconnect from a server and clean up resources
    pub fn handle_disconnect_from_server(&mut self, connection_id: usize) -> Task<Message> {
        // Clean up voice session first (before removing connection)
        self.cleanup_voice_session(connection_id);

        if let Some(conn) = self.connections.remove(&connection_id) {
            let shutdown_arc = conn.shutdown_handle.clone();
            tokio::spawn(async move {
                let mut guard = shutdown_arc.lock().await;
                if let Some(shutdown) = guard.take() {
                    shutdown.shutdown();
                }
            });

            let conn_id = conn.connection_id;
            let registry = network::NETWORK_RECEIVERS.clone();
            tokio::spawn(async move {
                let mut receivers = registry.lock().await;
                receivers.remove(&conn_id);
            });

            // Clean up text editor content for this connection
            self.news_body_content.remove(&connection_id);

            // Clean up history key mapping (but keep the manager - it may be shared)
            self.connection_history_keys.remove(&connection_id);

            if self.active_connection == Some(connection_id) {
                self.active_connection = None;
            }

            // Update tray icon state (Windows/Linux only)
            #[cfg(not(target_os = "macos"))]
            self.update_tray_state();
        }
        Task::none()
    }

    /// Switch active view to a different connection
    pub fn handle_switch_to_connection(&mut self, connection_id: usize) -> Task<Message> {
        if !self.connections.contains_key(&connection_id) {
            return Task::none();
        };

        self.active_connection = Some(connection_id);

        // Scroll chat and focus input (app-wide panels like Settings/About persist)
        self.scroll_chat_if_visible(true)
    }

    // ==================== Chat Helpers ====================

    /// Scroll chat if chat view is visible (no panel overlay).
    ///
    /// Use this for background events (e.g., incoming messages) that shouldn't
    /// close panels or steal focus from panel input fields.
    ///
    /// If `focus` is true, also focuses the chat input field.
    pub fn scroll_chat_if_visible(&self, focus: bool) -> Task<Message> {
        // Don't scroll or steal focus if a panel is open
        if self.active_panel() != ActivePanel::None {
            return Task::none();
        }

        let scroll_state = self
            .active_connection
            .and_then(|id| self.connections.get(&id))
            .map(|conn| {
                conn.scroll_states
                    .get(&conn.active_chat_tab)
                    .copied()
                    .unwrap_or_default()
            })
            .unwrap_or_default();

        let scroll_offset = if scroll_state.auto_scroll {
            scrollable::RelativeOffset::END
        } else {
            scrollable::RelativeOffset {
                x: 0.0,
                y: scroll_state.offset,
            }
        };

        if focus {
            Task::batch([
                operation::snap_to(ScrollableId::ChatMessages, scroll_offset),
                operation::focus(Id::from(InputId::ChatInput)),
            ])
        } else {
            operation::snap_to(ScrollableId::ChatMessages, scroll_offset)
        }
    }

    // ==================== Chat Handlers ====================

    /// Handle chat scroll position change
    pub fn handle_chat_scrolled(
        &mut self,
        viewport: iced::widget::scrollable::Viewport,
    ) -> Task<Message> {
        // Only track scroll when chat view is active (no panel overlay)
        if self.active_panel() != ActivePanel::None {
            return Task::none();
        }

        if let Some(conn_id) = self.active_connection
            && let Some(conn) = self.connections.get_mut(&conn_id)
            && let Some(valid_offset) = Self::get_valid_scroll_offset(&viewport)
        {
            let tab = conn.active_chat_tab.clone();
            let scroll_state = conn.scroll_states.entry(tab).or_default();
            scroll_state.offset = valid_offset;
            scroll_state.auto_scroll = valid_offset >= SCROLL_BOTTOM_THRESHOLD;
        }
        Task::none()
    }

    /// Extract a valid scroll offset from a viewport, if applicable.
    ///
    /// Returns `None` when content fits in viewport (nothing to scroll).
    /// Spurious scroll events from panel transitions are handled separately
    /// via the panel check in `handle_chat_scrolled`.
    fn get_valid_scroll_offset(viewport: &iced::widget::scrollable::Viewport) -> Option<f32> {
        let bounds = viewport.bounds();
        let content_bounds = viewport.content_bounds();

        // Content fits in viewport - nothing to scroll, ignore event
        if content_bounds.height <= bounds.height {
            return None;
        }

        Some(viewport.relative_offset().y)
    }

    /// Close a channel tab by sending ChatLeave to server
    ///
    /// The tab is removed when the server responds with ChatLeaveResponse.
    pub fn handle_close_channel_tab(&mut self, channel: String) -> Task<Message> {
        let Some(conn_id) = self.active_connection else {
            return Task::none();
        };
        let Some(conn) = self.connections.get_mut(&conn_id) else {
            return Task::none();
        };

        // Prevent double-send
        if conn.pending_channel_leave.is_some() {
            return self.add_active_tab_message(
                conn_id,
                ChatMessage::error(t("err-leave-already-pending")),
            );
        }

        // Send ChatLeave to server
        let msg = ClientMessage::ChatLeave {
            channel: channel.clone(),
        };

        if let Err(e) = conn.send(msg) {
            let error_msg = t_args("err-failed-send-message", &[("error", &e.to_string())]);
            return self.add_active_tab_message(conn_id, ChatMessage::error(error_msg));
        }

        // Track pending leave
        conn.pending_channel_leave = Some(channel);

        Task::none()
    }

    /// Close a user message tab
    ///
    /// The `nickname` parameter is the display name (always populated; equals username for regular accounts).
    pub fn handle_close_user_message_tab(&mut self, nickname: String) -> Task<Message> {
        if let Some(conn_id) = self.active_connection
            && let Some(conn) = self.connections.get_mut(&conn_id)
        {
            // Remove from tab list
            conn.user_message_tabs.retain(|n| n != &nickname);

            // Remove message history
            conn.user_messages.remove(&nickname);

            let tab = ChatTab::UserMessage(nickname);
            conn.unread_tabs.remove(&tab);
            conn.scroll_states.remove(&tab);

            if conn.active_chat_tab == tab {
                // Move to previous tab in list (user message tabs are at the end)
                // Fall back to last channel tab, or console if no channels
                let prev_tab = if let Some(last_channel) = conn.channel_tabs.last() {
                    ChatTab::Channel(last_channel.clone())
                } else {
                    ChatTab::Console
                };
                conn.active_chat_tab = prev_tab;
                return self.handle_show_chat_view();
            }

            // Even when closing a non-active tab, we need to restore scroll position
            // because Iced may reset the scrollable when the tab bar re-renders
            return self.scroll_chat_if_visible(false);
        }
        Task::none()
    }

    /// Handle chat message input change
    ///
    /// Also resets tab completion state since the input has changed.
    pub fn handle_message_input_changed(&mut self, input: String) -> Task<Message> {
        if let Some(conn_id) = self.active_connection
            && let Some(conn) = self.connections.get_mut(&conn_id)
        {
            // Reset tab completion when input changes (user typed something)
            conn.tab_completion = None;
            conn.message_input = input;
        }
        self.focused_field = InputId::ChatInput;
        Task::none()
    }

    /// Resolved focused id for chat-view Tab. If the chat input was
    /// already focused, run nickname/channel/command completion. If
    /// focus had wandered elsewhere (clicked on a chat row, sidebar,
    /// etc.), refocus the chat input without manipulating its
    /// contents.
    pub fn handle_chat_tab_resolved(&mut self, focused: Id) -> Task<Message> {
        if focused == Id::from(InputId::ChatInput) {
            self.handle_chat_tab_complete()
        } else {
            self.focused_field = InputId::ChatInput;
            operation::focus(Id::from(InputId::ChatInput))
        }
    }

    /// Handle Tab key for nickname completion in chat
    ///
    /// Behavior:
    /// - If already completing: cycle to next match
    /// - If input is empty or ends with space: just focus the input
    /// - Otherwise: find word at end of input and complete it
    pub fn handle_chat_tab_complete(&mut self) -> Task<Message> {
        let Some(conn_id) = self.active_connection else {
            return Task::none();
        };
        let Some(conn) = self.connections.get_mut(&conn_id) else {
            return Task::none();
        };

        // If already in completion mode, cycle to next match
        if let Some(ref mut completion) = conn.tab_completion {
            // Defensive: clear stale state if input was emptied externally
            if conn.message_input.is_empty() {
                conn.tab_completion = None;
            } else {
                completion.index = (completion.index + 1) % completion.matches.len();
                conn.message_input.truncate(completion.start_pos);
                conn.message_input
                    .push_str(&completion.matches[completion.index]);
                return operation::move_cursor_to_end(Id::from(InputId::ChatInput));
            }
        }

        // If input is empty, just focus the field
        if conn.message_input.is_empty() {
            self.focused_field = InputId::ChatInput;
            return operation::focus(Id::from(InputId::ChatInput));
        }

        // Determine completion type based on input context
        let input = &conn.message_input;

        // Case 1: Command completion - input is "/" or "/prefix" with no space
        if input.starts_with('/') && !input.contains(' ') {
            if let Some(matches) = complete_command(&input[1..], conn.is_admin, &conn.permissions) {
                conn.message_input = format!("/{}", matches[0]);
                conn.tab_completion = Some(TabCompletionState::new(matches, 1)); // 1 to keep the /
                return operation::move_cursor_to_end(Id::from(InputId::ChatInput));
            }
            return Task::none();
        }

        // Find the word at the end of input
        let (start_pos, word) = last_word(input);

        // Case 2: Channel completion - word starts with #
        if word.starts_with('#') {
            if let Some(matches) = complete_channel(word, &conn.known_channels) {
                conn.message_input.truncate(start_pos);
                conn.message_input.push_str(&matches[0]);
                conn.tab_completion = Some(TabCompletionState::new(matches, start_pos));
                return operation::move_cursor_to_end(Id::from(InputId::ChatInput));
            }
            return Task::none();
        }

        // Case 3: Nickname completion - default
        if word.is_empty() {
            self.focused_field = InputId::ChatInput;
            return operation::focus(Id::from(InputId::ChatInput));
        }

        if let Some(matches) = complete_nickname(word, &conn.online_users, |u| &u.nickname) {
            conn.message_input.truncate(start_pos);
            conn.message_input.push_str(&matches[0]);
            conn.tab_completion = Some(TabCompletionState::new(matches, start_pos));
            return operation::move_cursor_to_end(Id::from(InputId::ChatInput));
        }

        Task::none()
    }

    /// Handle send chat message button press
    ///
    /// This method intercepts the input and checks for commands:
    /// - `/command` - Execute a client-side command
    /// - `//text` - Escape sequence, sends `/text` as a regular message
    /// - Regular text - Send as chat or private message
    pub fn handle_send_message_pressed(&mut self) -> Task<Message> {
        let Some(conn_id) = self.active_connection else {
            return Task::none();
        };
        let Some(conn) = self.connections.get(&conn_id) else {
            return Task::none();
        };

        let input = conn.message_input.clone();

        // Parse input for commands
        match commands::parse_input(&input) {
            ParseResult::Empty => Task::none(),
            ParseResult::Command(command) => {
                // Clear input and tab completion state, then execute command
                if let Some(conn) = self.connections.get_mut(&conn_id) {
                    conn.message_input.clear();
                    conn.tab_completion = None;
                }
                commands::execute_command(self, conn_id, command)
            }
            ParseResult::Message(message, action) => {
                // Console tab doesn't allow plain text - show specific error
                if conn.active_chat_tab == ChatTab::Console {
                    return self.add_chat_error(
                        conn_id,
                        t_args("err-console-no-send", &[("join", "join"), ("msg", "msg")]),
                    );
                }

                // Check permission before sending
                let has_permission = match &conn.active_chat_tab {
                    // Handled by the early return above; defensive fallback
                    // re-emits the same error rather than panicking, in case
                    // a future refactor moves the guard.
                    ChatTab::Console => {
                        return self.add_chat_error(
                            conn_id,
                            t_args("err-console-no-send", &[("join", "join"), ("msg", "msg")]),
                        );
                    }
                    ChatTab::Channel(_) => conn.has_permission(PERMISSION_CHAT_SEND),
                    ChatTab::UserMessage(_) => conn.has_permission(PERMISSION_USER_MESSAGE),
                };

                if !has_permission {
                    return self.add_chat_error(conn_id, t("err-no-chat-permission"));
                }

                // Validate message content using shared validators
                if let Err(e) = validators::validate_message(&message) {
                    let error_msg = match e {
                        MessageError::Empty => t("err-message-empty"),
                        MessageError::TooLong => t_args(
                            "err-message-too-long",
                            &[
                                ("length", &message.chars().count().to_string()),
                                ("max", &validators::MAX_MESSAGE_LENGTH.to_string()),
                            ],
                        ),
                        MessageError::ContainsNewlines => t("err-message-contains-newlines"),
                        MessageError::InvalidCharacters => t("err-message-invalid-characters"),
                    };
                    return self.add_chat_error(conn_id, error_msg);
                }

                // Re-borrow conn after potential mutable borrow above
                let Some(conn) = self.connections.get(&conn_id) else {
                    return Task::none();
                };

                let (msg, pm_nickname) = match &conn.active_chat_tab {
                    // Handled by the early return at the start of this
                    // branch; defensive fallback re-emits the same error
                    // rather than panicking, in case a future refactor
                    // moves the guard.
                    ChatTab::Console => {
                        return self.add_chat_error(
                            conn_id,
                            t_args("err-console-no-send", &[("join", "join"), ("msg", "msg")]),
                        );
                    }
                    ChatTab::Channel(channel) => (
                        ClientMessage::ChatSend {
                            message,
                            action,
                            channel: channel.clone(),
                        },
                        None,
                    ),
                    ChatTab::UserMessage(nickname) => (
                        ClientMessage::UserMessage {
                            to_nickname: nickname.clone(),
                            message,
                            action,
                        },
                        Some(nickname.clone()),
                    ),
                };

                let send_result = conn.send(msg);

                match send_result {
                    Ok(message_id) => {
                        if let Some(conn) = self.connections.get_mut(&conn_id) {
                            conn.message_input.clear();
                            conn.tab_completion = None;

                            // Track PM messages so errors go to the correct tab
                            if let Some(nickname) = pm_nickname {
                                conn.pending_requests.track(
                                    message_id,
                                    ResponseRouting::ShowErrorInMessageTab(nickname),
                                );
                            }
                        }
                        Task::none()
                    }
                    Err(e) => {
                        let error_msg = format!("{}: {}", t("err-send-failed"), e);
                        self.add_chat_error(conn_id, error_msg)
                    }
                }
            }
        }
    }

    /// Switch to a different chat tab (Server or UserMessage)
    pub fn handle_switch_chat_tab(&mut self, tab: ChatTab) -> Task<Message> {
        let Some(conn_id) = self.active_connection else {
            return Task::none();
        };
        let Some(conn) = self.connections.get_mut(&conn_id) else {
            return Task::none();
        };

        // Check if we're clearing a user message unread (for tray update)
        #[cfg(not(target_os = "macos"))]
        let was_unread_user_message =
            matches!(&tab, ChatTab::UserMessage(_)) && conn.unread_tabs.contains(&tab);

        conn.unread_tabs.remove(&tab);
        conn.active_chat_tab = tab;

        // Update tray icon state if we cleared a user message unread (Windows/Linux only)
        #[cfg(not(target_os = "macos"))]
        if was_unread_user_message {
            self.update_tray_state();
        }

        self.handle_show_chat_view()
    }

    // ==================== Tab Navigation ====================

    /// Handle Tab pressed in connection form. Queries iced for the
    /// actually-focused widget id and forwards to
    /// [`Self::handle_connection_form_tab_resolved`] for cycling.
    pub fn handle_connection_form_tab_pressed(&mut self) -> Task<Message> {
        super::focus::dispatch_find_focused(Message::ConnectionFormTabResolved)
    }

    pub fn handle_connection_form_tab_resolved(&mut self, focused: Id) -> Task<Message> {
        // Note: Port is skipped because NumberInput handles its own Tab key.
        const CYCLE: &[InputId] = &[
            InputId::ServerName,
            InputId::ServerAddress,
            InputId::Username,
            InputId::Password,
            InputId::Nickname,
            InputId::Fingerprint,
        ];
        let next = super::focus::next_in_cycle(&focused, CYCLE);
        self.focused_field = next;
        operation::focus(Id::from(next))
    }

    // ==================== Private Helpers ====================

    /// Add an error message to the chat
    fn add_chat_error(&mut self, connection_id: usize, message: String) -> Task<Message> {
        self.add_active_tab_message(connection_id, ChatMessage::error(message))
    }
}
