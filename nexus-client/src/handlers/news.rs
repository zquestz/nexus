//! News panel handlers

use iced::Task;
use iced::widget::{Id, operation, text_editor};
use nexus_common::protocol::ClientMessage;
use nexus_common::validators::{self, NewsBodyError};
use rfd::AsyncFileDialog;

use crate::NexusApp;
use crate::i18n::{t, t_args};
use crate::image::{ImagePickerError, decode_data_uri_max_width};
use crate::style::{NEWS_IMAGE_MAX_CACHE_WIDTH, NEWS_IMAGE_MAX_SIZE};
use crate::types::{
    ActivePanel, InputId, Message, NewsManagementMode, PendingRequests, ResponseRouting,
};

impl NexusApp {
    // ==================== Panel Toggle ====================

    /// Toggle the news panel
    ///
    /// When opening, fetches the news list from the server.
    pub fn handle_toggle_news(&mut self) -> Task<Message> {
        if self.active_panel() == ActivePanel::News {
            return Task::none();
        }

        self.set_active_panel(ActivePanel::News);

        let Some(conn_id) = self.active_connection else {
            return Task::none();
        };
        let Some(conn) = self.connections.get_mut(&conn_id) else {
            return Task::none();
        };

        // Reset to list mode and clear any previous state
        conn.news_management.reset_to_list();
        conn.news_management.news_items = None; // Trigger loading state

        // Clear the text editor content
        self.news_body_content.remove(&conn_id);

        // Request news list from server
        match conn.send(ClientMessage::NewsList) {
            Ok(message_id) => {
                conn.pending_requests
                    .track(message_id, ResponseRouting::PopulateNewsList);
            }
            Err(e) => {
                conn.news_management.news_items =
                    Some(Err(format!("{}: {}", t("err-send-failed"), e)));
            }
        }

        Task::none()
    }

    /// Handle cancel in news panel
    ///
    /// In create/edit mode: returns to list view
    /// In list mode: closes the panel
    pub fn handle_cancel_news(&mut self) -> Task<Message> {
        let Some(conn_id) = self.active_connection else {
            return self.handle_show_chat_view();
        };
        let Some(conn) = self.connections.get_mut(&conn_id) else {
            return self.handle_show_chat_view();
        };

        match &conn.news_management.mode {
            NewsManagementMode::List => {
                // In list mode, close the panel and clean up
                self.news_body_content.remove(&conn_id);
                self.handle_show_chat_view()
            }
            NewsManagementMode::Create | NewsManagementMode::Edit { .. } => {
                // In create/edit mode, return to list and clear editor
                conn.news_management.reset_to_list();
                self.news_body_content.remove(&conn_id);
                Task::none()
            }
            NewsManagementMode::ConfirmDelete { .. } => {
                // Should not happen (modal handles its own cancel)
                conn.news_management.mode = NewsManagementMode::List;
                Task::none()
            }
        }
    }

    // ==================== List View Actions ====================

    /// Show the create news form
    pub fn handle_news_show_create(&mut self) -> Task<Message> {
        let Some(conn_id) = self.active_connection else {
            return Task::none();
        };
        let Some(conn) = self.connections.get_mut(&conn_id) else {
            return Task::none();
        };

        conn.news_management.enter_create_mode();

        // Initialize empty text editor content
        self.news_body_content
            .insert(conn_id, text_editor::Content::new());

        // Focus the text editor
        operation::focus(Id::from(InputId::NewsBody))
    }

    /// Handle edit button click on a news item
    ///
    /// Requests news item details from server, then transitions to edit mode.
    pub fn handle_news_edit_clicked(&mut self, id: i64) -> Task<Message> {
        let Some(conn_id) = self.active_connection else {
            return Task::none();
        };
        let Some(conn) = self.connections.get_mut(&conn_id) else {
            return Task::none();
        };

        // Request news item details from server
        match conn.send(ClientMessage::NewsEdit { id }) {
            Ok(message_id) => {
                conn.pending_requests
                    .track(message_id, ResponseRouting::PopulateNewsEdit);
            }
            Err(e) => {
                conn.news_management.list_error = Some(format!("{}: {}", t("err-send-failed"), e));
            }
        }

        Task::none()
    }

    /// Handle delete button click on a news item
    ///
    /// Shows the delete confirmation modal.
    pub fn handle_news_delete_clicked(&mut self, id: i64) -> Task<Message> {
        let Some(conn_id) = self.active_connection else {
            return Task::none();
        };
        let Some(conn) = self.connections.get_mut(&conn_id) else {
            return Task::none();
        };

        conn.news_management.enter_confirm_delete_mode(id);
        Task::none()
    }

