//! Server info display and edit state

use crate::image::{CachedImage, decode_data_uri_max_width};
use crate::style::SERVER_IMAGE_MAX_CACHE_WIDTH;
use nexus_common::validators::PasswordStrength;

/// Conversion factor between Mbps (megabits per second) and bytes/sec.
/// 1 Mbps = 1_000_000 bits/sec ÷ 8 bits/byte = 125_000 bytes/sec.
const BYTES_PER_SEC_PER_MBPS: f64 = 125_000.0;

// =============================================================================
// Server Info Display Tab
// =============================================================================

/// Tab selection for the Server Info panel.
///
/// Two-tab layout:
/// - Config: alphabetical configuration rows, visibility per-row by permission.
/// - Trackers: tracker administration (list / add / edit / remove / accept).
///   Gated on `tracker_list`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub enum ServerInfoTab {
    /// Config tab: alphabetical configuration rows.
    #[default]
    Config,
    /// Trackers tab: tracker admin (gated on `tracker_list`).
    Trackers,
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
    pub max_outbound_rate: Option<u64>,
    pub max_transfers_per_ip: Option<u32>,
    pub min_password_strength: PasswordStrength,
    pub name: Option<&'a str>,
    pub persistent_channels: Option<&'a str>,
    pub public_address: Option<&'a str>,
    pub scheduler_chunk_size: Option<u32>,
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
            max_outbound_rate: None,
            max_transfers_per_ip: None,
            min_password_strength: PasswordStrength::Good,
            name: None,
            persistent_channels: None,
            public_address: None,
            scheduler_chunk_size: None,
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
    /// Max outbound bandwidth cap in Mbps (NumberInput<f64>, stepper of 1.0,
    /// fractional values typed directly). Converted to bytes/sec on save.
    /// `0.0` means unlimited.
    pub max_outbound_rate_mbps: f64,
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
    /// WF2Q+ scheduler chunk size in bytes (editable, NumberInput).
    pub scheduler_chunk_size: Option<u32>,
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
            .field("max_outbound_rate_mbps", &self.max_outbound_rate_mbps)
            .field("max_transfers_per_ip", &self.max_transfers_per_ip)
            .field("min_password_strength", &self.min_password_strength)
            .field("name", &self.name)
            .field("persistent_channels", &self.persistent_channels)
            .field("public_address", &self.public_address)
            .field("scheduler_chunk_size", &self.scheduler_chunk_size)
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
            // NumberInput accepts any finite non-negative f64; 0.0 means
            // unlimited.
            max_outbound_rate_mbps: params.max_outbound_rate.unwrap_or(0) as f64
                / BYTES_PER_SEC_PER_MBPS,
            max_transfers_per_ip: params.max_transfers_per_ip,
            min_password_strength: params.min_password_strength,
            name: params.name.unwrap_or("").to_string(),
            persistent_channels: params.persistent_channels.unwrap_or("").to_string(),
            public_address: params.public_address.unwrap_or("").to_string(),
            scheduler_chunk_size: params.scheduler_chunk_size,
        }
    }

    /// Convert the Mbps form value to bytes/sec for diff comparison and
    /// protocol submission. NumberInput already constrains the value to a
    /// finite non-negative f64, so this can't fail.
    pub fn max_outbound_rate_bytes_per_sec(&self) -> u64 {
        (self.max_outbound_rate_mbps * BYTES_PER_SEC_PER_MBPS).round() as u64
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
        let max_outbound_rate_changed =
            self.max_outbound_rate_bytes_per_sec() != original.max_outbound_rate.unwrap_or(0);
        let scheduler_chunk_size_changed =
            self.scheduler_chunk_size != original.scheduler_chunk_size;
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
            || max_outbound_rate_changed
            || scheduler_chunk_size_changed
    }
}

/// Format a bytes/sec rate as Mbps (decimal, no trailing zeros). Used by the
/// edit form to populate its initial text-input from the stored u64 value,
/// and by the display view to render the Bandwidth section.
///
/// Precision is `{:.6}` so sub-millibit caps render accurately rather than
/// rounding to a misleading "0". Trailing zeros are trimmed, so normal-range
/// values (1 Mbps, 100 Mbps, 1000 Mbps) display identically to a tighter
/// precision.
pub fn format_bytes_per_sec_as_mbps(bytes_per_sec: u64) -> String {
    let mbps = bytes_per_sec as f64 / BYTES_PER_SEC_PER_MBPS;
    let formatted = format!("{:.6}", mbps);
    let trimmed = formatted.trim_end_matches('0').trim_end_matches('.');
    trimmed.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Normal-range caps render with no trailing zeros — the precision
    /// bump from {:.4} to {:.6} must NOT add visual noise to common values.
    #[test]
    fn format_normal_range_caps_no_trailing_zeros() {
        assert_eq!(format_bytes_per_sec_as_mbps(125_000_000), "1000"); // 1 Gbps
        assert_eq!(format_bytes_per_sec_as_mbps(12_500_000), "100"); // 100 Mbps
        assert_eq!(format_bytes_per_sec_as_mbps(125_000), "1"); // 1 Mbps
        assert_eq!(format_bytes_per_sec_as_mbps(62_500), "0.5"); // 0.5 Mbps
        assert_eq!(format_bytes_per_sec_as_mbps(12_500), "0.1"); // 0.1 Mbps
    }

    /// Sub-resolution caps (below ~12 bytes/sec) used to round to "0"
    /// under {:.4}, which the display view rendered as a literal "0"
    /// next to the cap label — silently lying about the actual cap. The
    /// {:.6} precision lets these render accurately.
    #[test]
    fn format_sub_resolution_caps_render_accurately() {
        assert_eq!(format_bytes_per_sec_as_mbps(125), "0.001"); // 0.001 Mbps
        assert_eq!(format_bytes_per_sec_as_mbps(12), "0.000096"); // 0.000096 Mbps
        assert_eq!(format_bytes_per_sec_as_mbps(6), "0.000048"); // would have been "0"
        assert_eq!(format_bytes_per_sec_as_mbps(1), "0.000008"); // would have been "0"
    }

    /// The display view branches on `bytes_per_sec == 0` BEFORE calling
    /// this function (the "Unlimited" label). This formatter is only
    /// asked to render non-zero values in practice; verify it renders
    /// "0" for 0 anyway (defensive: any future caller that bypasses the
    /// branch sees a sensible result).
    #[test]
    fn format_zero_returns_bare_zero() {
        assert_eq!(format_bytes_per_sec_as_mbps(0), "0");
    }
}
