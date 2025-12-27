//! News response handlers

use iced::Task;
use iced::widget::markdown;
use nexus_common::framing::MessageId;
use nexus_common::protocol::{NewsAction, NewsItem};

use crate::NexusApp;
use crate::i18n::t;
use crate::image::decode_data_uri_max_width;
use crate::style::NEWS_IMAGE_MAX_CACHE_WIDTH;
use crate::types::{
    ActivePanel, ChatMessage, Message, NewsManagementMode, PendingRequests, ResponseRouting,
};

impl NexusApp {
    /// Handle news list response
    ///
    /// Populates the news list in the news management panel.
    pub fn handle_news_list_response(
        &mut self,
        connection_id: usize,
        message_id: MessageId,
        success: bool,
        error: Option<String>,
        items: Option<Vec<NewsItem>>,
    ) -> Task<Message> {
        let Some(conn) = self.connections.get_mut(&connection_id) else {
            return Task::none();
        };

        // Check if this was a tracked request
        let routing = conn.pending_requests.remove(&message_id);

        if success {
            if let Some(items) = items {
                // Cache images and markdown for all items
                for item in &items {
                    // Cache image if present
                    if let Some(image_data) = &item.image
                        && let Some(cached) =
                            decode_data_uri_max_width(image_data, NEWS_IMAGE_MAX_CACHE_WIDTH)
                    {
                        conn.news_image_cache.insert(item.id, cached);
                    }

                    // Cache parsed markdown if body is present
                    if let Some(body) = &item.body
                        && !body.is_empty()
                    {
                        let parsed: Vec<markdown::Item> = markdown::parse(body).collect();
                        conn.news_markdown_cache.insert(item.id, parsed);
                    }
                }

                // If from news panel, populate the list
                if matches!(routing, Some(ResponseRouting::PopulateNewsList)) {
                    conn.news_management.news_items = Some(Ok(items));
                }
            }
        } else {
            // On error, show in the appropriate place
            if matches!(routing, Some(ResponseRouting::PopulateNewsList)) {
                conn.news_management.news_items = Some(Err(error.unwrap_or_default()));
            }
        }

        Task::none()
    }

    /// Handle news show response
    ///
    /// Used for refreshing a single news item after NewsUpdated broadcast.
    pub fn handle_news_show_response(
        &mut self,
        connection_id: usize,
        message_id: MessageId,
        success: bool,
        error: Option<String>,
        news: Option<NewsItem>,
    ) -> Task<Message> {
        let Some(conn) = self.connections.get_mut(&connection_id) else {
            return Task::none();
        };

        // Check if this was a tracked request
        let routing = conn.pending_requests.remove(&message_id);

        if success {
            if let Some(item) = news {
                // Handle based on routing
                if let Some(ResponseRouting::NewsShowForRefresh(id)) = routing
                    && item.id == id
                {
                    // Update or add the item in the list
                    if let Some(Ok(items)) = &mut conn.news_management.news_items {
                        let mut found = false;
                        for existing in items.iter_mut() {
                            if existing.id == item.id {
                                *existing = item.clone();
                                found = true;
                                break;
                            }
                        }

                        if !found {
                            items.push(item.clone());
                            items.sort_by_key(|i| i.id);
                        }
                    }

                    // Update image cache
                    if let Some(image_data) = &item.image
                        && let Some(cached) =
                            decode_data_uri_max_width(image_data, NEWS_IMAGE_MAX_CACHE_WIDTH)
                    {
                        conn.news_image_cache.insert(item.id, cached);
                    } else {
                        conn.news_image_cache.remove(&item.id);
                    }

                    // Update markdown cache
                    if let Some(body) = &item.body
                        && !body.is_empty()
                    {
                        let parsed: Vec<markdown::Item> = markdown::parse(body).collect();
                        conn.news_markdown_cache.insert(item.id, parsed);
                    } else {
                        conn.news_markdown_cache.remove(&item.id);
                    }
                }
            }
        } else {
            // Silently ignore errors for refresh requests
            let _ = error;
        }

        Task::none()
    }

