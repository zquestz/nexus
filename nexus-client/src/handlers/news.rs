//! News panel handlers

use iced::Task;
use iced::widget::{Id, operation};
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
                // In list mode, close the panel
                self.handle_show_chat_view()
            }
            NewsManagementMode::Create | NewsManagementMode::Edit { .. } => {
                // In create/edit mode, return to list
                conn.news_management.reset_to_list();
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
        self.focused_field = InputId::NewsBody;
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

    // ==================== Create Form Handlers ====================

    /// Handle news create body field change
    pub fn handle_news_create_body_changed(&mut self, body: String) -> Task<Message> {
        if let Some(conn_id) = self.active_connection
            && let Some(conn) = self.connections.get_mut(&conn_id)
        {
            conn.news_management.create_body = body;
            self.focused_field = InputId::NewsBody;
        }
        Task::none()
    }

    /// Handle pick image button press in create form
    pub fn handle_news_create_pick_image_pressed(&mut self) -> Task<Message> {
        // Clear any previous error when starting a new pick
        if let Some(conn_id) = self.active_connection
            && let Some(conn) = self.connections.get_mut(&conn_id)
        {
            conn.news_management.create_error = None;
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
            Message::NewsCreateImageLoaded,
        )
    }

    /// Handle image loaded from file picker in create form
    pub fn handle_news_create_image_loaded(
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
                    conn.news_management.create_image = data_uri;
                    conn.news_management.cached_create_image = cached;
                    conn.news_management.create_error = None;
                } else {
                    conn.news_management.create_error = Some(t("err-news-image-decode-failed"));
                }
            }
            Err(ImagePickerError::Cancelled) => {
                // User cancelled, do nothing
            }
            Err(ImagePickerError::TooLarge) => {
                conn.news_management.create_error = Some(t("err-news-image-too-large"));
            }
            Err(ImagePickerError::UnsupportedType) => {
                conn.news_management.create_error = Some(t("err-news-image-unsupported-type"));
            }
        }

        Task::none()
    }

    /// Handle clear image button press in create form
    pub fn handle_news_create_clear_image_pressed(&mut self) -> Task<Message> {
        let Some(conn_id) = self.active_connection else {
            return Task::none();
        };
        let Some(conn) = self.connections.get_mut(&conn_id) else {
            return Task::none();
        };

        conn.news_management.create_image.clear();
        conn.news_management.cached_create_image = None;
        conn.news_management.create_error = None;

        Task::none()
    }

    /// Handle create news button pressed
    pub fn handle_news_create_pressed(&mut self) -> Task<Message> {
        let Some(conn_id) = self.active_connection else {
            return Task::none();
        };
        let Some(conn) = self.connections.get_mut(&conn_id) else {
            return Task::none();
        };

        let body = conn.news_management.create_body.trim().to_string();
        let image = conn.news_management.create_image.clone();

        // Must have either body or image
        if body.is_empty() && image.is_empty() {
            conn.news_management.create_error = Some(t("err-news-empty"));
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
            conn.news_management.create_error = Some(error_msg);
            return Task::none();
        }

        // Build the create message
        let msg = ClientMessage::NewsCreate {
            body: if body.is_empty() { None } else { Some(body) },
            image: if image.is_empty() { None } else { Some(image) },
        };

        // Send create request
        match conn.send(msg) {
            Ok(message_id) => {
                conn.pending_requests
                    .track(message_id, ResponseRouting::NewsCreateResult);
            }
            Err(e) => {
                conn.news_management.create_error =
                    Some(format!("{}: {}", t("err-send-failed"), e));
            }
        }

        Task::none()
    }

    // ==================== Edit Form Handlers ====================

    /// Handle news edit body field change
    pub fn handle_news_edit_body_changed(&mut self, body: String) -> Task<Message> {
        if let Some(conn_id) = self.active_connection
            && let Some(conn) = self.connections.get_mut(&conn_id)
        {
            conn.news_management.edit_body = body;
            self.focused_field = InputId::NewsBody;
        }
        Task::none()
    }

    /// Handle pick image button press in edit form
    pub fn handle_news_edit_pick_image_pressed(&mut self) -> Task<Message> {
        // Clear any previous error when starting a new pick
        if let Some(conn_id) = self.active_connection
            && let Some(conn) = self.connections.get_mut(&conn_id)
        {
            conn.news_management.edit_error = None;
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
            Message::NewsEditImageLoaded,
        )
    }

    /// Handle image loaded from file picker in edit form
    pub fn handle_news_edit_image_loaded(
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
                    conn.news_management.edit_image = data_uri;
                    conn.news_management.cached_edit_image = cached;
                    conn.news_management.edit_error = None;
                } else {
                    conn.news_management.edit_error = Some(t("err-news-image-decode-failed"));
                }
            }
            Err(ImagePickerError::Cancelled) => {
                // User cancelled, do nothing
            }
            Err(ImagePickerError::TooLarge) => {
                conn.news_management.edit_error = Some(t("err-news-image-too-large"));
            }
            Err(ImagePickerError::UnsupportedType) => {
                conn.news_management.edit_error = Some(t("err-news-image-unsupported-type"));
            }
        }

        Task::none()
    }

    /// Handle clear image button press in edit form
    pub fn handle_news_edit_clear_image_pressed(&mut self) -> Task<Message> {
        let Some(conn_id) = self.active_connection else {
            return Task::none();
        };
        let Some(conn) = self.connections.get_mut(&conn_id) else {
            return Task::none();
        };

        conn.news_management.edit_image.clear();
        conn.news_management.cached_edit_image = None;
        conn.news_management.edit_error = None;

        Task::none()
    }

    /// Handle update news button pressed
    pub fn handle_news_update_pressed(&mut self) -> Task<Message> {
        let Some(conn_id) = self.active_connection else {
            return Task::none();
        };
        let Some(conn) = self.connections.get_mut(&conn_id) else {
            return Task::none();
        };

        let id = match &conn.news_management.mode {
            NewsManagementMode::Edit { id, .. } => *id,
            _ => return Task::none(),
        };

        let body = conn.news_management.edit_body.trim().to_string();
        let image = conn.news_management.edit_image.clone();

        // Must have either body or image
        if body.is_empty() && image.is_empty() {
            conn.news_management.edit_error = Some(t("err-news-empty"));
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
            conn.news_management.edit_error = Some(error_msg);
            return Task::none();
        }

        // Build the update message
        let msg = ClientMessage::NewsUpdate {
            id,
            body: if body.is_empty() { None } else { Some(body) },
            image: if image.is_empty() { None } else { Some(image) },
        };

        // Send update request
        match conn.send(msg) {
            Ok(message_id) => {
                conn.pending_requests
                    .track(message_id, ResponseRouting::NewsUpdateResult);
            }
            Err(e) => {
                conn.news_management.edit_error = Some(format!("{}: {}", t("err-send-failed"), e));
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
}
