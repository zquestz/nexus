//! Event notification system
//!
//! This module handles emitting desktop notifications for various events
//! based on user configuration.

use std::sync::Mutex;
use std::time::{Duration, Instant};

use notify_rust::Notification;
#[cfg(all(unix, not(target_os = "macos")))]
use notify_rust::NotificationHandle;

// Keep notification handles alive to prevent GNOME/Cinnamon from dismissing them.
// These desktop environments close notifications when the D-Bus connection drops,
// so we hold onto handles until they expire naturally.
// See: https://gitlab.gnome.org/GNOME/gnome-shell/-/issues/8797
#[cfg(all(unix, not(target_os = "macos")))]
static NOTIFICATION_HANDLES: Mutex<Vec<(Instant, NotificationHandle)>> = Mutex::new(Vec::new());

/// How long to keep notification handles alive (slightly longer than the notification timeout)
#[cfg(all(unix, not(target_os = "macos")))]
const HANDLE_LIFETIME: Duration = Duration::from_secs(6);

use crate::NexusApp;
use crate::config::events::{EventType, NotificationContent};
use crate::i18n::{t, t_args};
use crate::types::{ActivePanel, ChatTab};

// =============================================================================
// Constants
// =============================================================================

/// Maximum length for file paths in notifications before truncating
const MAX_PATH_DISPLAY_LENGTH: usize = 50;

// =============================================================================
// Event Context
// =============================================================================

/// Context data for event notifications
///
/// Contains optional fields that can be used to build notification content
/// depending on the event type and configured detail level.
#[derive(Debug, Clone, Default)]
pub struct EventContext {
    /// Connection ID where the event occurred
    pub connection_id: Option<usize>,

    /// Username or nickname associated with the event
    pub username: Option<String>,

    /// Message content (for chat/PM events)
    pub message: Option<String>,

    /// Server name associated with the event
    pub server_name: Option<String>,

    /// File path (for transfer events)
    pub path: Option<String>,

    /// Error message (for transfer failures)
    pub error: Option<String>,

    /// Whether this is an upload (true) or download (false) - for transfer events
    pub is_upload: Option<bool>,
}

impl EventContext {
    /// Create a new empty event context
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the connection ID field
    pub fn with_connection_id(mut self, connection_id: usize) -> Self {
        self.connection_id = Some(connection_id);
        self
    }

    /// Set the username field
    pub fn with_username(mut self, username: impl Into<String>) -> Self {
        self.username = Some(username.into());
        self
    }

    /// Set the message field
    pub fn with_message(mut self, message: impl Into<String>) -> Self {
        self.message = Some(message.into());
        self
    }

    /// Set the server name field
    pub fn with_server_name(mut self, server_name: impl Into<String>) -> Self {
        self.server_name = Some(server_name.into());
        self
    }

    /// Set the file path field (for transfer events)
    pub fn with_path(mut self, path: impl Into<String>) -> Self {
        self.path = Some(path.into());
        self
    }

    /// Set the error field (for transfer failures)
    pub fn with_error(mut self, error: impl Into<String>) -> Self {
        self.error = Some(error.into());
        self
    }

    /// Set upload flag (for transfer events)
    pub fn with_is_upload(mut self, is_upload: bool) -> Self {
        self.is_upload = Some(is_upload);
        self
    }
}

// =============================================================================
// Notification Emission
// =============================================================================

/// Check if a notification should be shown and emit it if so
///
/// This function checks the user's event configuration and current application
/// state to determine whether a notification should be displayed.
pub fn emit_event(app: &NexusApp, event_type: EventType, context: EventContext) {
    // Check global notifications toggle first
    if !app.config.settings.notifications_enabled {
        return;
    }

    let config = app.config.settings.event_settings.get(event_type);

    // Check if notifications are enabled for this event
    if !config.show_notification {
        return;
    }

    // Check event-specific conditions
    if !should_show_notification(app, event_type, &context) {
        return;
    }

    // Build and show the notification
    let (summary, body) =
        build_notification_content(event_type, &context, config.notification_content);

    // Build and show the notification
    let mut notification = Notification::new();
    notification
        .appname("Nexus BBS")
        .summary(&summary)
        .body(body.as_deref().unwrap_or(""))
        .auto_icon()
        .timeout(notify_rust::Timeout::Milliseconds(5000));

    // On Linux, keep handle alive to prevent GNOME/Cinnamon from dismissing
    // notifications when the D-Bus connection would otherwise be dropped.
    #[cfg(all(unix, not(target_os = "macos")))]
    if let Ok(handle) = notification.show()
        && let Ok(mut handles) = NOTIFICATION_HANDLES.lock()
    {
        let now = Instant::now();
        handles.retain(|(created, _)| now.duration_since(*created) < HANDLE_LIFETIME);
        handles.push((now, handle));
    }

    // On non-Linux platforms, just show and ignore result
    #[cfg(not(all(unix, not(target_os = "macos"))))]
    let _ = notification.show();
}

