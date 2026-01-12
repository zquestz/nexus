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
    /// Server broadcast received
    #[default]
    Broadcast,
    /// A user joined a channel you're in
    ChatJoin,
    /// A user left a channel you're in
    ChatLeave,
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
    /// User message received
    UserMessage,
}

impl EventType {
    /// Get all event types
    pub fn all() -> &'static [EventType] {
        &[
            EventType::Broadcast,
            EventType::ChatJoin,
            EventType::ChatLeave,
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
            EventType::ChatJoin => "event-chat-join",
            EventType::ChatLeave => "event-chat-leave",
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
// Sound Choice
// =============================================================================

/// Available sounds for event notifications
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum SoundChoice {
    /// Alert sound - attention-grabbing two-tone
    #[default]
    Alert,
    /// Bell sound - classic bell with longer decay
    Bell,
    /// Chime sound - pleasant melodic chime
    Chime,
    /// Ding sound - single clean high ding
    Ding,
    /// Pop sound - short soft pop
    Pop,
}

impl<'de> serde::Deserialize<'de> for SoundChoice {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        Ok(match s.as_str() {
            "alert" => SoundChoice::Alert,
            "bell" => SoundChoice::Bell,
            "chime" => SoundChoice::Chime,
            "ding" => SoundChoice::Ding,
            "pop" => SoundChoice::Pop,
            // Unknown values (including legacy "none") default to Alert
            _ => SoundChoice::default(),
        })
    }
}

impl serde::Serialize for SoundChoice {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let s = match self {
            SoundChoice::Alert => "alert",
            SoundChoice::Bell => "bell",
            SoundChoice::Chime => "chime",
            SoundChoice::Ding => "ding",
            SoundChoice::Pop => "pop",
        };
        serializer.serialize_str(s)
    }
}

impl SoundChoice {
    /// Get all sound choices
    pub fn all() -> &'static [SoundChoice] {
        &[
            SoundChoice::Alert,
            SoundChoice::Bell,
            SoundChoice::Chime,
            SoundChoice::Ding,
            SoundChoice::Pop,
        ]
    }

    /// Get the translation key for this sound's display name
    pub fn translation_key(&self) -> &'static str {
        match self {
            SoundChoice::Alert => "sound-alert",
            SoundChoice::Bell => "sound-bell",
            SoundChoice::Chime => "sound-chime",
            SoundChoice::Ding => "sound-ding",
            SoundChoice::Pop => "sound-pop",
        }
    }
}

impl fmt::Display for SoundChoice {
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

    /// Whether to play a sound for this event
    #[serde(default)]
    pub play_sound: bool,

    /// Which sound to play
    #[serde(default)]
    pub sound: SoundChoice,

    /// Play sound even when window is focused/viewing relevant content
    #[serde(default)]
    pub always_play_sound: bool,
}

impl EventConfig {
    /// Create a new EventConfig with notifications enabled
    pub fn with_notification() -> Self {
        Self {
            show_notification: true,
            notification_content: NotificationContent::default(),
            play_sound: false,
            sound: SoundChoice::Alert,
            always_play_sound: false,
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
    play_sound: false,
    sound: SoundChoice::Alert,
    always_play_sound: false,
};

/// Create default event configurations with sensible defaults
fn default_event_configs() -> HashMap<EventType, EventConfig> {
    let mut events = HashMap::new();

    // Broadcasts: enabled by default
    events.insert(EventType::Broadcast, EventConfig::with_notification());

    // Chat join: disabled by default (can be noisy)
    events.insert(EventType::ChatJoin, EventConfig::default());

    // Chat leave: disabled by default (can be noisy)
    events.insert(EventType::ChatLeave, EventConfig::default());

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
        assert_eq!(all.len(), 14);
        assert!(all.contains(&EventType::Broadcast));
        assert!(all.contains(&EventType::ChatJoin));
        assert!(all.contains(&EventType::ChatLeave));
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
    fn test_sound_choice_all() {
        let all = SoundChoice::all();
        assert_eq!(all.len(), 5);
        assert!(all.contains(&SoundChoice::Alert));
        assert!(all.contains(&SoundChoice::Bell));
        assert!(all.contains(&SoundChoice::Chime));
        assert!(all.contains(&SoundChoice::Ding));
        assert!(all.contains(&SoundChoice::Pop));
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
        // Sound should be off by default but set to Alert
        assert!(!user_msg_config.play_sound);
        assert_eq!(user_msg_config.sound, SoundChoice::Alert);
        assert!(!user_msg_config.always_play_sound);
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
        assert_eq!(
            settings.get(EventType::UserMessage).play_sound,
            deserialized.get(EventType::UserMessage).play_sound
        );
        assert_eq!(
            settings.get(EventType::UserMessage).sound,
            deserialized.get(EventType::UserMessage).sound
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
        assert!(!config.play_sound);
    }

    #[test]
    fn test_event_settings_get_mut() {
        let mut settings = EventSettings::default();

        // Modify the config
        settings.get_mut(EventType::UserMessage).show_notification = false;
        settings.get_mut(EventType::UserMessage).play_sound = true;

        assert!(!settings.get(EventType::UserMessage).show_notification);
        assert!(settings.get(EventType::UserMessage).play_sound);
    }

    #[test]
    fn test_sound_choice_serialization() {
        // Test Alert serialization
        let alert_json = serde_json::to_string(&SoundChoice::Alert).expect("serialize");
        assert_eq!(alert_json, "\"alert\"");

        // Test Bell serialization
        let bell_json = serde_json::to_string(&SoundChoice::Bell).expect("serialize");
        assert_eq!(bell_json, "\"bell\"");

        // Test deserialization
        let alert: SoundChoice = serde_json::from_str("\"alert\"").expect("deserialize");
        assert_eq!(alert, SoundChoice::Alert);

        let bell: SoundChoice = serde_json::from_str("\"bell\"").expect("deserialize");
        assert_eq!(bell, SoundChoice::Bell);
    }

    #[test]
    fn test_sound_choice_unknown_defaults_to_alert() {
        // Unknown values should default to Alert
        let sound: SoundChoice = serde_json::from_str("\"unknown\"").expect("deserialize");
        assert_eq!(sound, SoundChoice::Alert);

        // Legacy "none" value should also default to Alert
        let none: SoundChoice = serde_json::from_str("\"none\"").expect("deserialize");
        assert_eq!(none, SoundChoice::Alert);
    }
}
