//! Shared ServerInfo builder for consistent field visibility across handlers.
//!
//! This helper is used by `login.rs`, `user_update.rs`, and `broadcasts.rs` to ensure
//! permission-based field filtering is consistent. All three call sites use this
//! instead of constructing `ServerInfo` directly.

use nexus_common::logging::current_log_level;
use nexus_common::protocol::ServerInfo;

/// Raw server info values before permission filtering.
///
/// Callers populate this from DB queries or broadcast params, then pass it
/// to `build_server_info()` which applies permission-based field visibility.
pub struct ServerInfoValues {
    pub name: String,
    pub description: String,
    pub public_address: String,
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
    pub max_outbound_rate: u64,
    pub scheduler_chunk_size: u32,
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
/// Fields visible to all users: name, description, public_address, version,
/// max_connections_per_ip, max_transfers_per_ip, transfer_port,
/// transfer_websocket_port, min_password_strength, log_level,
/// chat_burst_limit, chat_rate_limit.
///
/// Permission-gated fields:
/// - `file_reindex_interval`: admin or FileReindex permission
/// - `persistent_channels`: admin only
/// - `auto_join_channels`: admin or ChatJoin permission
/// - `scheduler_chunk_size`: admin only (internal tuning knob)
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

    // Empty public_address is treated as unset — send None to keep the wire clean.
    let public_address = if values.public_address.is_empty() {
        None
    } else {
        Some(values.public_address.clone())
    };

    // scheduler_chunk_size is a pure internal tuning knob — admin-only.
    let scheduler_chunk_size = if options.is_admin {
        Some(values.scheduler_chunk_size)
    } else {
        None
    };

    ServerInfo {
        name: Some(values.name.clone()),
        description: Some(values.description.clone()),
        public_address,
        version: Some(values.version.clone()),
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
        log_level: Some(current_log_level().to_string()),
        max_outbound_rate: Some(values.max_outbound_rate),
        scheduler_chunk_size,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_values() -> ServerInfoValues {
        ServerInfoValues {
            name: String::new(),
            description: String::new(),
            public_address: String::new(),
            version: String::new(),
            image: String::new(),
            max_connections_per_ip: 0,
            max_transfers_per_ip: 0,
            transfer_port: 7501,
            transfer_websocket_port: None,
            file_reindex_interval: 0,
            persistent_channels: String::new(),
            auto_join_channels: String::new(),
            min_password_strength: 0,
            chat_burst_limit: 0,
            chat_rate_limit: 0,
            max_outbound_rate: 0,
            scheduler_chunk_size: 8192,
        }
    }

    /// `scheduler_chunk_size` is an internal tuning knob and must never
    /// reach non-admin clients. Pinned to catch a future refactor that
    /// drops the gate.
    #[test]
    fn scheduler_chunk_size_hidden_from_non_admin() {
        let info = build_server_info(
            &sample_values(),
            &ServerInfoOptions {
                is_admin: false,
                has_file_reindex: false,
                has_chat_join: false,
                include_image: false,
            },
        );
        assert!(info.scheduler_chunk_size.is_none());
    }

    #[test]
    fn scheduler_chunk_size_visible_to_admin() {
        let info = build_server_info(
            &sample_values(),
            &ServerInfoOptions {
                is_admin: true,
                has_file_reindex: false,
                has_chat_join: false,
                include_image: false,
            },
        );
        assert_eq!(info.scheduler_chunk_size, Some(8192));
    }
}
