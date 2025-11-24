//! Keyboard navigation

use crate::NexusApp;
use crate::types::{BookmarkEditMode, InputId, Message, UserEditState};
use iced::keyboard::{self, key};
use iced::widget::text_input;
use iced::{Event, Task};

impl NexusApp {
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
            self.focused_field = next_field.clone();
            return text_input::focus(text_input::Id::from(next_field));
        } else if self.ui_state.show_add_user {
            // On add user screen, cycle through fields
            let next_field = match self.focused_field {
                InputId::AdminUsername => InputId::AdminPassword,
                InputId::AdminPassword => InputId::AdminUsername,
                _ => InputId::AdminUsername,
            };
            self.focused_field = next_field.clone();
            return text_input::focus(text_input::Id::from(next_field));
        } else if self.ui_state.show_edit_user {
            // On edit user screen, handle both stages
            if let Some(conn_id) = self.active_connection {
                if let Some(conn) = self.connections.get(&conn_id) {
                    match &conn.user_management.edit_state {
                        UserEditState::SelectingUser { .. } => {
                            // Stage 1: Only username field
                            self.focused_field = InputId::EditUsername;
                            return text_input::focus(text_input::Id::from(InputId::EditUsername));
                        }
                        UserEditState::EditingUser { .. } => {
                            // Stage 2: Cycle through edit fields
                            let next_field = match self.focused_field {
                                InputId::EditNewUsername => InputId::EditNewPassword,
                                InputId::EditNewPassword => InputId::EditNewUsername,
                                _ => InputId::EditNewUsername,
                            };
                            self.focused_field = next_field.clone();
                            return text_input::focus(text_input::Id::from(next_field));
                        }
                        UserEditState::None => {}
                    }
                }
            }
        } else if self.ui_state.show_broadcast {
            // Broadcast screen only has one field, so focus stays
            self.focused_field = InputId::BroadcastMessage;
            return text_input::focus(text_input::Id::from(InputId::BroadcastMessage));
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
            self.focused_field = next_field.clone();
            return text_input::focus(text_input::Id::from(next_field));
        }
        Task::none()
    }

    /// Handle keyboard events (Tab, Enter, Escape)
    pub fn handle_keyboard_event(&mut self, event: Event) -> Task<Message> {
        // Handle Tab key
        if let Event::Keyboard(keyboard::Event::KeyPressed {
            key: keyboard::Key::Named(key::Named::Tab),
            modifiers,
            ..
        }) = event
        {
            if !modifiers.shift() {
                return self.update(Message::TabPressed);
            }
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
            } else if self.ui_state.show_add_user {
                // On add user screen, try to create user
                if let Some(conn_id) = self.active_connection {
                    if let Some(conn) = self.connections.get(&conn_id) {
                        let can_create = !conn.user_management.username.trim().is_empty()
                            && !conn.user_management.password.trim().is_empty();
                        if can_create {
                            return self.update(Message::CreateUserPressed);
                        }
                    }
                }
            } else if self.ui_state.show_edit_user {
                // On edit user screen, handle Enter for both stages
                if let Some(conn_id) = self.active_connection {
                    if let Some(conn) = self.connections.get(&conn_id) {
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
                }
            } else if self.ui_state.show_broadcast {
                // On broadcast screen, try to send broadcast
                if let Some(conn_id) = self.active_connection {
                    if let Some(conn) = self.connections.get(&conn_id) {
                        let can_send = !conn.broadcast_message.trim().is_empty();
                        if can_send {
                            return self.update(Message::SendBroadcastPressed);
                        }
                    }
                }
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
            } else if self.ui_state.show_add_user
                || self.ui_state.show_edit_user
                || self.ui_state.show_broadcast
            {
                // Cancel add/edit user screens or broadcast
                if self.ui_state.show_add_user {
                    return self.update(Message::ToggleAddUser);
                }
                if self.ui_state.show_edit_user {
                    return self.update(Message::CancelEditUser);
                }
                if self.ui_state.show_broadcast {
                    return self.update(Message::ToggleBroadcast);
                }
            }
        }
        Task::none()
    }
}