    /// Handle confirm delete button pressed
    pub fn handle_news_confirm_delete(&mut self) -> Task<Message> {
        let Some(conn_id) = self.active_connection else {
            return Task::none();
        };
        let Some(conn) = self.connections.get_mut(&conn_id) else {
            return Task::none();
        };

        let id = match &conn.news_management.mode {
            NewsManagementMode::ConfirmDelete { id } => *id,
            _ => return Task::none(),
        };

        // Send delete request
        match conn.send(ClientMessage::NewsDelete { id }) {
            Ok(message_id) => {
                conn.pending_requests
                    .track(message_id, ResponseRouting::NewsDeleteResult);
            }
            Err(e) => {
                conn.news_management.mode = NewsManagementMode::List;
                conn.news_management.list_error = Some(format!("{}: {}", t("err-send-failed"), e));
            }
        }

        // Return to list view immediately (will refresh on success)
        conn.news_management.mode = NewsManagementMode::List;
        Task::none()
    }

    /// Handle cancel delete button pressed
    pub fn handle_news_cancel_delete(&mut self) -> Task<Message> {
        let Some(conn_id) = self.active_connection else {
            return Task::none();
        };
        let Some(conn) = self.connections.get_mut(&conn_id) else {
            return Task::none();
        };

        conn.news_management.mode = NewsManagementMode::List;
        Task::none()
    }

    // ==================== Text Editor Action ====================

    /// Handle text editor action for news body
    pub fn handle_news_body_action(&mut self, action: text_editor::Action) -> Task<Message> {
        let Some(conn_id) = self.active_connection else {
            return Task::none();
        };

        // Get or create content for this connection
        let content = self.news_body_content.entry(conn_id).or_default();

        content.perform(action);

        Task::none()
    }

    // ==================== Image Handlers ====================

    /// Handle pick image button press
    pub fn handle_news_pick_image_pressed(&mut self) -> Task<Message> {
        // Clear any previous error when starting a new pick
        if let Some(conn_id) = self.active_connection
            && let Some(conn) = self.connections.get_mut(&conn_id)
        {
            conn.news_management.form_error = None;
        }

        Task::perform(
            async {
                let handle = AsyncFileDialog::new()
                    .add_filter("Images", &["png", "jpg", "jpeg", "webp", "svg"])
                    .pick_file()
                    .await;

                match handle {
                    Some(file) => {
                        let path = file.path();
                        let extension = path
                            .extension()
                            .and_then(|e| e.to_str())
                            .unwrap_or("")
                            .to_lowercase();

                        // Determine MIME type from extension
                        let mime_type = match extension.as_str() {
                            "png" => "image/png",
                            "jpg" | "jpeg" => "image/jpeg",
                            "webp" => "image/webp",
                            "svg" => "image/svg+xml",
                            _ => return Err(ImagePickerError::UnsupportedType),
                        };

                        // Read file contents
                        let bytes = file.read().await;

                        // Check file size
                        if bytes.len() > NEWS_IMAGE_MAX_SIZE {
                            return Err(ImagePickerError::TooLarge);
                        }

                        // Validate file content matches expected format
                        if !crate::image::validate_image_bytes(&bytes, mime_type) {
                            return Err(ImagePickerError::UnsupportedType);
                        }

                        // Encode as data URI
                        use base64::Engine;
                        let base64_data = base64::engine::general_purpose::STANDARD.encode(&bytes);
                        let data_uri = format!("data:{};base64,{}", mime_type, base64_data);

                        Ok(data_uri)
                    }
                    None => Err(ImagePickerError::Cancelled),
                }
            },
            Message::NewsImageLoaded,
        )
    }

    /// Handle image loaded from file picker
    pub fn handle_news_image_loaded(
        &mut self,
        result: Result<String, ImagePickerError>,
    ) -> Task<Message> {
        let Some(conn_id) = self.active_connection else {
            return Task::none();
        };
        let Some(conn) = self.connections.get_mut(&conn_id) else {
            return Task::none();
        };

        match result {
            Ok(data_uri) => {
                let cached = decode_data_uri_max_width(&data_uri, NEWS_IMAGE_MAX_CACHE_WIDTH);
                if cached.is_some() {
                    conn.news_management.form_image = data_uri;
                    conn.news_management.cached_form_image = cached;
                    conn.news_management.form_error = None;
                } else {
                    conn.news_management.form_error = Some(t("err-news-image-decode-failed"));
                }
            }
            Err(ImagePickerError::Cancelled) => {
                // User cancelled, do nothing
            }
            Err(ImagePickerError::TooLarge) => {
                conn.news_management.form_error = Some(t("err-news-image-too-large"));
            }
            Err(ImagePickerError::UnsupportedType) => {
                conn.news_management.form_error = Some(t("err-news-image-unsupported-type"));
            }
        }

        Task::none()
    }