/// Determine if a notification should be shown based on app state
///
/// This checks conditions beyond just "is notification enabled" - for example,
/// whether the user is already viewing the relevant content.
fn should_show_notification(app: &NexusApp, event_type: EventType, context: &EventContext) -> bool {
    match event_type {
        EventType::UserMessage => {
            // Don't notify if window is focused AND this connection is active AND we're viewing that user's PM tab
            if app.window_focused
                && let Some(event_conn_id) = context.connection_id
                && let Some(active_conn_id) = app.active_connection
                && event_conn_id == active_conn_id
                && let Some(conn) = app.connections.get(&event_conn_id)
                && let Some(ref username) = context.username
                && conn.active_chat_tab == ChatTab::UserMessage(username.clone())
            {
                return false;
            }
            true
        }
        EventType::Broadcast => {
            // Don't notify if window is focused
            !app.window_focused
        }
        EventType::ChatMention => {
            // Don't notify if window is focused AND viewing server chat on this connection
            if app.window_focused
                && let Some(event_conn_id) = context.connection_id
                && let Some(active_conn_id) = app.active_connection
                && event_conn_id == active_conn_id
                && let Some(conn) = app.connections.get(&event_conn_id)
                && conn.active_chat_tab == ChatTab::Server
                && conn.active_panel == ActivePanel::None
            {
                return false;
            }
            true
        }
        EventType::NewsPost => {
            // Don't notify if News panel is open on this connection
            if let Some(event_conn_id) = context.connection_id
                && let Some(active_conn_id) = app.active_connection
                && event_conn_id == active_conn_id
                && let Some(conn) = app.connections.get(&event_conn_id)
                && conn.active_panel == ActivePanel::News
            {
                return false;
            }
            true
        }
        EventType::ChatMessage => {
            // Don't notify if window is focused AND viewing server chat on this connection
            if app.window_focused
                && let Some(event_conn_id) = context.connection_id
                && let Some(active_conn_id) = app.active_connection
                && event_conn_id == active_conn_id
                && let Some(conn) = app.connections.get(&event_conn_id)
                && conn.active_chat_tab == ChatTab::Server
                && conn.active_panel == ActivePanel::None
            {
                return false;
            }
            true
        }
        EventType::ConnectionLost => {
            // Don't notify if window is focused - user will see the disconnection
            !app.window_focused
        }
        EventType::PermissionsChanged => {
            // Don't notify if window is focused - user will see the change
            !app.window_focused
        }
        EventType::TransferComplete => {
            // Don't notify if window is focused AND Transfers panel is open
            if app.window_focused && app.ui_state.active_panel == ActivePanel::Transfers {
                return false;
            }
            true
        }
        EventType::TransferFailed => {
            // Don't notify if window is focused AND Transfers panel is open
            if app.window_focused && app.ui_state.active_panel == ActivePanel::Transfers {
                return false;
            }
            true
        }
        EventType::UserConnected => {
            // Don't notify if window is focused - user will see in the user list
            !app.window_focused
        }
        EventType::UserDisconnected => {
            // Don't notify if window is focused - user will see in the user list
            !app.window_focused
        }
        EventType::UserKicked => {
            // Don't notify if window is focused - user will see the kick message
            !app.window_focused
        }
    }
}

