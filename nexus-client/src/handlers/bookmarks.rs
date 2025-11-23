//! Bookmark management

use crate::NexusApp;
use crate::types::{BookmarkEditMode, InputId, Message};
use iced::Task;
use iced::widget::text_input;

impl NexusApp {
    // Bookmark field update handlers
    pub fn handle_bookmark_name_changed(&mut self, name: String) -> Task<Message> {
        self.bookmark_edit.name = name;
        self.focused_field = InputId::BookmarkName;
        Task::none()
    }

    pub fn handle_bookmark_address_changed(&mut self, addr: String) -> Task<Message> {
        self.bookmark_edit.address = addr;
        self.focused_field = InputId::BookmarkAddress;
        Task::none()
    }

    pub fn handle_bookmark_port_changed(&mut self, port: String) -> Task<Message> {
        self.bookmark_edit.port = port;
        self.focused_field = InputId::BookmarkPort;
        Task::none()
    }

    pub fn handle_bookmark_username_changed(&mut self, username: String) -> Task<Message> {
        self.bookmark_edit.username = username;
        self.focused_field = InputId::BookmarkUsername;
        Task::none()
    }

    pub fn handle_bookmark_password_changed(&mut self, password: String) -> Task<Message> {
        self.bookmark_edit.password = password;
        self.focused_field = InputId::BookmarkPassword;
        Task::none()
    }

    pub fn handle_bookmark_auto_connect_toggled(&mut self, enabled: bool) -> Task<Message> {
        self.bookmark_edit.auto_connect = enabled;
        Task::none()
    }

    // Bookmark operation handlers
    pub fn handle_connect_to_bookmark(&mut self, index: usize) -> Task<Message> {
        // Auto-fill connection form from bookmark
        if let Some(bookmark) = self.config.get_bookmark(index) {
            self.connection_form.server_name = bookmark.name.clone();
            self.connection_form.server_address = bookmark.address.clone();
            self.connection_form.port = bookmark.port.clone();
            self.connection_form.username = bookmark.username.clone();
            self.connection_form.password = bookmark.password.clone();

            // Auto-connect
            return self.update(Message::ConnectPressed);
        }
        Task::none()
    }

    pub fn handle_show_add_bookmark(&mut self) -> Task<Message> {
        self.bookmark_edit.clear();
        self.bookmark_edit.mode = BookmarkEditMode::Add;
        self.focused_field = InputId::BookmarkName;
        text_input::focus(text_input::Id::from(InputId::BookmarkName))
    }

    pub fn handle_show_edit_bookmark(&mut self, index: usize) -> Task<Message> {
        if let Some(bookmark) = self.config.get_bookmark(index) {
            self.bookmark_edit
                .load_from_bookmark(BookmarkEditMode::Edit(index), bookmark);
            self.focused_field = InputId::BookmarkName;
            return text_input::focus(text_input::Id::from(InputId::BookmarkName));
        }
        Task::none()
    }

    pub fn handle_cancel_bookmark_edit(&mut self) -> Task<Message> {
        self.bookmark_edit.clear();
        Task::none()
    }

    pub fn handle_save_bookmark(&mut self) -> Task<Message> {
        let bookmark = self.bookmark_edit.to_bookmark();

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
            self.connection_form.error = Some(format!("Failed to save config: {}", e));
        }

        self.bookmark_edit.clear();
        Task::none()
    }

    pub fn handle_delete_bookmark(&mut self, index: usize) -> Task<Message> {
        self.config.delete_bookmark(index);
        let _ = self.config.save();
        Task::none()
    }
}
