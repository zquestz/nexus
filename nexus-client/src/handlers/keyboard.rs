//! Keyboard navigation

use crate::NexusApp;
use crate::types::{
    ActivePanel, BookmarkEditMode, ChatTab, InputId, Message, NewsManagementMode,
    UserManagementMode,
};
use iced::keyboard::{self, key};
use iced::widget::{Id, operation};
use iced::{Event, Task};

impl NexusApp {
    /// Handle keyboard events (Tab, Enter, Escape, F5)
    pub fn handle_keyboard_event(&mut self, event: Event) -> Task<Message> {
        // Handle F5 for refresh in Files panel
        if let Event::Keyboard(keyboard::Event::KeyPressed {
            key: keyboard::Key::Named(key::Named::F5),
            ..
        }) = event
            && self.active_panel() == ActivePanel::Files
        {
            return self.update(Message::FileRefresh);
        }

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
                    && !self.bookmark_edit.bookmark.address.trim().is_empty();
                if can_save {
                    return self.update(Message::SaveBookmark);
                }
            } else if self.active_panel() == ActivePanel::UserManagement {
                // On user management screen, handle Enter based on mode
                if let Some(conn_id) = self.active_connection
                    && let Some(conn) = self.connections.get(&conn_id)
                {
                    match &conn.user_management.mode {
                        UserManagementMode::Create => {
                            // Create mode: Submit create request
                            let can_create = !conn.user_management.username.trim().is_empty()
                                && !conn.user_management.password.trim().is_empty();
                            if can_create {
                                return self.update(Message::UserManagementCreatePressed);
                            }
                        }
                        UserManagementMode::Edit { new_username, .. } => {
                            // Edit mode: Submit update
                            if !new_username.trim().is_empty() {
                                return self.update(Message::UserManagementUpdatePressed);
                            }
                        }
                        UserManagementMode::List => {
                            // List mode: No Enter action (use Escape to close)
                        }
                        UserManagementMode::ConfirmDelete { .. } => {
                            // ConfirmDelete: No Enter action (user must click button)
                        }
                    }
                }
            } else if self.active_panel() == ActivePanel::Broadcast {
                // On broadcast screen, try to send broadcast
                if let Some(conn_id) = self.active_connection
                    && let Some(conn) = self.connections.get(&conn_id)
                {
                    let can_send = !conn.broadcast_message.trim().is_empty();
                    if can_send {
                        return self.update(Message::SendBroadcastPressed);
                    }
                }
            } else if self.active_panel() == ActivePanel::Settings {
                // On settings screen, save settings
                return self.update(Message::SaveSettings);
            } else if self.active_panel() == ActivePanel::About {
                // On about screen, close the panel
                return self.update(Message::CloseAbout);
            } else if self.active_panel() == ActivePanel::ServerInfo {
                // On server info screen, submit if in edit mode, otherwise close
                if let Some(conn_id) = self.active_connection
                    && let Some(conn) = self.connections.get(&conn_id)
                    && conn.server_info_edit.is_some()
                {
                    return self.update(Message::UpdateServerInfoPressed);
                }
                return self.update(Message::CloseServerInfo);
            } else if self.active_panel() == ActivePanel::UserInfo {
                // On user info screen, close the panel
                return self.update(Message::CloseUserInfo);
            } else if self.active_panel() == ActivePanel::ChangePassword {
                // On change password screen, submit if all fields are filled
                if let Some(conn_id) = self.active_connection
                    && let Some(conn) = self.connections.get(&conn_id)
                    && let Some(state) = &conn.password_change_state
                {
                    let can_save = !state.current_password.is_empty()
                        && !state.new_password.is_empty()
                        && !state.confirm_password.is_empty();
                    if can_save {
                        return self.update(Message::ChangePasswordSavePressed);
                    }
                }
                return Task::none();
            } else if self.active_panel() == ActivePanel::News {
                // On news screen, handle Enter based on mode
                if let Some(conn_id) = self.active_connection
                    && let Some(conn) = self.connections.get(&conn_id)
                {
                    match &conn.news_management.mode {
                        NewsManagementMode::Create | NewsManagementMode::Edit { .. } => {
                            // Create/Edit mode: Don't submit on Enter - the text_editor
                            // uses Enter for newlines. Users must click the submit button.
                        }
                        NewsManagementMode::List => {
                            // List mode: No Enter action (use Escape to close)
                        }
                        NewsManagementMode::ConfirmDelete { .. } => {
                            // ConfirmDelete: No Enter action (user must click button)
                        }
                    }
                }
            } else if self.active_connection.is_none() {
                // On connection screen, try to connect
                let can_connect = !self.connection_form.server_name.trim().is_empty()
                    && !self.connection_form.server_address.trim().is_empty();
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
                match self.active_panel() {
                    ActivePanel::About => return self.update(Message::CloseAbout),
                    ActivePanel::UserManagement => {
                        // In user management, Escape returns to list (or closes if on list)
                        return self.update(Message::CancelUserManagement);
                    }
                    ActivePanel::Broadcast => return self.update(Message::CancelBroadcast),
                    ActivePanel::Settings => return self.update(Message::CancelSettings),
                    ActivePanel::ServerInfo => {
                        // If in edit mode, cancel edit; otherwise close panel
                        if let Some(conn_id) = self.active_connection
                            && let Some(conn) = self.connections.get(&conn_id)
                            && conn.server_info_edit.is_some()
                        {
                            return self.update(Message::CancelEditServerInfo);
                        }
                        return self.update(Message::CloseServerInfo);
                    }
                    ActivePanel::UserInfo => return self.update(Message::CloseUserInfo),
                    ActivePanel::ChangePassword => {
                        return self.update(Message::ChangePasswordCancelPressed);
                    }
                    ActivePanel::News => {
                        // In news panel, Escape returns to list (or closes if on list)
                        return self.update(Message::CancelNews);
                    }
                    ActivePanel::Files => {
                        // If delete confirmation is showing, cancel it
                        if let Some(conn_id) = self.active_connection
                            && let Some(conn) = self.connections.get(&conn_id)
                            && conn.files_management.pending_delete.is_some()
                        {
                            return self.update(Message::FileCancelDelete);
                        }
                        // If creating directory, close dialog
                        if let Some(conn_id) = self.active_connection
                            && let Some(conn) = self.connections.get(&conn_id)
                            && conn.files_management.creating_directory
                        {
                            return self.update(Message::FileNewDirectoryCancel);
                        }
                        // Otherwise close panel
                        return self.update(Message::CancelFiles);
                    }
                    ActivePanel::None => {}
                }
            }
        }
        Task::none()
    }

    /// Navigate to the next chat tab (wraps around)
    ///
    /// Only works when chat is visible (no panel active).
    pub fn handle_next_chat_tab(&mut self) -> Task<Message> {
        // Only switch chat tabs when chat is visible (no panel active)
        if self.active_panel() != ActivePanel::None {
            return Task::none();
        }

        let Some(conn_id) = self.active_connection else {
            return Task::none();
        };
        let Some(conn) = self.connections.get(&conn_id) else {
            return Task::none();
        };

        // Build ordered list of tabs: Server first, then PMs alphabetically
        let mut tabs = vec![ChatTab::Server];
        let mut pm_nicknames: Vec<String> = conn.user_messages.keys().cloned().collect();
        pm_nicknames.sort();
        for nickname in pm_nicknames {
            tabs.push(ChatTab::UserMessage(nickname));
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
    ///
    /// Only works when chat is visible (no panel active).
    pub fn handle_prev_chat_tab(&mut self) -> Task<Message> {
        // Only switch chat tabs when chat is visible (no panel active)
        if self.active_panel() != ActivePanel::None {
            return Task::none();
        }

        let Some(conn_id) = self.active_connection else {
            return Task::none();
        };
        let Some(conn) = self.connections.get(&conn_id) else {
            return Task::none();
        };

        // Build ordered list of tabs: Server first, then PMs alphabetically
        let mut tabs = vec![ChatTab::Server];
        let mut pm_nicknames: Vec<String> = conn.user_messages.keys().cloned().collect();
        pm_nicknames.sort();
        for nickname in pm_nicknames {
            tabs.push(ChatTab::UserMessage(nickname));
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
            // On bookmark edit screen, check actual focus and cycle
            return self.update(Message::BookmarkEditTabPressed);
        } else if self.active_panel() == ActivePanel::UserManagement {
            // On user management screen, handle Tab based on mode
            if let Some(conn_id) = self.active_connection
                && let Some(conn) = self.connections.get(&conn_id)
            {
                match &conn.user_management.mode {
                    UserManagementMode::Create => {
                        // Create mode: Check actual focus and cycle
                        return self.update(Message::UserManagementCreateTabPressed);
                    }
                    UserManagementMode::Edit { .. } => {
                        // Edit mode: Check actual focus and cycle
                        return self.update(Message::UserManagementEditTabPressed);
                    }
                    UserManagementMode::List | UserManagementMode::ConfirmDelete { .. } => {
                        // List/ConfirmDelete: No Tab navigation
                    }
                }
            }
        } else if self.active_panel() == ActivePanel::ChangePassword {
            // Change password panel: check actual focus and cycle through fields
            return self.update(Message::ChangePasswordTabPressed);
        } else if self.active_panel() == ActivePanel::ServerInfo {
            // Server info edit screen: check actual focus and cycle
            if let Some(conn_id) = self.active_connection
                && let Some(conn) = self.connections.get(&conn_id)
                && conn.server_info_edit.is_some()
            {
                return self.update(Message::ServerInfoEditTabPressed);
            }
        } else if self.active_panel() == ActivePanel::Files {
            // Files panel: focus directory name input if dialog is open
            if let Some(conn_id) = self.active_connection
                && let Some(conn) = self.connections.get(&conn_id)
                && conn.files_management.creating_directory
            {
                return operation::focus(Id::from(InputId::NewDirectoryName));
            }
        } else if self.active_panel() == ActivePanel::Broadcast {
            // Broadcast screen only has one field, so focus stays
            self.focused_field = InputId::BroadcastMessage;
            return operation::focus(Id::from(InputId::BroadcastMessage));
        } else if self.active_panel() == ActivePanel::News {
            // News panel uses text_editor which handles its own focus
            // No Tab navigation needed
            return Task::none();
        } else if self.active_panel() == ActivePanel::Settings {
            // Settings panel: check actual focus and cycle through fields
            return self.update(Message::SettingsTabPressed);
        } else if self.active_connection.is_some() {
            // In chat view, Tab triggers nickname completion
            return self.update(Message::ChatTabComplete);
        } else if self.active_connection.is_none() {
            // On connection screen, check actual focus and cycle
            return self.update(Message::ConnectionFormTabPressed);
        }
        Task::none()
    }
}
