//! Connection and chat message handlers

use crate::commands::{self, ParseResult};
use crate::i18n::{get_locale, t, t_args};
use crate::network::{ConnectionParams, ProxyConfig};
use crate::types::{
    ActivePanel, ChatMessage, ChatTab, InputId, Message, PendingRequests, ResponseRouting,
    ScrollableId, TabCompletionState,
};
use crate::views::constants::{PERMISSION_CHAT_SEND, PERMISSION_USER_MESSAGE};
use crate::{NexusApp, network};
use iced::Task;
use iced::widget::{Id, operation, scrollable};
use nexus_common::protocol::ClientMessage;
use nexus_common::validators::{self, MessageError};

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
        self.connection_form.error = None;
        self.focused_field = InputId::Password;
        Task::none()
    }

    /// Handle port field change
    pub fn handle_port_changed(&mut self, port: u16) -> Task<Message> {
        self.connection_form.port = port;
        self.connection_form.error = None;
        self.focused_field = InputId::Port;
        Task::none()
    }

    /// Handle server address field change
    pub fn handle_server_address_changed(&mut self, addr: String) -> Task<Message> {
        self.connection_form.server_address = addr;
        self.connection_form.error = None;
        self.focused_field = InputId::ServerAddress;
        Task::none()
    }

    /// Handle server name field change
    pub fn handle_server_name_changed(&mut self, name: String) -> Task<Message> {
        self.connection_form.server_name = name;
        self.connection_form.error = None;
        self.focused_field = InputId::ServerName;
        Task::none()
    }

    /// Handle username field change
    pub fn handle_username_changed(&mut self, username: String) -> Task<Message> {
        self.connection_form.username = username;
        self.connection_form.error = None;
        self.focused_field = InputId::Username;
        Task::none()
    }

    /// Handle nickname field change
    pub fn handle_nickname_changed(&mut self, nickname: String) -> Task<Message> {
        self.connection_form.nickname = nickname;
        self.connection_form.error = None;
        self.focused_field = InputId::Nickname;
        Task::none()
    }

    // ==================== Connection Actions ====================

    /// Handle connect button press
    pub fn handle_connect_pressed(&mut self) -> Task<Message> {
        if self.connection_form.is_connecting {
            return Task::none();
        }

        self.connection_form.error = None;

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

        Task::perform(
            async move {
                network::connect_to_server(ConnectionParams {
                    server_address,
                    port,
                    username,
                    password,
                    nickname,
                    locale,
                    avatar,
                    connection_id,
                    proxy,
                })
                .await
            },
            Message::ConnectionResult,
        )
    }

    /// Disconnect from a server and clean up resources
    pub fn handle_disconnect_from_server(&mut self, connection_id: usize) -> Task<Message> {
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

            if self.active_connection == Some(connection_id) {
                self.active_connection = None;
            }
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

    /// Close a user message tab
    ///
    /// The `nickname` parameter is the display name (always populated; equals username for regular accounts).
    pub fn handle_close_user_message_tab(&mut self, nickname: String) -> Task<Message> {
        if let Some(conn_id) = self.active_connection
            && let Some(conn) = self.connections.get_mut(&conn_id)
        {
            conn.user_messages.remove(&nickname);

            let tab = ChatTab::UserMessage(nickname);
            conn.unread_tabs.remove(&tab);
            conn.scroll_states.remove(&tab);

            if conn.active_chat_tab == tab {
                conn.active_chat_tab = ChatTab::Server;
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

        // If input is empty or ends with whitespace, just focus the field
        if conn.message_input.is_empty() || conn.message_input.ends_with(char::is_whitespace) {
            self.focused_field = InputId::ChatInput;
            return operation::focus(Id::from(InputId::ChatInput));
        }

        // Find the word at the end of input (the prefix to complete)
        let start_pos = conn
            .message_input
            .rfind(char::is_whitespace)
            .map_or(0, |i| i + 1);
        let prefix_lower = conn.message_input[start_pos..].to_lowercase();

        // Find matching nicknames (case-insensitive prefix match)
        let mut matches: Vec<String> = conn
            .online_users
            .iter()
            .filter(|u| u.nickname.to_lowercase().starts_with(&prefix_lower))
            .map(|u| u.nickname.clone())
            .collect();

        if matches.is_empty() {
            return Task::none();
        }

        // Sort matches alphabetically for consistent ordering
        matches.sort_unstable_by(|a, b| a.to_lowercase().cmp(&b.to_lowercase()));

        // Apply first match using truncate-and-append
        conn.message_input.truncate(start_pos);
        conn.message_input.push_str(&matches[0]);

        // Store completion state for cycling
        conn.tab_completion = Some(TabCompletionState::new(matches, start_pos));

        operation::move_cursor_to_end(Id::from(InputId::ChatInput))
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
            ParseResult::Message(message) => {
                // Check permission before sending
                let has_permission = match &conn.active_chat_tab {
                    ChatTab::Server => conn.has_permission(PERMISSION_CHAT_SEND),
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
                                ("length", &message.len().to_string()),
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
                    ChatTab::Server => (ClientMessage::ChatSend { message }, None),
                    ChatTab::UserMessage(nickname) => (
                        ClientMessage::UserMessage {
                            to_nickname: nickname.clone(),
                            message,
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

        conn.unread_tabs.remove(&tab);
        conn.active_chat_tab = tab;

        self.handle_show_chat_view()
    }

    // ==================== Tab Navigation ====================

    /// Handle Tab pressed in connection form
    ///
    /// Checks which field is actually focused using async operations,
    /// then moves to the next field in sequence.
    pub fn handle_connection_form_tab_pressed(&mut self) -> Task<Message> {
        // Check focus state of all six connection form fields in parallel
        let check_name = operation::is_focused(Id::from(InputId::ServerName));
        let check_address = operation::is_focused(Id::from(InputId::ServerAddress));
        let check_port = operation::is_focused(Id::from(InputId::Port));
        let check_username = operation::is_focused(Id::from(InputId::Username));
        let check_password = operation::is_focused(Id::from(InputId::Password));
        let check_nickname = operation::is_focused(Id::from(InputId::Nickname));

        // Batch the checks and combine results
        Task::batch([
            check_name.map(|focused| (0, focused)),
            check_address.map(|focused| (1, focused)),
            check_port.map(|focused| (2, focused)),
            check_username.map(|focused| (3, focused)),
            check_password.map(|focused| (4, focused)),
            check_nickname.map(|focused| (5, focused)),
        ])
        .collect()
        .map(|results: Vec<(u8, bool)>| {
            let name_focused = results.iter().any(|(i, f)| *i == 0 && *f);
            let address_focused = results.iter().any(|(i, f)| *i == 1 && *f);
            let port_focused = results.iter().any(|(i, f)| *i == 2 && *f);
            let username_focused = results.iter().any(|(i, f)| *i == 3 && *f);
            let password_focused = results.iter().any(|(i, f)| *i == 4 && *f);
            let nickname_focused = results.iter().any(|(i, f)| *i == 5 && *f);
            Message::ConnectionFormFocusResult(
                name_focused,
                address_focused,
                port_focused,
                username_focused,
                password_focused,
                nickname_focused,
            )
        })
    }

    /// Handle focus check result for connection form Tab navigation
    pub fn handle_connection_form_focus_result(
        &mut self,
        name_focused: bool,
        address_focused: bool,
        port_focused: bool,
        username_focused: bool,
        password_focused: bool,
        nickname_focused: bool,
    ) -> Task<Message> {
        // Determine next field based on which is currently focused
        // Note: Port is skipped because NumberInput handles its own Tab key
        let next_field = if name_focused {
            InputId::ServerAddress
        } else if address_focused {
            // Skip Port (NumberInput)
            InputId::Username
        } else if port_focused {
            InputId::Username
        } else if username_focused {
            InputId::Password
        } else if password_focused {
            InputId::Nickname
        } else if nickname_focused {
            // Wrap around to first field
            InputId::ServerName
        } else {
            // None focused, start at first field
            InputId::ServerName
        };

        self.focused_field = next_field;
        operation::focus(Id::from(next_field))
    }

    // ==================== Private Helpers ====================

    /// Add an error message to the chat
    fn add_chat_error(&mut self, connection_id: usize, message: String) -> Task<Message> {
        self.add_chat_message(connection_id, ChatMessage::error(message))
    }
}