    /// Handle clear image button press
    pub fn handle_news_clear_image_pressed(&mut self) -> Task<Message> {
        let Some(conn_id) = self.active_connection else {
            return Task::none();
        };
        let Some(conn) = self.connections.get_mut(&conn_id) else {
            return Task::none();
        };

        conn.news_management.form_image.clear();
        conn.news_management.cached_form_image = None;
        conn.news_management.form_error = None;

        Task::none()
    }

    // ==================== Submit Handler ====================

    /// Handle submit button pressed (create or update based on mode)
    pub fn handle_news_submit_pressed(&mut self) -> Task<Message> {
        let Some(conn_id) = self.active_connection else {
            return Task::none();
        };

        // Get the body text from the editor
        let body = self
            .news_body_content
            .get(&conn_id)
            .map(|c| c.text())
            .unwrap_or_default()
            .trim()
            .to_string();

        let Some(conn) = self.connections.get_mut(&conn_id) else {
            return Task::none();
        };

        let image = conn.news_management.form_image.clone();

        // Must have either body or image
        if body.is_empty() && image.is_empty() {
            conn.news_management.form_error = Some(t("err-news-empty"));
            return Task::none();
        }

        // Validate body if present
        if !body.is_empty()
            && let Err(e) = validators::validate_news_body(&body)
        {
            let error_msg = match e {
                NewsBodyError::TooLong => t_args(
                    "err-news-body-too-long",
                    &[
                        ("length", &body.len().to_string()),
                        ("max", &validators::MAX_NEWS_BODY_LENGTH.to_string()),
                    ],
                ),
                NewsBodyError::InvalidCharacters => t("err-news-body-invalid-characters"),
            };
            conn.news_management.form_error = Some(error_msg);
            return Task::none();
        }

        // Determine if this is create or update based on mode
        match &conn.news_management.mode {
            NewsManagementMode::Create => {
                let msg = ClientMessage::NewsCreate {
                    body: if body.is_empty() { None } else { Some(body) },
                    image: if image.is_empty() { None } else { Some(image) },
                };

                match conn.send(msg) {
                    Ok(message_id) => {
                        conn.pending_requests
                            .track(message_id, ResponseRouting::NewsCreateResult);
                    }
                    Err(e) => {
                        conn.news_management.form_error =
                            Some(format!("{}: {}", t("err-send-failed"), e));
                    }
                }
            }
            NewsManagementMode::Edit { id } => {
                let id = *id;
                let msg = ClientMessage::NewsUpdate {
                    id,
                    body: if body.is_empty() { None } else { Some(body) },
                    image: if image.is_empty() { None } else { Some(image) },
                };

                match conn.send(msg) {
                    Ok(message_id) => {
                        conn.pending_requests
                            .track(message_id, ResponseRouting::NewsUpdateResult);
                    }
                    Err(e) => {
                        conn.news_management.form_error =
                            Some(format!("{}: {}", t("err-send-failed"), e));
                    }
                }
            }
            _ => {
                // Not in create or edit mode, do nothing
            }
        }

        Task::none()
    }

    // ==================== Refresh Handlers ====================

    /// Refresh news list for a specific connection
    pub fn refresh_news_list_for(&mut self, connection_id: usize) -> Task<Message> {
        let Some(conn) = self.connections.get_mut(&connection_id) else {
            return Task::none();
        };

        // Only refresh if we're in the news panel
        if conn.active_panel != ActivePanel::News {
            return Task::none();
        }

        // Clear news list to show loading state
        conn.news_management.news_items = None;

        // Request news list from server
        match conn.send(ClientMessage::NewsList) {
            Ok(message_id) => {
                conn.pending_requests
                    .track(message_id, ResponseRouting::PopulateNewsList);
            }
            Err(e) => {
                conn.news_management.news_items =
                    Some(Err(format!("{}: {}", t("err-send-failed"), e)));
            }
        }

        Task::none()
    }

    /// Initialize text editor content for editing a news item
    ///
    /// Returns a focus task to focus the text editor.
    pub fn init_news_edit_content(
        &mut self,
        connection_id: usize,
        body: Option<String>,
    ) -> Task<Message> {
        let content = if let Some(text) = body {
            text_editor::Content::with_text(&text)
        } else {
            text_editor::Content::new()
        };
        self.news_body_content.insert(connection_id, content);

        // Focus the text editor
        operation::focus(Id::from(InputId::NewsBody))
    }
}