    /// Handle news create response
    ///
    /// On success, returns to list view and refreshes.
    pub fn handle_news_create_response(
        &mut self,
        connection_id: usize,
        message_id: MessageId,
        success: bool,
        error: Option<String>,
        news: Option<NewsItem>,
    ) -> Task<Message> {
        let Some(conn) = self.connections.get_mut(&connection_id) else {
            return Task::none();
        };

        // Check if this was a tracked request
        let routing = conn.pending_requests.remove(&message_id);

        if success {
            // Show success message in chat
            let task =
                self.add_chat_message(connection_id, ChatMessage::system(t("msg-news-created")));

            // If from news panel, return to list and refresh
            if matches!(routing, Some(ResponseRouting::NewsCreateResult)) {
                // Add the new item to the list if we got it back
                if let Some(item) = news {
                    if let Some(conn) = self.connections.get_mut(&connection_id) {
                        // Cache image if present
                        if let Some(image_data) = &item.image
                            && let Some(cached) =
                                decode_data_uri_max_width(image_data, NEWS_IMAGE_MAX_CACHE_WIDTH)
                        {
                            conn.news_image_cache.insert(item.id, cached);
                        }

                        // Cache parsed markdown if body is present
                        if let Some(body) = &item.body
                            && !body.is_empty()
                        {
                            let parsed: Vec<markdown::Item> = markdown::parse(body).collect();
                            conn.news_markdown_cache.insert(item.id, parsed);
                        }

                        // Add to list
                        if let Some(Ok(items)) = &mut conn.news_management.news_items {
                            items.push(item);
                            items.sort_by_key(|i| i.id);
                        }

                        // Return to list mode
                        conn.news_management.reset_to_list();
                    }
                } else {
                    // No item returned, just refresh the list
                    return Task::batch([task, self.refresh_news_list_for(connection_id)]);
                }

                // Clear the text editor content
                self.news_body_content.remove(&connection_id);
            }

            return task;
        }

        // On error, show in the appropriate place
        if matches!(routing, Some(ResponseRouting::NewsCreateResult)) {
            if let Some(conn) = self.connections.get_mut(&connection_id) {
                conn.news_management.form_error = Some(error.unwrap_or_default());
            }
        } else {
            return self
                .add_chat_message(connection_id, ChatMessage::error(error.unwrap_or_default()));
        }

        Task::none()
    }

    /// Handle news edit response
    ///
    /// Populates the edit form with the news item details.
    pub fn handle_news_edit_response(
        &mut self,
        connection_id: usize,
        message_id: MessageId,
        success: bool,
        error: Option<String>,
        news: Option<NewsItem>,
    ) -> Task<Message> {
        let Some(conn) = self.connections.get_mut(&connection_id) else {
            return Task::none();
        };

        // Check if this was a tracked request
        let routing = conn.pending_requests.remove(&message_id);

        if success {
            if let Some(item) = news {
                // If from news panel, populate the edit form
                if matches!(routing, Some(ResponseRouting::PopulateNewsEdit)) {
                    // Set up the image in form state
                    conn.news_management.enter_edit_mode(item.id, item.image);

                    // Initialize the text editor content with the body and focus it
                    return self.init_news_edit_content(connection_id, item.body);
                }
            }
        } else {
            // On error, show in the appropriate place
            if matches!(routing, Some(ResponseRouting::PopulateNewsEdit)) {
                conn.news_management.list_error = Some(error.unwrap_or_default());
            } else {
                return self.add_chat_message(
                    connection_id,
                    ChatMessage::error(error.unwrap_or_default()),
                );
            }
        }

        Task::none()
    }

