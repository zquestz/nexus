//! Bookmark management

use crate::NexusApp;
use crate::types::{BookmarkEditMode, BookmarkEditState, InputId, Message};
use iced::Task;
use iced::widget::text_input;

// Error message constants
const ERR_NAME_REQUIRED: &str = "Bookmark name is required";
const ERR_ADDRESS_REQUIRED: &str = "Server address is required";
const ERR_PORT_REQUIRED: &str = "Port is required";
const ERR_PORT_INVALID: &str = "Port must be a valid number (1-65535)";

impl NexusApp {
    /// Handle bookmark name field change
    pub fn handle_bookmark_name_changed(&mut self, name: String) -> Task<Message> {
        self.bookmark_edit.bookmark.name = name;
        self.bookmark_edit.error = None;
        self.focused_field = InputId::BookmarkName;
        Task::none()
    }

    /// Handle bookmark address field change
    pub fn handle_bookmark_address_changed(&mut self, addr: String) -> Task<Message> {
        self.bookmark_edit.bookmark.address = addr;
        self.bookmark_edit.error = None;
        self.focused_field = InputId::BookmarkAddress;
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

    /// Handle bookmark password field change
    pub fn handle_bookmark_password_changed(&mut self, password: String) -> Task<Message> {
        self.bookmark_edit.bookmark.password = password;
        self.bookmark_edit.error = None;
        self.focused_field = InputId::BookmarkPassword;
        Task::none()
    }

    /// Handle bookmark auto-connect toggle
    pub fn handle_bookmark_auto_connect_toggled(&mut self, enabled: bool) -> Task<Message> {
        self.bookmark_edit.bookmark.auto_connect = enabled;
        Task::none()
    }

    /// Connect to a bookmarked server
    pub fn handle_connect_to_bookmark(&mut self, index: usize) -> Task<Message> {
        // Check if this bookmark is already connecting
        if self.connecting_bookmarks.contains(&index) {
            return Task::none();
        }

        // Get bookmark data
        if let Some(bookmark) = self.config.get_bookmark(index) {
            // Mark this bookmark as connecting
            self.connecting_bookmarks.insert(index);

            // Generate connection ID
            let connection_id = self.next_connection_id;
            self.next_connection_id += 1;

            // Validate port
            let port: u16 = match bookmark.port.parse() {
                Ok(p) => p,
                Err(_) => {
                    // Clear the connecting lock on validation failure
                    self.connecting_bookmarks.remove(&index);
                    self.connection_form.error =
                        Some(format!("Invalid port in bookmark: {}", bookmark.name));
                    return Task::none();
                }
            };

            let server_address = bookmark.address.clone();
            let username = bookmark.username.clone();
            let password = bookmark.password.clone();
            let server_name = bookmark.name.clone();

            // Store bookmark index for this connection
            let bookmark_index = Some(index);

            // Clone server_name for the result handler closure
            let display_name_for_result = server_name.clone();

            // Connect directly without modifying connection_form
            return Task::perform(
                async move {
                    crate::network::connect_to_server(
                        server_address,
                        port,
                        username,
                        password,
                        connection_id,
                    )
                    .await
                },
                move |result| {
                    let display_name = display_name_for_result.clone();
                    Message::BookmarkConnectionResult {
                        result,
                        bookmark_index,
                        display_name,
                    }
                },
            );
        }
        Task::none()
    }

    /// Show the add bookmark dialog
    pub fn handle_show_add_bookmark(&mut self) -> Task<Message> {
        self.bookmark_edit = BookmarkEditState::default();
        self.bookmark_edit.mode = BookmarkEditMode::Add;
        self.focused_field = InputId::BookmarkName;
        text_input::focus(text_input::Id::from(InputId::BookmarkName))
    }

    /// Show the edit bookmark dialog for a specific bookmark
    pub fn handle_show_edit_bookmark(&mut self, index: usize) -> Task<Message> {
        if let Some(bookmark) = self.config.get_bookmark(index) {
            self.bookmark_edit.mode = BookmarkEditMode::Edit(index);
            self.bookmark_edit.bookmark = bookmark.clone();
            self.focused_field = InputId::BookmarkName;
            return text_input::focus(text_input::Id::from(InputId::BookmarkName));
        }
        Task::none()
    }

    /// Cancel bookmark editing and close the dialog
    pub fn handle_cancel_bookmark_edit(&mut self) -> Task<Message> {
        self.bookmark_edit = BookmarkEditState::default();
        Task::none()
    }

    /// Save the current bookmark (add or update)
    pub fn handle_save_bookmark(&mut self) -> Task<Message> {
        // Validate bookmark fields
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
            self.bookmark_edit.error = Some(format!("Failed to save config: {}", e));
            return Task::none();
        }

        self.bookmark_edit = BookmarkEditState::default();
        Task::none()
    }

    /// Delete a bookmark by index
    pub fn handle_delete_bookmark(&mut self, index: usize) -> Task<Message> {
        self.config.delete_bookmark(index);
        if let Err(e) = self.config.save() {
            self.connection_form.error = Some(format!("Failed to save config: {}", e));
        }
        // Close the bookmark edit dialog if it's open
        self.bookmark_edit = BookmarkEditState::default();
        Task::none()
    }

    /// Validate bookmark fields
    fn validate_bookmark(&self) -> Option<String> {
        if self.bookmark_edit.bookmark.name.trim().is_empty() {
            return Some(ERR_NAME_REQUIRED.to_string());
        }
        if self.bookmark_edit.bookmark.address.trim().is_empty() {
            return Some(ERR_ADDRESS_REQUIRED.to_string());
        }
        if self.bookmark_edit.bookmark.port.trim().is_empty() {
            return Some(ERR_PORT_REQUIRED.to_string());
        }
        if self.bookmark_edit.bookmark.port.parse::<u16>().is_err() {
            return Some(ERR_PORT_INVALID.to_string());
        }
        None
    }
}
