//! Event types and configuration for notifications

use std::collections::HashMap;
use std::fmt;

use serde::{Deserialize, Serialize};

use crate::i18n::t;

// =============================================================================
// Event Types
// =============================================================================

/// Types of events that can trigger notifications
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EventType {
    /// User message received
    #[default]
    UserMessage,
    /// Server broadcast received
    Broadcast,
    /// Chat message received
    ChatMessage,
    /// Nickname mentioned in chat
    ChatMention,
    /// Connection lost unexpectedly
    ConnectionLost,
    /// New news post created
    NewsPost,
    /// Permissions were changed
    PermissionsChanged,
    /// File transfer completed successfully
    TransferComplete,
    /// File transfer failed
    TransferFailed,
    /// A user connected to the server
    UserConnected,
    /// A user disconnected from the server
    UserDisconnected,
    /// You were kicked from the server
    UserKicked,
}

impl EventType {
    /// Get all event types
    pub fn all() -> &'static [EventType] {
        &[
            EventType::Broadcast,
            EventType::ChatMessage,
            EventType::ChatMention,
            EventType::ConnectionLost,
            EventType::NewsPost,
            EventType::PermissionsChanged,
            EventType::TransferComplete,
            EventType::TransferFailed,
            EventType::UserConnected,
            EventType::UserDisconnected,
            EventType::UserKicked,
            EventType::UserMessage,
        ]
    }

    /// Get the translation key for this event type's display name
    pub fn translation_key(&self) -> &'static str {
        match self {
            EventType::Broadcast => "event-broadcast",
            EventType::ChatMessage => "event-chat-message",
            EventType::ChatMention => "event-chat-mention",
            EventType::ConnectionLost => "event-connection-lost",
            EventType::NewsPost => "event-news-post",
            EventType::PermissionsChanged => "event-permissions-changed",
            EventType::TransferComplete => "event-transfer-complete",
            EventType::TransferFailed => "event-transfer-failed",
            EventType::UserConnected => "event-user-connected",
            EventType::UserDisconnected => "event-user-disconnected",
            EventType::UserKicked => "event-user-kicked",
            EventType::UserMessage => "event-user-message",
        }
    }
}

impl fmt::Display for EventType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", t(self.translation_key()))
    }
}

// =============================================================================
// Notification Content
// =============================================================================

/// Level of detail in notification content
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NotificationContent {
    /// Just the event type: "New user message"
    EventOnly,

    /// Include context: "Message from Alice"
    WithContext,

    /// Include preview: "Alice: Hey, are you there?"
    #[default]
    WithPreview,
}

impl NotificationContent {
    /// Get all notification content levels
    pub fn all() -> &'static [NotificationContent] {
        &[
            NotificationContent::EventOnly,
            NotificationContent::WithContext,
            NotificationContent::WithPreview,
        ]
    }

    /// Get the translation key for this content level's display name
    pub fn translation_key(&self) -> &'static str {
        match self {
            NotificationContent::EventOnly => "notification-content-simple",
            NotificationContent::WithContext => "notification-content-compact",
            NotificationContent::WithPreview => "notification-content-detailed",
        }
    }
}

impl fmt::Display for NotificationContent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", t(self.translation_key()))
    }
}

// =============================================================================
// Event Configuration
// =============================================================================

/// Configuration for a single event type
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct EventConfig {
    /// Whether to show a desktop notification
    #[serde(default)]
    pub show_notification: bool,

    /// Level of detail in notification content
    #[serde(default)]
    pub notification_content: NotificationContent,
}

impl EventConfig {
    /// Create a new EventConfig with notifications enabled
    pub fn with_notification() -> Self {
        Self {
            show_notification: true,
            notification_content: NotificationContent::default(),
        }
    }
}

// =============================================================================
// Event Settings
// =============================================================================

/// All event-related settings
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventSettings {
    /// Per-event configuration
    #[serde(default = "default_event_configs")]
    pub events: HashMap<EventType, EventConfig>,
}

impl Default for EventSettings {
    fn default() -> Self {
        Self {
            events: default_event_configs(),
        }
    }
}

impl EventSettings {
    /// Get the configuration for a specific event type
    pub fn get(&self, event_type: EventType) -> &EventConfig {
        self.events
            .get(&event_type)
            .unwrap_or(&DEFAULT_EVENT_CONFIG)
    }

