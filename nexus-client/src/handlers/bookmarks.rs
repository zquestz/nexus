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
            let avatar = self.config.settings.avatar.clone();
            let display_name = bookmark.name.clone();

            return Task::perform(
                async move {
                    crate::network::connect_to_server(
                        server_address,
                        port,
                        username,
                        password,
                        locale,
                        avatar,
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

    // ==================== Tab Navigation ====================

    /// Handle Tab pressed in bookmark edit form
    ///
    /// Checks which field is actually focused using async operations,
    /// then moves to the next field in sequence.
    pub fn handle_bookmark_edit_tab_pressed(&mut self) -> Task<Message> {
        // Check focus state of all five bookmark fields in parallel
        let check_name = operation::is_focused(Id::from(InputId::BookmarkName));
        let check_address = operation::is_focused(Id::from(InputId::BookmarkAddress));
        let check_port = operation::is_focused(Id::from(InputId::BookmarkPort));
        let check_username = operation::is_focused(Id::from(InputId::BookmarkUsername));
        let check_password = operation::is_focused(Id::from(InputId::BookmarkPassword));

        // Batch the checks and combine results
        Task::batch([
            check_name.map(|focused| (0, focused)),
            check_address.map(|focused| (1, focused)),
            check_port.map(|focused| (2, focused)),
            check_username.map(|focused| (3, focused)),
            check_password.map(|focused| (4, focused)),
        ])
        .collect()
        .map(|results: Vec<(u8, bool)>| {
            let name_focused = results.iter().any(|(i, f)| *i == 0 && *f);
            let address_focused = results.iter().any(|(i, f)| *i == 1 && *f);
            let port_focused = results.iter().any(|(i, f)| *i == 2 && *f);
            let username_focused = results.iter().any(|(i, f)| *i == 3 && *f);
            let password_focused = results.iter().any(|(i, f)| *i == 4 && *f);
            Message::BookmarkEditFocusResult(
                name_focused,
                address_focused,
                port_focused,
                username_focused,
                password_focused,
            )
        })
    }

    /// Handle focus check result for bookmark edit Tab navigation
    pub fn handle_bookmark_edit_focus_result(
        &mut self,
        name_focused: bool,
        address_focused: bool,
        port_focused: bool,
        username_focused: bool,
        password_focused: bool,
    ) -> Task<Message> {
        // Determine next field based on which is currently focused
        let next_field = if name_focused {
            InputId::BookmarkAddress
        } else if address_focused {
            InputId::BookmarkPort
        } else if port_focused {
            InputId::BookmarkUsername
        } else if username_focused {
            InputId::BookmarkPassword
        } else if password_focused {
            // Wrap around to first field
            InputId::BookmarkName
        } else {
            // None focused, start at first field
            InputId::BookmarkName
        };

        self.focused_field = next_field;
        operation::focus(Id::from(next_field))
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
