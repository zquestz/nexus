//! Shared ServerInfo builder for consistent field visibility across handlers.
//!
//! This helper is used by `login.rs`, `user_update.rs`, and `broadcasts.rs` to ensure
//! permission-based field filtering is consistent. All three call sites use this
//! instead of constructing `ServerInfo` directly.

use nexus_common::protocol::ServerInfo;

use crate::logging::server_log_level;

/// Raw server info values before permission filtering.
///
/// Callers populate this from DB queries or broadcast params, then pass it
/// to `build_server_info()` which applies permission-based field visibility.
pub struct ServerInfoValues {
    pub name: String,
    pub description: String,
    pub version: String,
    pub image: String,
    pub max_connections_per_ip: u32,
    pub max_transfers_per_ip: u32,
    pub transfer_port: u16,
    pub transfer_websocket_port: Option<u16>,
    pub file_reindex_interval: u32,
    pub persistent_channels: String,
    pub auto_join_channels: String,
    pub min_password_strength: u8,
    pub chat_burst_limit: u32,
    pub chat_rate_limit: u32,
    pub fingerprint: String,
}

/// Options controlling what's included in the built ServerInfo.
pub struct ServerInfoOptions {
    /// Whether the user is an admin
    pub is_admin: bool,
    /// Whether the user has the FileReindex permission
    pub has_file_reindex: bool,
    /// Whether the user has chat join permission (for auto-join channels visibility)
    pub has_chat_join: bool,
    /// Whether to include the server image (false for PermissionsUpdated)
    pub include_image: bool,
}

/// Build a `ServerInfo` with permission-based field visibility.
///
/// Fields visible to all users: name, description, version, fingerprint,
/// max_connections_per_ip, max_transfers_per_ip, transfer_port,
/// transfer_websocket_port, min_password_strength, log_level,
/// chat_burst_limit, chat_rate_limit.
///
/// Permission-gated fields:
/// - `file_reindex_interval`: admin or FileReindex permission
/// - `persistent_channels`: admin only
/// - `auto_join_channels`: admin or ChatJoin permission
pub fn build_server_info(values: &ServerInfoValues, options: &ServerInfoOptions) -> ServerInfo {
    let file_reindex_interval = if options.is_admin || options.has_file_reindex {
        Some(values.file_reindex_interval)
    } else {
        None
    };

    let persistent_channels = if options.is_admin {
        Some(values.persistent_channels.clone())
    } else {
        None
    };

    let auto_join_channels = if options.is_admin || options.has_chat_join {
        Some(values.auto_join_channels.clone())
    } else {
        None
    };

    let image = if options.include_image {
        Some(values.image.clone())
    } else {
        None
    };

    ServerInfo {
        name: Some(values.name.clone()),
        description: Some(values.description.clone()),
        version: Some(values.version.clone()),
        fingerprint: Some(values.fingerprint.clone()),
        max_connections_per_ip: Some(values.max_connections_per_ip),
        max_transfers_per_ip: Some(values.max_transfers_per_ip),
        image,
        transfer_port: values.transfer_port,
        transfer_websocket_port: values.transfer_websocket_port,
        file_reindex_interval,
        persistent_channels,
        auto_join_channels,
        chat_burst_limit: Some(values.chat_burst_limit),
        chat_rate_limit: Some(values.chat_rate_limit),
        min_password_strength: Some(values.min_password_strength),
        log_level: Some(server_log_level().to_string()),
    }
}
