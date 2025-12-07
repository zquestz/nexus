//! Keyboard navigation

use crate::NexusApp;
use crate::types::{ActivePanel, BookmarkEditMode, ChatTab, InputId, Message, UserEditState};
use iced::keyboard::{self, key};
use iced::widget::{Id, operation};
use iced::{Event, Task};

impl NexusApp {
    /// Handle keyboard events (Tab, Enter, Escape)
    pub fn handle_keyboard_event(&mut self, event: Event) -> Task<Message> {
        // Handle Cmd/Ctrl+Shift+Tab for previous chat tab (must be before plain Tab check)
        if let Event::Keyboard(keyboard::Event::KeyPressed {
            key: keyboard::Key::Named(key::Named::Tab),
            modifiers,
            ..
        }) = event
            && modifiers.command()
            && modifiers.shift()
        {
            return self.update(Message::PrevChatTab);
        }
        // Handle Cmd/Ctrl+Tab for next chat tab (must be before plain Tab check)
        if let Event::Keyboard(keyboard::Event::KeyPressed {
            key: keyboard::Key::Named(key::Named::Tab),
            modifiers,
            ..
        }) = event
            && modifiers.command()
            && !modifiers.shift()
        {
            return self.update(Message::NextChatTab);
        }

        // Handle plain Tab key for field cycling
        if let Event::Keyboard(keyboard::Event::KeyPressed {
            key: keyboard::Key::Named(key::Named::Tab),
            modifiers,
            ..
        }) = event
            && !modifiers.command()
            && !modifiers.shift()
        {
            return self.update(Message::TabPressed);
        }
        // Handle Enter key
        if let Event::Keyboard(keyboard::Event::KeyPressed {
            key: keyboard::Key::Named(key::Named::Enter),
            ..
        }) = event
        {
            if self.bookmark_edit.mode != BookmarkEditMode::None {
                // On bookmark edit screen, try to save
                let can_save = !self.bookmark_edit.bookmark.name.trim().is_empty()
                    && !self.bookmark_edit.bookmark.address.trim().is_empty()
                    && !self.bookmark_edit.bookmark.port.trim().is_empty();
                if can_save {
                    return self.update(Message::SaveBookmark);
                }
            } else if self.ui_state.active_panel == ActivePanel::AddUser {
                // On add user screen, try to create user
                if let Some(conn_id) = self.active_connection
                    && let Some(conn) = self.connections.get(&conn_id)
                {
                    let can_create = !conn.user_management.username.trim().is_empty()
                        && !conn.user_management.password.trim().is_empty();
                    if can_create {
                        return self.update(Message::CreateUserPressed);
                    }
                }
            } else if self.ui_state.active_panel == ActivePanel::EditUser {
                // On edit user screen, handle Enter for both stages
                if let Some(conn_id) = self.active_connection
                    && let Some(conn) = self.connections.get(&conn_id)
                {
                    match &conn.user_management.edit_state {
                        UserEditState::SelectingUser { username } => {
                            // Stage 1: Submit edit request
                            if !username.trim().is_empty() {
                                return self.update(Message::EditUserPressed);
                            }
                        }
                        UserEditState::EditingUser { new_username, .. } => {
                            // Stage 2: Submit update
                            if !new_username.trim().is_empty() {
                                return self.update(Message::UpdateUserPressed);
                            }
                        }
                        UserEditState::None => {}
                    }
                }
            } else if self.ui_state.active_panel == ActivePanel::Broadcast {
                // On broadcast screen, try to send broadcast
                if let Some(conn_id) = self.active_connection
                    && let Some(conn) = self.connections.get(&conn_id)
                {
                    let can_send = !conn.broadcast_message.trim().is_empty();
                    if can_send {
                        return self.update(Message::SendBroadcastPressed);
                    }
                }
            } else if self.ui_state.active_panel == ActivePanel::About {
                // On about screen, close the panel
                return self.update(Message::CloseAbout);
            } else if self.ui_state.active_panel == ActivePanel::ServerInfo {
                // On server info screen, close the panel
                return self.update(Message::CloseServerInfo);
            } else if self.active_connection.is_none() {
                // On connection screen, try to connect
                let can_connect = !self.connection_form.server_address.trim().is_empty()
                    && !self.connection_form.port.trim().is_empty()
                    && !self.connection_form.username.trim().is_empty()
                    && !self.connection_form.password.trim().is_empty();
                if can_connect {
                    return self.update(Message::ConnectPressed);
                }
            }
        }
        // Handle Escape key
        if let Event::Keyboard(keyboard::Event::KeyPressed {
            key: keyboard::Key::Named(key::Named::Escape),
            ..
        }) = event
        {
            if self.bookmark_edit.mode != BookmarkEditMode::None {
                // Cancel bookmark edit
                return self.update(Message::CancelBookmarkEdit);
            } else {
                // Cancel active panel
                match self.ui_state.active_panel {
                    ActivePanel::About => return self.update(Message::CloseAbout),
                    ActivePanel::AddUser => return self.update(Message::CancelAddUser),
                    ActivePanel::EditUser => return self.update(Message::CancelEditUser),
                    ActivePanel::Broadcast => return self.update(Message::CancelBroadcast),
                    ActivePanel::Settings => return self.update(Message::CancelSettings),
                    ActivePanel::ServerInfo => return self.update(Message::CloseServerInfo),
                    ActivePanel::None => {}
                }
            }
        }
        Task::none()
    }