/// Build notification summary and body based on content level
fn build_notification_content(
    event_type: EventType,
    context: &EventContext,
    content_level: NotificationContent,
) -> (String, Option<String>) {
    match event_type {
        EventType::Broadcast => build_broadcast_notification(context, content_level),
        EventType::ChatMessage => build_chat_message_notification(context, content_level),
        EventType::ChatMention => build_chat_mention_notification(context, content_level),
        EventType::ConnectionLost => build_connection_lost_notification(context, content_level),
        EventType::NewsPost => build_news_post_notification(context, content_level),
        EventType::PermissionsChanged => {
            build_permissions_changed_notification(context, content_level)
        }
        EventType::TransferComplete => build_transfer_complete_notification(context, content_level),
        EventType::TransferFailed => build_transfer_failed_notification(context, content_level),
        EventType::UserConnected => build_user_connected_notification(context, content_level),
        EventType::UserDisconnected => build_user_disconnected_notification(context, content_level),
        EventType::UserKicked => build_user_kicked_notification(context, content_level),
        EventType::UserMessage => build_user_message_notification(context, content_level),
    }
}

/// Build notification content for user message events
fn build_user_message_notification(
    context: &EventContext,
    content_level: NotificationContent,
) -> (String, Option<String>) {
    match content_level {
        NotificationContent::EventOnly => {
            // "New user message"
            (t("notification-user-message"), None)
        }
        NotificationContent::WithContext => {
            // "Message from Alice"
            let summary = if let Some(ref username) = context.username {
                t_args("notification-user-message-from", &[("username", username)])
            } else {
                t("notification-user-message")
            };
            (summary, None)
        }
        NotificationContent::WithPreview => {
            // Summary: "Message from Alice"
            // Body: "Hey, are you there?"
            let summary = if let Some(ref username) = context.username {
                t_args("notification-user-message-from", &[("username", username)])
            } else {
                t("notification-user-message")
            };
            let body = context.message.clone();
            (summary, body)
        }
    }
}

/// Build notification content for chat message events
fn build_chat_message_notification(
    context: &EventContext,
    content_level: NotificationContent,
) -> (String, Option<String>) {
    match content_level {
        NotificationContent::EventOnly => {
            // "New chat message"
            (t("notification-chat-message"), None)
        }
        NotificationContent::WithContext => {
            // "Message from Alice"
            let summary = if let Some(ref username) = context.username {
                t_args("notification-chat-message-from", &[("username", username)])
            } else {
                t("notification-chat-message")
            };
            (summary, None)
        }
        NotificationContent::WithPreview => {
            // Summary: "Message from Alice"
            // Body: "Hello everyone!"
            let summary = if let Some(ref username) = context.username {
                t_args("notification-chat-message-from", &[("username", username)])
            } else {
                t("notification-chat-message")
            };
            let body = context.message.clone();
            (summary, body)
        }
    }
}

/// Build notification content for broadcast events
fn build_broadcast_notification(
    context: &EventContext,
    content_level: NotificationContent,
) -> (String, Option<String>) {
    match content_level {
        NotificationContent::EventOnly => {
            // "Server broadcast"
            (t("notification-broadcast"), None)
        }
        NotificationContent::WithContext => {
            // "Broadcast from Alice"
            let summary = if let Some(ref username) = context.username {
                t_args("notification-broadcast-from", &[("username", username)])
            } else {
                t("notification-broadcast")
            };
            (summary, None)
        }
        NotificationContent::WithPreview => {
            // Summary: "Broadcast from Alice"
            // Body: "Server maintenance in 10 minutes"
            let summary = if let Some(ref username) = context.username {
                t_args("notification-broadcast-from", &[("username", username)])
            } else {
                t("notification-broadcast")
            };
            let body = context.message.clone();
            (summary, body)
        }
    }
}

/// Build notification content for chat mention events
fn build_chat_mention_notification(
    context: &EventContext,
    content_level: NotificationContent,
) -> (String, Option<String>) {
    match content_level {
        NotificationContent::EventOnly => {
            // "You were mentioned"
            (t("notification-chat-mention"), None)
        }
        NotificationContent::WithContext => {
            // "Mentioned by Alice"
            let summary = if let Some(ref username) = context.username {
                t_args("notification-chat-mention-by", &[("username", username)])
            } else {
                t("notification-chat-mention")
            };
            (summary, None)
        }
        NotificationContent::WithPreview => {
            // Summary: "Mentioned by Alice"
            // Body: "Hey @Bob, are you there?"
            let summary = if let Some(ref username) = context.username {
                t_args("notification-chat-mention-by", &[("username", username)])
            } else {
                t("notification-chat-mention")
            };
            let body = context.message.clone();
            (summary, body)
        }
    }
}