    /// Get mutable configuration for a specific event type
    pub fn get_mut(&mut self, event_type: EventType) -> &mut EventConfig {
        self.events.entry(event_type).or_default()
    }
}

/// Default event config (used when an event type is not in the map)
static DEFAULT_EVENT_CONFIG: EventConfig = EventConfig {
    show_notification: false,
    notification_content: NotificationContent::WithPreview,
};

/// Create default event configurations with sensible defaults
fn default_event_configs() -> HashMap<EventType, EventConfig> {
    let mut events = HashMap::new();

    // Broadcasts: enabled by default
    events.insert(EventType::Broadcast, EventConfig::with_notification());

    // Chat messages: disabled by default (can be noisy)
    events.insert(EventType::ChatMessage, EventConfig::default());

    // Chat mentions: enabled by default
    events.insert(EventType::ChatMention, EventConfig::with_notification());

    // Connection lost: enabled by default
    events.insert(EventType::ConnectionLost, EventConfig::with_notification());

    // News posts: enabled by default
    events.insert(EventType::NewsPost, EventConfig::with_notification());

    // Permissions changed: enabled by default
    events.insert(
        EventType::PermissionsChanged,
        EventConfig::with_notification(),
    );

    // Transfer complete: enabled by default
    events.insert(
        EventType::TransferComplete,
        EventConfig::with_notification(),
    );

    // Transfer failed: enabled by default
    events.insert(EventType::TransferFailed, EventConfig::with_notification());

    // User connected: disabled by default (can be noisy on busy servers)
    events.insert(EventType::UserConnected, EventConfig::default());

    // User disconnected: disabled by default (can be noisy on busy servers)
    events.insert(EventType::UserDisconnected, EventConfig::default());

    // User kicked: enabled by default
    events.insert(EventType::UserKicked, EventConfig::with_notification());

    // User messages: enabled by default
    events.insert(EventType::UserMessage, EventConfig::with_notification());

    events
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_event_type_all() {
        let all = EventType::all();
        assert_eq!(all.len(), 12);
        assert!(all.contains(&EventType::Broadcast));
        assert!(all.contains(&EventType::ChatMessage));
        assert!(all.contains(&EventType::ChatMention));
        assert!(all.contains(&EventType::ConnectionLost));
        assert!(all.contains(&EventType::NewsPost));
        assert!(all.contains(&EventType::PermissionsChanged));
        assert!(all.contains(&EventType::TransferComplete));
        assert!(all.contains(&EventType::TransferFailed));
        assert!(all.contains(&EventType::UserConnected));
        assert!(all.contains(&EventType::UserDisconnected));
        assert!(all.contains(&EventType::UserKicked));
        assert!(all.contains(&EventType::UserMessage));
    }

    #[test]
    fn test_notification_content_all() {
        let all = NotificationContent::all();
        assert_eq!(all.len(), 3);
    }

    #[test]
    fn test_default_event_settings() {
        let settings = EventSettings::default();

        // User message should be enabled by default
        let user_msg_config = settings.get(EventType::UserMessage);
        assert!(user_msg_config.show_notification);
        assert_eq!(
            user_msg_config.notification_content,
            NotificationContent::WithPreview
        );
    }

    #[test]
    fn test_event_settings_serialization_roundtrip() {
        let settings = EventSettings::default();
        let json = serde_json::to_string(&settings).expect("serialize");
        let deserialized: EventSettings = serde_json::from_str(&json).expect("deserialize");

        assert_eq!(
            settings.get(EventType::UserMessage).show_notification,
            deserialized.get(EventType::UserMessage).show_notification
        );
    }

    #[test]
    fn test_event_settings_get_unknown_event() {
        let settings = EventSettings {
            events: HashMap::new(),
        };

        // Should return default config for unknown event
        let config = settings.get(EventType::UserMessage);
        assert!(!config.show_notification);
    }

    #[test]
    fn test_event_settings_get_mut() {
        let mut settings = EventSettings::default();

        // Modify the config
        settings.get_mut(EventType::UserMessage).show_notification = false;

        assert!(!settings.get(EventType::UserMessage).show_notification);
    }
}
