//! Server info display and edit state

use crate::image::{CachedImage, decode_data_uri_max_width};
use crate::style::SERVER_IMAGE_MAX_CACHE_WIDTH;
use nexus_common::validators::PasswordStrength;

// =============================================================================
// Server Info Display Tab
// =============================================================================

/// Tab selection for server info display mode
///
/// Tabs are shown based on available data:
/// - General: visible to all users (version, log level, connections, transfers, password strength)
/// - Files: visible to admins or users with file_reindex permission
/// - Chat: visible to users with chat_join permission (auto-join only) or admins (both)
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ServerInfoTab {
    /// General tab: version, log level, connections per IP, transfers per IP, password strength
    #[default]
    General,
    /// Files tab: reindex interval (admins + file_reindex permission)
    Files,
    /// Chat tab: auto-join (chat_join permission), persistent (admins only), flood config
    Chat,
}

// =============================================================================
// Server Info Edit State
// =============================================================================

/// Parameters for creating or comparing ServerInfoEditState.
/// Used to reduce the number of function arguments.
#[derive(Clone)]
pub struct ServerInfoParams<'a> {
    pub auto_join_channels: Option<&'a str>,
    pub chat_burst_limit: Option<u32>,
    pub chat_rate_limit: Option<u32>,
    pub description: Option<&'a str>,
    pub file_reindex_interval: Option<u32>,
    pub image: &'a str,
    pub max_connections_per_ip: Option<u32>,
    pub max_transfers_per_ip: Option<u32>,
    pub min_password_strength: PasswordStrength,
    pub name: Option<&'a str>,
    pub persistent_channels: Option<&'a str>,
    pub public_address: Option<&'a str>,
}

impl Default for ServerInfoParams<'_> {
    fn default() -> Self {
        Self {
            auto_join_channels: None,
            chat_burst_limit: None,
            chat_rate_limit: None,
            description: None,
            file_reindex_interval: None,
            image: "",
            max_connections_per_ip: None,
            max_transfers_per_ip: None,
            min_password_strength: PasswordStrength::Good,
            name: None,
            persistent_channels: None,
            public_address: None,
        }
    }
}

/// Server info edit panel state
///
/// Stores the form values for editing server configuration.
/// Only admins can access this form.
#[derive(Clone)]
pub struct ServerInfoEditState {
    /// Auto-join channels (space-separated)
    pub auto_join_channels: String,
    /// Cached image for preview (decoded from image field)
    pub cached_image: Option<CachedImage>,
    /// Chat burst limit (max messages in burst window)
    pub chat_burst_limit: u32,
    /// Chat rate limit (messages per second after burst)
    pub chat_rate_limit: u32,
    /// Server description (editable)
    pub description: String,
    /// Error message to display
    pub error: Option<String>,
    /// File reindex interval in minutes (editable, 0 = disabled)
    pub file_reindex_interval: Option<u32>,
    /// Server image data URI (editable, empty string means no image)
    pub image: String,
    /// Whether a submit is in progress (prevents double-submit)
    pub is_submitting: bool,
    /// Max connections per IP (editable, uses NumberInput)
    pub max_connections_per_ip: Option<u32>,
    /// Max transfers per IP (editable, uses NumberInput)
    pub max_transfers_per_ip: Option<u32>,
    /// Minimum password strength required for user accounts
    pub min_password_strength: PasswordStrength,
    /// Server name (editable)
    pub name: String,
    /// Persistent channels (space-separated)
    pub persistent_channels: String,
    /// Public address for `nexus://` URI sharing (editable; empty = unset)
    pub public_address: String,
}

// Manual Debug implementation because CachedImage doesn't implement Debug
impl std::fmt::Debug for ServerInfoEditState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ServerInfoEditState")
            .field("auto_join_channels", &self.auto_join_channels)
            .field(
                "cached_image",
                &self.cached_image.as_ref().map(|_| "<cached>"),
            )
            .field("chat_burst_limit", &self.chat_burst_limit)
            .field("chat_rate_limit", &self.chat_rate_limit)
            .field("description", &self.description)
            .field("error", &self.error)
            .field("file_reindex_interval", &self.file_reindex_interval)
            .field("image", &format!("<{} bytes>", self.image.len()))
            .field("is_submitting", &self.is_submitting)
            .field("max_connections_per_ip", &self.max_connections_per_ip)
            .field("max_transfers_per_ip", &self.max_transfers_per_ip)
            .field("min_password_strength", &self.min_password_strength)
            .field("name", &self.name)
            .field("persistent_channels", &self.persistent_channels)
            .field("public_address", &self.public_address)
            .finish()
    }
}

impl ServerInfoEditState {
    /// Create a new server info edit state with current values
    pub fn new(params: ServerInfoParams<'_>) -> Self {
        // Decode image for preview
        let cached_image = if params.image.is_empty() {
            None
        } else {
            decode_data_uri_max_width(params.image, SERVER_IMAGE_MAX_CACHE_WIDTH)
        };

        Self {
            auto_join_channels: params.auto_join_channels.unwrap_or("").to_string(),
            cached_image,
            chat_burst_limit: params.chat_burst_limit.unwrap_or(0),
            chat_rate_limit: params.chat_rate_limit.unwrap_or(0),
            description: params.description.unwrap_or("").to_string(),
            error: None,
            file_reindex_interval: params.file_reindex_interval,
            image: params.image.to_string(),
            is_submitting: false,
            max_connections_per_ip: params.max_connections_per_ip,
            max_transfers_per_ip: params.max_transfers_per_ip,
            min_password_strength: params.min_password_strength,
            name: params.name.unwrap_or("").to_string(),
            persistent_channels: params.persistent_channels.unwrap_or("").to_string(),
            public_address: params.public_address.unwrap_or("").to_string(),
        }
    }

    /// Check if the form has any changes compared to original values
    pub fn has_changes(&self, original: &ServerInfoParams<'_>) -> bool {
        let auto_join_changed =
            self.auto_join_channels != original.auto_join_channels.unwrap_or("");
        let chat_burst_limit_changed =
            self.chat_burst_limit != original.chat_burst_limit.unwrap_or(0);
        let chat_rate_limit_changed = self.chat_rate_limit != original.chat_rate_limit.unwrap_or(0);
        let desc_changed = self.description != original.description.unwrap_or("");
        let reindex_changed = self.file_reindex_interval != original.file_reindex_interval;
        let image_changed = self.image != original.image;
        let max_conn_changed = self.max_connections_per_ip != original.max_connections_per_ip;
        let max_xfer_changed = self.max_transfers_per_ip != original.max_transfers_per_ip;
        let min_password_strength_changed =
            self.min_password_strength != original.min_password_strength;
        let name_changed = self.name != original.name.unwrap_or("");
        let persistent_changed =
            self.persistent_channels != original.persistent_channels.unwrap_or("");
        let public_address_changed = self.public_address != original.public_address.unwrap_or("");
        auto_join_changed
            || chat_burst_limit_changed
            || chat_rate_limit_changed
            || desc_changed
            || reindex_changed
            || image_changed
            || max_conn_changed
            || max_xfer_changed
            || min_password_strength_changed
            || name_changed
            || persistent_changed
            || public_address_changed
    }
}