    /// Handle news update response
    ///
    /// On success, returns to list view.
    pub fn handle_news_update_response(
        &mut self,
        connection_id: usize,
        message_id: MessageId,
        success: bool,
        error: Option<String>,
        news: Option<NewsItem>,
    ) -> Task<Message> {
        let Some(conn) = self.connections.get_mut(&connection_id) else {
            return Task::none();
        };

        // Check if this was a tracked request
        let routing = conn.pending_requests.remove(&message_id);

        if success {
            // Show success message in chat
            let task =
                self.add_chat_message(connection_id, ChatMessage::system(t("msg-news-updated")));

            // If from news panel, update the list and return to list view
            if matches!(routing, Some(ResponseRouting::NewsUpdateResult)) {
                if let Some(item) = news {
                    if let Some(conn) = self.connections.get_mut(&connection_id) {
                        // Update image cache
                        if let Some(image_data) = &item.image {
                            if let Some(cached) =
                                decode_data_uri_max_width(image_data, NEWS_IMAGE_MAX_CACHE_WIDTH)
                            {
                                conn.news_image_cache.insert(item.id, cached);
                            }
                        } else {
                            conn.news_image_cache.remove(&item.id);
                        }

                        // Update markdown cache
                        if let Some(body) = &item.body
                            && !body.is_empty()
                        {
                            let parsed: Vec<markdown::Item> = markdown::parse(body).collect();
                            conn.news_markdown_cache.insert(item.id, parsed);
                        } else {
                            conn.news_markdown_cache.remove(&item.id);
                        }

                        // Update in list
                        if let Some(Ok(items)) = &mut conn.news_management.news_items {
                            for existing in items.iter_mut() {
                                if existing.id == item.id {
                                    *existing = item.clone();
                                    break;
                                }
                            }
                        }

                        // Return to list mode
                        conn.news_management.reset_to_list();
                    }
                } else {
                    // No item returned, just refresh
                    return Task::batch([task, self.refresh_news_list_for(connection_id)]);
                }

                // Clear the text editor content
                self.news_body_content.remove(&connection_id);
            }

            return task;
        }

        // On error, show in the appropriate place
        if matches!(routing, Some(ResponseRouting::NewsUpdateResult)) {
            if let Some(conn) = self.connections.get_mut(&connection_id) {
                conn.news_management.form_error = Some(error.unwrap_or_default());
            }
        } else {
            return self
                .add_chat_message(connection_id, ChatMessage::error(error.unwrap_or_default()));
        }

        Task::none()
    }

    /// Handle news delete response
    ///
    /// On success, removes the item from the list.
    pub fn handle_news_delete_response(
        &mut self,
        connection_id: usize,
        message_id: MessageId,
        success: bool,
        error: Option<String>,
        id: Option<i64>,
    ) -> Task<Message> {
        let Some(conn) = self.connections.get_mut(&connection_id) else {
            return Task::none();
        };

        // Check if this was a tracked request
        let routing = conn.pending_requests.remove(&message_id);

        if success {
            // Show success message in chat
            let task =
                self.add_chat_message(connection_id, ChatMessage::system(t("msg-news-deleted")));

            // If from news panel, close dialog and remove from list
            if matches!(routing, Some(ResponseRouting::NewsDeleteResult))
                && let Some(deleted_id) = id
                && let Some(conn) = self.connections.get_mut(&connection_id)
            {
                // Close the delete dialog
                conn.news_management.mode = NewsManagementMode::List;
                conn.news_management.delete_error = None;

                // Remove from list
                if let Some(Ok(items)) = &mut conn.news_management.news_items {
                    items.retain(|item| item.id != deleted_id);
                }

                // Remove from image cache
                conn.news_image_cache.remove(&deleted_id);

                // Remove from markdown cache
                conn.news_markdown_cache.remove(&deleted_id);
            }

            return task;
        }

        // On error, show in the delete dialog (keep it open for retry)
        if matches!(routing, Some(ResponseRouting::NewsDeleteResult)) {
            if let Some(conn) = self.connections.get_mut(&connection_id) {
                conn.news_management.delete_error = Some(error.unwrap_or_default());
            }
        } else {
            return self
                .add_chat_message(connection_id, ChatMessage::error(error.unwrap_or_default()));
        }

        Task::none()
    }

    /// Handle news updated broadcast
    ///
    /// Sent when any news item is created, updated, or deleted.
    pub fn handle_news_updated(
        &mut self,
        connection_id: usize,
        action: NewsAction,
        id: i64,
    ) -> Task<Message> {
        let Some(conn) = self.connections.get_mut(&connection_id) else {
            return Task::none();
        };

        // Only update if we're in the news panel and have loaded items
        if conn.active_panel != ActivePanel::News {
            return Task::none();
        }

        match action {
            NewsAction::Created | NewsAction::Updated => {
                // Fetch the updated/new item
                match conn.send(nexus_common::protocol::ClientMessage::NewsShow { id }) {
                    Ok(message_id) => {
                        conn.pending_requests
                            .track(message_id, ResponseRouting::NewsShowForRefresh(id));
                    }
                    Err(_) => {
                        // Silently fail - it's just a refresh
                    }
                }
            }
            NewsAction::Deleted => {
                // Remove from list
                if let Some(Ok(items)) = &mut conn.news_management.news_items {
                    items.retain(|item| item.id != id);
                }

                // Remove from image cache
                conn.news_image_cache.remove(&id);

                // Remove from markdown cache
                conn.news_markdown_cache.remove(&id);
            }
        }

        Task::none()
    }
}
