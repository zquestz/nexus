//! Bookmark management

use crate::NexusApp;
use crate::i18n::{get_locale, t, t_args};
use crate::types::{BookmarkEditMode, BookmarkEditState, InputId, Message};
use iced::Task;
use iced::widget::{Id, operation};
use std::collections::HashMap;

impl NexusApp {
    // ==================== Form Field Handlers ====================

    /// Handle bookmark address field change
    pub fn handle_bookmark_address_changed(&mut self, addr: String) -> Task<Message> {
        self.bookmark_edit.bookmark.address = addr;
        self.bookmark_edit.error = None;
        self.focused_field = InputId::BookmarkAddress;
        Task::none()
    }

    /// Handle bookmark auto-connect toggle
    pub fn handle_bookmark_auto_connect_toggled(&mut self, enabled: bool) -> Task<Message> {
        self.bookmark_edit.bookmark.auto_connect = enabled;
        Task::none()
    }

    /// Handle bookmark name field change
    pub fn handle_bookmark_name_changed(&mut self, name: String) -> Task<Message> {
        self.bookmark_edit.bookmark.name = name;
        self.bookmark_edit.error = None;
        self.focused_field = InputId::BookmarkName;
        Task::none()
    }

    /// Handle bookmark password field change
    pub fn handle_bookmark_password_changed(&mut self, password: String) -> Task<Message> {
        self.bookmark_edit.bookmark.password = password;
        self.bookmark_edit.error = None;
        self.focused_field = InputId::BookmarkPassword;
        Task::none()
    }

    /// Handle bookmark port field change
    pub fn handle_bookmark_port_changed(&mut self, port: String) -> Task<Message> {
        self.bookmark_edit.bookmark.port = port;
        self.bookmark_edit.error = None;
        self.focused_field = InputId::BookmarkPort;
        Task::none()
    }

    /// Handle bookmark username field change
    pub fn handle_bookmark_username_changed(&mut self, username: String) -> Task<Message> {
        self.bookmark_edit.bookmark.username = username;
        self.bookmark_edit.error = None;
        self.focused_field = InputId::BookmarkUsername;
        Task::none()
    }

    // ==================== Dialog Actions ====================

    /// Cancel bookmark editing and close the dialog
    pub fn handle_cancel_bookmark_edit(&mut self) -> Task<Message> {
        self.bookmark_edit = BookmarkEditState::default();
        Task::none()
    }

    /// Save the current bookmark (add or update)
    pub fn handle_save_bookmark(&mut self) -> Task<Message> {
        if let Some(error) = self.validate_bookmark() {
            self.bookmark_edit.error = Some(error);
            return Task::none();
        }

        let bookmark = self.bookmark_edit.bookmark.clone();

        match self.bookmark_edit.mode {
            BookmarkEditMode::Add => {
                self.config.add_bookmark(bookmark);
            }
            BookmarkEditMode::Edit(index) => {
                self.config.update_bookmark(index, bookmark);
            }
            BookmarkEditMode::None => {}
        }

        if let Err(e) = self.config.save() {
            self.bookmark_edit.error = Some(t_args(
                "err-failed-save-config",
                &[("error", &e.to_string())],
            ));
            return Task::none();
        }

        self.bookmark_edit = BookmarkEditState::default();
        Task::none()
    }

    /// Show the add bookmark dialog
    pub fn handle_show_add_bookmark(&mut self) -> Task<Message> {
        self.bookmark_edit = BookmarkEditState::default();
        self.bookmark_edit.mode = BookmarkEditMode::Add;
        self.focused_field = InputId::BookmarkName;
        operation::focus(Id::from(InputId::BookmarkName))
    }

    /// Show the edit bookmark dialog for a specific bookmark
    pub fn handle_show_edit_bookmark(&mut self, index: usize) -> Task<Message> {
        if let Some(bookmark) = self.config.get_bookmark(index) {
            self.bookmark_edit.mode = BookmarkEditMode::Edit(index);
            self.bookmark_edit.bookmark = bookmark.clone();
            self.focused_field = InputId::BookmarkName;

            // Move any connection error to the edit dialog (acknowledges and clears it)
            self.bookmark_edit.error = self.bookmark_errors.remove(&index);

            return operation::focus(Id::from(InputId::BookmarkName));
        }
        Task::none()
    }

    // ==================== Bookmark Operations ====================

    /// Connect to a bookmarked server
    pub fn handle_connect_to_bookmark(&mut self, index: usize) -> Task<Message> {
        if self.connecting_bookmarks.contains(&index) {
            return Task::none();
        }

        if let Some(bookmark) = self.config.get_bookmark(index) {
            self.connecting_bookmarks.insert(index);

            let connection_id = self.next_connection_id;
            self.next_connection_id += 1;

            let port: u16 = match bookmark.port.parse() {
                Ok(p) => p,
                Err(_) => {
                    self.connecting_bookmarks.remove(&index);
                    self.connection_form.error = Some(t_args(
                        "err-invalid-port-bookmark",
                        &[("name", &bookmark.name)],
                    ));
                    return Task::none();
                }
            };

            let server_address = bookmark.address.clone();
            let username = bookmark.username.clone();
            let password = bookmark.password.clone();
            let locale = get_locale().to_string();
            let display_name = bookmark.name.clone();

            return Task::perform(
                async move {
                    crate::network::connect_to_server(
                        server_address,
                        port,
                        username,
                        password,
                        locale,
                        connection_id,
                    )
                    .await
                },
                move |result| Message::BookmarkConnectionResult {
                    result,
                    bookmark_index: Some(index),
                    display_name: display_name.clone(),
                },
            );
        }
        Task::none()
    }

    /// Delete a bookmark by index
    pub fn handle_delete_bookmark(&mut self, index: usize) -> Task<Message> {
        self.config.delete_bookmark(index);
        if let Err(e) = self.config.save() {
            self.connection_form.error = Some(t_args(
                "err-failed-save-config",
                &[("error", &e.to_string())],
            ));
        }

        // Clean up bookmark_errors: remove the deleted index and shift higher indices down
        self.bookmark_errors.remove(&index);
        let shifted: HashMap<usize, String> = self
            .bookmark_errors
            .drain()
            .map(|(i, err)| if i > index { (i - 1, err) } else { (i, err) })
            .collect();
        self.bookmark_errors = shifted;

        self.bookmark_edit = BookmarkEditState::default();
        Task::none()
    }

    // ==================== Private Helpers ====================

    /// Validate bookmark fields
    fn validate_bookmark(&self) -> Option<String> {
        if self.bookmark_edit.bookmark.name.trim().is_empty() {
            return Some(t("err-name-required"));
        }
        if self.bookmark_edit.bookmark.address.trim().is_empty() {
            return Some(t("err-address-required"));
        }
        if self.bookmark_edit.bookmark.port.trim().is_empty() {
            return Some(t("err-port-required"));
        }
        if self.bookmark_edit.bookmark.port.parse::<u16>().is_err() {
            return Some(t("err-port-invalid"));
        }
        None
    }
}