    /// Navigate to the next chat tab (wraps around)
    pub fn handle_next_chat_tab(&mut self) -> Task<Message> {
        let Some(conn_id) = self.active_connection else {
            return Task::none();
        };
        let Some(conn) = self.connections.get(&conn_id) else {
            return Task::none();
        };

        // Build ordered list of tabs: Server first, then PMs alphabetically
        let mut tabs = vec![ChatTab::Server];
        let mut pm_usernames: Vec<String> = conn.user_messages.keys().cloned().collect();
        pm_usernames.sort();
        for username in pm_usernames {
            tabs.push(ChatTab::UserMessage(username));
        }

        // Find current tab index and move to next (with wrap)
        let current_index = tabs
            .iter()
            .position(|t| *t == conn.active_chat_tab)
            .unwrap_or(0);
        let next_index = (current_index + 1) % tabs.len();
        let next_tab = tabs[next_index].clone();

        self.update(Message::SwitchChatTab(next_tab))
    }

    /// Navigate to the previous chat tab (wraps around)
    pub fn handle_prev_chat_tab(&mut self) -> Task<Message> {
        let Some(conn_id) = self.active_connection else {
            return Task::none();
        };
        let Some(conn) = self.connections.get(&conn_id) else {
            return Task::none();
        };

        // Build ordered list of tabs: Server first, then PMs alphabetically
        let mut tabs = vec![ChatTab::Server];
        let mut pm_usernames: Vec<String> = conn.user_messages.keys().cloned().collect();
        pm_usernames.sort();
        for username in pm_usernames {
            tabs.push(ChatTab::UserMessage(username));
        }

        // Find current tab index and move to previous (with wrap)
        let current_index = tabs
            .iter()
            .position(|t| *t == conn.active_chat_tab)
            .unwrap_or(0);
        let prev_index = if current_index == 0 {
            tabs.len() - 1
        } else {
            current_index - 1
        };
        let prev_tab = tabs[prev_index].clone();

        self.update(Message::SwitchChatTab(prev_tab))
    }

    /// Handle Tab key navigation across different screens
    pub fn handle_tab_navigation(&mut self) -> Task<Message> {
        if self.bookmark_edit.mode != BookmarkEditMode::None {
            // On bookmark edit screen, cycle through fields
            let next_field = match self.focused_field {
                InputId::BookmarkName => InputId::BookmarkAddress,
                InputId::BookmarkAddress => InputId::BookmarkPort,
                InputId::BookmarkPort => InputId::BookmarkUsername,
                InputId::BookmarkUsername => InputId::BookmarkPassword,
                InputId::BookmarkPassword => InputId::BookmarkName,
                _ => InputId::BookmarkName,
            };
            self.focused_field = next_field;
            return operation::focus(Id::from(next_field));
        } else if self.ui_state.active_panel == ActivePanel::AddUser {
            // On add user screen, cycle through fields
            let next_field = match self.focused_field {
                InputId::AdminUsername => InputId::AdminPassword,
                InputId::AdminPassword => InputId::AdminUsername,
                _ => InputId::AdminUsername,
            };
            self.focused_field = next_field;
            return operation::focus(Id::from(next_field));
        } else if self.ui_state.active_panel == ActivePanel::EditUser {
            // On edit user screen, handle both stages
            if let Some(conn_id) = self.active_connection
                && let Some(conn) = self.connections.get(&conn_id)
            {
                match &conn.user_management.edit_state {
                    UserEditState::SelectingUser { .. } => {
                        // Stage 1: Only username field
                        self.focused_field = InputId::EditUsername;
                        return operation::focus(Id::from(InputId::EditUsername));
                    }
                    UserEditState::EditingUser { .. } => {
                        // Stage 2: Cycle through edit fields
                        let next_field = match self.focused_field {
                            InputId::EditNewUsername => InputId::EditNewPassword,
                            InputId::EditNewPassword => InputId::EditNewUsername,
                            _ => InputId::EditNewUsername,
                        };
                        self.focused_field = next_field;
                        return operation::focus(Id::from(next_field));
                    }
                    UserEditState::None => {}
                }
            }
        } else if self.ui_state.active_panel == ActivePanel::Broadcast {
            // Broadcast screen only has one field, so focus stays
            self.focused_field = InputId::BroadcastMessage;
            return operation::focus(Id::from(InputId::BroadcastMessage));
        } else if self.ui_state.active_panel == ActivePanel::Settings {
            // Settings panel has no text inputs yet, just return
            return Task::none();
        } else if self.active_connection.is_some() {
            // In chat view, Tab refocuses the chat input
            self.focused_field = InputId::ChatInput;
            return operation::focus(Id::from(InputId::ChatInput));
        } else if self.active_connection.is_none() {
            // On connection screen, cycle through fields
            let next_field = match self.focused_field {
                InputId::ServerName => InputId::ServerAddress,
                InputId::ServerAddress => InputId::Port,
                InputId::Port => InputId::Username,
                InputId::Username => InputId::Password,
                InputId::Password => InputId::ServerName,
                _ => InputId::ServerName,
            };
            self.focused_field = next_field;
            return operation::focus(Id::from(next_field));
        }
        Task::none()
    }
}