/// Build notification content for news post events
fn build_news_post_notification(
    context: &EventContext,
    content_level: NotificationContent,
) -> (String, Option<String>) {
    match content_level {
        NotificationContent::EventOnly => {
            // "New news post"
            (t("notification-news-post"), None)
        }
        NotificationContent::WithContext => {
            // "News post by Alice"
            let summary = if let Some(ref username) = context.username {
                t_args("notification-news-post-by", &[("username", username)])
            } else {
                t("notification-news-post")
            };
            (summary, None)
        }
        NotificationContent::WithPreview => {
            // Summary: "News post by Alice"
            // Body: First line or truncated preview of the post
            let summary = if let Some(ref username) = context.username {
                t_args("notification-news-post-by", &[("username", username)])
            } else {
                t("notification-news-post")
            };
            // Use message field for news body preview (truncated)
            let body = context.message.as_ref().map(|msg| {
                // Take first line or truncate to reasonable length
                let first_line = msg.lines().next().unwrap_or(msg);
                if first_line.len() > 100 {
                    format!("{}...", &first_line[..97])
                } else {
                    first_line.to_string()
                }
            });
            (summary, body)
        }
    }
}

/// Build notification content for connection lost events
fn build_connection_lost_notification(
    context: &EventContext,
    content_level: NotificationContent,
) -> (String, Option<String>) {
    match content_level {
        NotificationContent::EventOnly => {
            // "Connection lost"
            (t("notification-connection-lost"), None)
        }
        NotificationContent::WithContext | NotificationContent::WithPreview => {
            // "Disconnected from ServerName"
            let summary = if let Some(ref server_name) = context.server_name {
                t_args(
                    "notification-connection-lost-from",
                    &[("server", server_name)],
                )
            } else {
                t("notification-connection-lost")
            };
            // Body contains error message if available
            let body = context.message.clone();
            (summary, body)
        }
    }
}

/// Build notification content for transfer complete events
fn build_transfer_complete_notification(
    context: &EventContext,
    content_level: NotificationContent,
) -> (String, Option<String>) {
    match content_level {
        NotificationContent::EventOnly => {
            // "Transfer complete"
            (t("notification-transfer-complete"), None)
        }
        NotificationContent::WithContext | NotificationContent::WithPreview => {
            // "Download complete" or "Upload complete"
            let summary = if context.is_upload == Some(true) {
                t("notification-upload-complete")
            } else if context.is_upload == Some(false) {
                t("notification-download-complete")
            } else {
                t("notification-transfer-complete")
            };
            // Body contains filename (truncated if needed)
            let body = context.path.as_ref().map(|path| truncate_path(path));
            (summary, body)
        }
    }
}

/// Build notification content for transfer failed events
fn build_transfer_failed_notification(
    context: &EventContext,
    content_level: NotificationContent,
) -> (String, Option<String>) {
    match content_level {
        NotificationContent::EventOnly => {
            // "Transfer failed"
            (t("notification-transfer-failed"), None)
        }
        NotificationContent::WithContext => {
            // "Download failed" or "Upload failed"
            let summary = if context.is_upload == Some(true) {
                t("notification-upload-failed")
            } else if context.is_upload == Some(false) {
                t("notification-download-failed")
            } else {
                t("notification-transfer-failed")
            };
            // Body contains filename
            let body = context.path.as_ref().map(|path| truncate_path(path));
            (summary, body)
        }
        NotificationContent::WithPreview => {
            // "Download failed" or "Upload failed"
            // Body: "filename: error message"
            let summary = if context.is_upload == Some(true) {
                t("notification-upload-failed")
            } else if context.is_upload == Some(false) {
                t("notification-download-failed")
            } else {
                t("notification-transfer-failed")
            };
            // Body contains filename and error
            let body = match (&context.path, &context.error) {
                (Some(path), Some(error)) => Some(format!("{}: {}", truncate_path(path), error)),
                (Some(path), None) => Some(truncate_path(path)),
                (None, Some(error)) => Some(error.clone()),
                (None, None) => None,
            };
            (summary, body)
        }
    }
}

