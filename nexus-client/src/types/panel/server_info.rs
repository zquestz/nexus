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
/// - Channels: visible to users with chat_join permission (auto-join only) or admins (both)
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ServerInfoTab {
    /// General tab: version, log level, connections per IP, transfers per IP, password strength
    #[default]
    General,
    /// Files tab: reindex interval (admins + file_reindex permission)
    Files,
    /// Channels tab: auto-join (chat_join permission), persistent (admins only)
    Channels,
}

// =============================================================================
// Server Info Edit State
// =============================================================================

/// Parameters for creating or comparing ServerInfoEditState.
/// Used to reduce the number of function arguments.
#[derive(Clone)]
pub struct ServerInfoParams<'a> {
    pub name: Option<&'a str>,
    pub description: Option<&'a str>,
    pub max_connections_per_ip: Option<u32>,
    pub max_transfers_per_ip: Option<u32>,
    pub image: &'a str,
    pub file_reindex_interval: Option<u32>,
    pub persistent_channels: Option<&'a str>,
    pub auto_join_channels: Option<&'a str>,
    pub min_password_strength: PasswordStrength,
}

impl Default for ServerInfoParams<'_> {
    fn default() -> Self {
        Self {
            name: None,
            description: None,
            max_connections_per_ip: None,
            max_transfers_per_ip: None,
            image: "",
            file_reindex_interval: None,
            persistent_channels: None,
            auto_join_channels: None,
            min_password_strength: PasswordStrength::Good,
        }
    }
}

/// Server info edit panel state
///
/// Stores the form values for editing server configuration.
/// Only admins can access this form.
#[derive(Clone)]
pub struct ServerInfoEditState {
    /// Server name (editable)
    pub name: String,
    /// Server description (editable)
    pub description: String,
    /// Max connections per IP (editable, uses NumberInput)
    pub max_connections_per_ip: Option<u32>,
    /// Max transfers per IP (editable, uses NumberInput)
    pub max_transfers_per_ip: Option<u32>,
    /// Server image data URI (editable, empty string means no image)
    pub image: String,
    /// File reindex interval in minutes (editable, 0 = disabled)
    pub file_reindex_interval: Option<u32>,
    /// Persistent channels (space-separated)
    pub persistent_channels: String,
    /// Auto-join channels (space-separated)
    pub auto_join_channels: String,
    /// Minimum password strength required for user accounts
    pub min_password_strength: PasswordStrength,
    /// Cached image for preview (decoded from image field)
    pub cached_image: Option<CachedImage>,
    /// Error message to display
    pub error: Option<String>,
    /// Whether a submit is in progress (prevents double-submit)
    pub is_submitting: bool,
}

// Manual Debug implementation because CachedImage doesn't implement Debug
impl std::fmt::Debug for ServerInfoEditState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ServerInfoEditState")
            .field("name", &self.name)
            .field("description", &self.description)
            .field("max_connections_per_ip", &self.max_connections_per_ip)
            .field("max_transfers_per_ip", &self.max_transfers_per_ip)
            .field("image", &format!("<{} bytes>", self.image.len()))
            .field("file_reindex_interval", &self.file_reindex_interval)
            .field("persistent_channels", &self.persistent_channels)
            .field("auto_join_channels", &self.auto_join_channels)
            .field("min_password_strength", &self.min_password_strength)
            .field(
                "cached_image",
                &self.cached_image.as_ref().map(|_| "<cached>"),
            )
            .field("error", &self.error)
            .field("is_submitting", &self.is_submitting)
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
            name: params.name.unwrap_or("").to_string(),
            description: params.description.unwrap_or("").to_string(),
            max_connections_per_ip: params.max_connections_per_ip,
            max_transfers_per_ip: params.max_transfers_per_ip,
            image: params.image.to_string(),
            file_reindex_interval: params.file_reindex_interval,
            persistent_channels: params.persistent_channels.unwrap_or("").to_string(),
            auto_join_channels: params.auto_join_channels.unwrap_or("").to_string(),
            min_password_strength: params.min_password_strength,
            cached_image,
            error: None,
            is_submitting: false,
        }
    }

    /// Check if the form has any changes compared to original values
    pub fn has_changes(&self, original: &ServerInfoParams<'_>) -> bool {
        let name_changed = self.name != original.name.unwrap_or("");
        let desc_changed = self.description != original.description.unwrap_or("");
        let max_conn_changed = self.max_connections_per_ip != original.max_connections_per_ip;
        let max_xfer_changed = self.max_transfers_per_ip != original.max_transfers_per_ip;
        let image_changed = self.image != original.image;
        let reindex_changed = self.file_reindex_interval != original.file_reindex_interval;
        let persistent_changed =
            self.persistent_channels != original.persistent_channels.unwrap_or("");
        let auto_join_changed =
            self.auto_join_channels != original.auto_join_channels.unwrap_or("");
        let min_password_strength_changed =
            self.min_password_strength != original.min_password_strength;
        name_changed
            || desc_changed
            || max_conn_changed
            || max_xfer_changed
            || image_changed
            || reindex_changed
            || persistent_changed
            || auto_join_changed
            || min_password_strength_changed
    }
}