/// Truncate a file path for display in notifications
fn truncate_path(path: &str) -> String {
    if path.len() <= MAX_PATH_DISPLAY_LENGTH {
        path.to_string()
    } else {
        // Show "...last_part" to keep the filename visible
        format!("...{}", &path[path.len() - MAX_PATH_DISPLAY_LENGTH + 3..])
    }
}

/// Build notification content for permissions changed events
fn build_permissions_changed_notification(
    context: &EventContext,
    content_level: NotificationContent,
) -> (String, Option<String>) {
    match content_level {
        NotificationContent::EventOnly => {
            // "Permissions changed"
            (t("notification-permissions-changed"), None)
        }
        NotificationContent::WithContext | NotificationContent::WithPreview => {
            // "Permissions changed on ServerName"
            let summary = if let Some(ref server_name) = context.server_name {
                t_args(
                    "notification-permissions-changed-on",
                    &[("server", server_name)],
                )
            } else {
                t("notification-permissions-changed")
            };
            (summary, None)
        }
    }
}

/// Build notification content for user connected events
fn build_user_connected_notification(
    context: &EventContext,
    content_level: NotificationContent,
) -> (String, Option<String>) {
    match content_level {
        NotificationContent::EventOnly => {
            // "User connected"
            (t("notification-user-connected"), None)
        }
        NotificationContent::WithContext | NotificationContent::WithPreview => {
            // "Alice connected"
            let summary = if let Some(ref username) = context.username {
                t_args(
                    "notification-user-connected-name",
                    &[("username", username)],
                )
            } else {
                t("notification-user-connected")
            };
            (summary, None)
        }
    }
}

/// Build notification content for user disconnected events
fn build_user_disconnected_notification(
    context: &EventContext,
    content_level: NotificationContent,
) -> (String, Option<String>) {
    match content_level {
        NotificationContent::EventOnly => {
            // "User disconnected"
            (t("notification-user-disconnected"), None)
        }
        NotificationContent::WithContext | NotificationContent::WithPreview => {
            // "Alice disconnected"
            let summary = if let Some(ref username) = context.username {
                t_args(
                    "notification-user-disconnected-name",
                    &[("username", username)],
                )
            } else {
                t("notification-user-disconnected")
            };
            (summary, None)
        }
    }
}

/// Build notification content for user kicked events
fn build_user_kicked_notification(
    context: &EventContext,
    content_level: NotificationContent,
) -> (String, Option<String>) {
    match content_level {
        NotificationContent::EventOnly => {
            // "You were kicked"
            (t("notification-user-kicked"), None)
        }
        NotificationContent::WithContext => {
            // "Kicked from ServerName"
            let summary = if let Some(ref server_name) = context.server_name {
                t_args("notification-user-kicked-from", &[("server", server_name)])
            } else {
                t("notification-user-kicked")
            };
            (summary, None)
        }
        NotificationContent::WithPreview => {
            // Summary: "Kicked from ServerName"
            // Body: "Kicked by admin"
            let summary = if let Some(ref server_name) = context.server_name {
                t_args("notification-user-kicked-from", &[("server", server_name)])
            } else {
                t("notification-user-kicked")
            };
            // Body contains the kick message/reason
            let body = context.message.clone();
            (summary, body)
        }
    }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_event_context_builder() {
        let context = EventContext::new()
            .with_connection_id(42)
            .with_username("alice")
            .with_message("Hello!")
            .with_server_name("Test Server");

        assert_eq!(context.connection_id, Some(42));
        assert_eq!(context.username, Some("alice".to_string()));
        assert_eq!(context.message, Some("Hello!".to_string()));
        assert_eq!(context.server_name, Some("Test Server".to_string()));
    }

    #[test]
    fn test_event_context_default() {
        let context = EventContext::default();

        assert!(context.connection_id.is_none());
        assert!(context.username.is_none());
        assert!(context.message.is_none());
        assert!(context.server_name.is_none());
    }
}
