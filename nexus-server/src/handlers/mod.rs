//! Message handlers for client commands

mod ban_create;
mod ban_delete;
mod ban_list;
mod broadcast;
mod chat;
mod chat_join;
mod chat_leave;
mod chat_list;
mod chat_secret;
mod chat_topic_update;
mod connection_monitor;
pub(crate) mod duration;
pub mod errors;
mod file_copy;
mod file_create_dir;
mod file_delete;
mod file_info;
mod file_list;
mod file_move;
mod file_reindex;
mod file_rename;
mod file_search;
mod group_create;
mod group_delete;
mod group_edit;
mod group_list;
mod group_update;
mod handshake;
mod login;
mod news_create;
mod news_delete;
mod news_edit;
mod news_list;
mod news_show;
mod news_update;
mod server_info;
mod server_info_update;
mod tracker_create;
mod tracker_delete;
mod tracker_edit;
mod tracker_list;
mod tracker_update;
mod trust_create;
mod trust_delete;
mod trust_list;
mod user_away;
mod user_back;
mod user_create;
mod user_delete;
mod user_edit;
mod user_info;
mod user_kick;
mod user_list;
mod user_message;
mod user_status;
mod user_update;
mod voice_join;
mod voice_leave;

#[cfg(test)]
pub mod testing;

pub use ban_create::handle_ban_create;
pub use ban_delete::handle_ban_delete;
pub use ban_list::handle_ban_list;
pub use broadcast::handle_user_broadcast;
pub use chat::handle_chat_send;
pub use chat_join::handle_chat_join;
pub use chat_leave::handle_chat_leave;
pub use chat_list::handle_chat_list;
pub use chat_secret::handle_chat_secret;
pub use chat_topic_update::handle_chat_topic_update;
pub use connection_monitor::handle_connection_monitor;
pub use errors::*;
pub use file_copy::handle_file_copy;
pub use file_create_dir::handle_file_create_dir;
pub use file_delete::handle_file_delete;
pub use file_info::handle_file_info;
pub use file_list::handle_file_list;
pub use file_move::handle_file_move;
pub use file_reindex::handle_file_reindex;
pub use file_rename::handle_file_rename;
pub use file_search::handle_file_search;
pub use group_create::handle_group_create;
pub use group_delete::handle_group_delete;
pub use group_edit::handle_group_edit;
pub use group_list::handle_group_list;
pub use group_update::handle_group_update;
pub use handshake::handle_handshake;
pub use login::{LoginRequest, handle_login};
pub use news_create::handle_news_create;
pub use news_delete::handle_news_delete;
pub use news_edit::handle_news_edit;
pub use news_list::handle_news_list;
pub use news_show::handle_news_show;
pub use news_update::handle_news_update;
pub use server_info::{ServerInfoOptions, ServerInfoValues, build_server_info};
pub use server_info_update::{ServerInfoUpdateRequest, handle_server_info_update};
pub use tracker_create::{TrackerCreateRequest, handle_tracker_create};
pub use tracker_delete::handle_tracker_delete;
pub use tracker_edit::handle_tracker_edit;
pub use tracker_list::handle_tracker_list;
pub use tracker_update::{TrackerUpdateRequest, handle_tracker_update};
pub use trust_create::handle_trust_create;
pub use trust_delete::handle_trust_delete;
pub use trust_list::handle_trust_list;
pub use user_away::handle_user_away;
pub use user_back::handle_user_back;
pub use user_create::{UserCreateRequest, handle_user_create};
pub use user_delete::handle_user_delete;
pub use user_edit::handle_user_edit;
pub use user_info::handle_user_info;
pub use user_kick::handle_user_kick;
pub use user_list::handle_user_list;
pub use user_message::handle_user_message;
pub use user_status::handle_user_status;
pub use user_update::{UserUpdateRequest, handle_user_update};
pub use voice_join::handle_voice_join;
pub use voice_leave::handle_voice_leave;

use std::io;
use std::net::SocketAddr;
use std::path::Path;
use std::sync::{Arc, RwLock};

use crate::constants::{ERR_CHANNEL_CLOSED, ERR_SYSTEM_TIME_AFTER_EPOCH};

use tokio::io::AsyncWrite;
use tokio::sync::mpsc;

use nexus_common::framing::{FrameWriter, MessageId};
use nexus_common::io::send_server_message_with_id;
use nexus_common::protocol::ServerMessage;

use std::net::IpAddr;

use ipnet::IpNet;

use crate::channels::ChannelManager;
use crate::connection_tracker::ConnectionTracker;
use crate::db::Database;
use crate::files::FileIndex;
use crate::flood::FloodConfig;
use crate::ip_rule_cache::IpRuleCache;
use crate::tracker::TrackerManager;
use crate::transfers::TransferRegistry;
use crate::users::UserManager;
use crate::users::user::UserSession;
use crate::voice::{VoiceRegistry, send_voice_leave_notifications};

/// Context passed to all handlers with shared resources
pub struct HandlerContext<'a, W> {
    pub writer: &'a mut FrameWriter<W>,
    pub peer_addr: SocketAddr,
    pub user_manager: &'a UserManager,
    pub db: &'a Database,
    pub tx: &'a mpsc::UnboundedSender<(ServerMessage, Option<MessageId>)>,
    pub locale: &'a str,
    /// Message ID from the incoming request (for response correlation)
    pub message_id: MessageId,
    /// File area root path (None if file area is not configured)
    pub file_root: Option<&'static Path>,
    /// Port for file transfers (typically 7501)
    pub transfer_port: u16,
    /// Port for WebSocket file transfers (typically 7503), None if WebSocket disabled
    pub transfer_websocket_port: Option<u16>,
    /// Connection tracker for both main and transfer connections
    pub connection_tracker: Arc<ConnectionTracker>,
    /// In-memory IP rule cache for fast lookups and cache updates (bans and trusts)
    pub ip_rule_cache: Arc<RwLock<IpRuleCache>>,
    /// File index for searching files
    pub file_index: Arc<FileIndex>,
    /// Channel manager for multi-channel chat
    pub channel_manager: &'a ChannelManager,
    /// Transfer registry for disconnecting active transfers on ban
    pub transfer_registry: Arc<TransferRegistry>,
    /// Voice registry for managing active voice sessions
    pub voice_registry: &'a VoiceRegistry,
    /// Tracker publisher manager (per-tracker publisher tasks)
    pub tracker_manager: &'a TrackerManager,
    /// Server certificate fingerprint (SHA-256, colon-separated)
    pub fingerprint: &'static str,
    /// Shared flood protection config (burst and rate limits)
    pub flood_config: Arc<FloodConfig>,
}

impl<'a, W: AsyncWrite + Unpin> HandlerContext<'a, W> {
    /// Send a message to the client, echoing the request's message ID
    pub async fn send_message(&mut self, message: &ServerMessage) -> io::Result<()> {
        send_server_message_with_id(self.writer, message, self.message_id).await
    }

    /// Send a message via the channel instead of directly to the socket.
    ///
    /// This ensures the message is queued after any broadcast messages sent through
    /// the same channel, maintaining proper ordering. Used when a response must
    /// appear after broadcast messages in the client's receive order.
    pub fn send_message_via_channel(&self, message: &ServerMessage) -> io::Result<()> {
        self.tx
            .send((message.clone(), Some(self.message_id)))
            .map_err(|_| io::Error::other(ERR_CHANNEL_CLOSED))
    }

    /// Send an error message without disconnecting
    pub async fn send_error(&mut self, message: &str, command: Option<&str>) -> io::Result<()> {
        let error_msg = ServerMessage::Error {
            message: message.to_string(),
            command: command.map(|s| s.to_string()),
        };
        self.send_message(&error_msg).await
    }

    /// Send an error message and disconnect
    pub async fn send_error_and_disconnect(
        &mut self,
        message: &str,
        command: Option<&str>,
    ) -> io::Result<()> {
        self.send_error(message, command).await?;
        Err(io::Error::other(message))
    }
}

/// Compose a `TrackerInfo` wire struct from a DB row + optional
/// runtime status.
///
/// When `status` is `None` (disabled tracker, or no task running yet),
/// runtime fields default to "not connected" — `connected: false` and
/// every other runtime field `None`. This accurately reflects the
/// truth: there's no live publisher task, so there's no connection
/// state to report.
#[must_use]
pub fn compose_tracker_info(
    record: crate::db::TrackerRecord,
    status: Option<crate::tracker::TrackerStatus>,
) -> nexus_common::protocol::TrackerInfo {
    let s = status.unwrap_or_default();
    nexus_common::protocol::TrackerInfo {
        id: record.id,
        address: record.address,
        port: record.port,
        fingerprint: record.fingerprint,
        password: record.password,
        name: record.name,
        enabled: record.enabled,
        created_at: record.created_at,
        updated_at: record.updated_at,
        connected: s.connected,
        last_connected_at: s.last_connected_at,
        last_error: s.last_error,
        last_error_kind: s.last_error_kind,
        pending_fingerprint: s.pending_fingerprint,
        refresh_interval: s.refresh_interval,
    }
}

/// Get current Unix timestamp in seconds
pub fn current_timestamp() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect(ERR_SYSTEM_TIME_AFTER_EPOCH)
        .as_secs() as i64
}

/// Remove a user from voice (if in voice) and then from UserManager, broadcasting appropriate messages.
///
/// This helper ensures voice cleanup happens before user removal, which is needed when
/// disconnecting users via handlers (kick, delete, disable) that don't go through the
/// normal connection cleanup path in `connection.rs`.
///
/// Returns the removed UserSession if the user was found.
pub async fn remove_user_with_voice_cleanup(
    user_manager: &UserManager,
    voice_registry: &VoiceRegistry,
    channel_manager: &ChannelManager,
    session_id: u32,
    user: &UserSession,
) -> Option<UserSession> {
    // Remove from voice session and notify remaining participants
    if let Some(info) = voice_registry.remove_by_session_id(session_id).await {
        // Notify the leaving user and broadcast to remaining participants
        send_voice_leave_notifications(&info, Some(&user.tx), user_manager, channel_manager).await;
    }

    // Now remove from UserManager and broadcast UserDisconnected
    user_manager.remove_user_and_broadcast(session_id).await
}

/// Clean up voice sessions for all users matching a specific IP address.
///
/// This should be called before disconnecting users via ban to ensure they are
/// properly removed from voice and other participants are notified.
///
/// The `skip_ip` predicate can skip certain IPs (e.g., trusted IPs that won't be banned).
pub async fn cleanup_voice_for_ip<S>(
    user_manager: &UserManager,
    voice_registry: &VoiceRegistry,
    channel_manager: &ChannelManager,
    ip: &str,
    skip_ip: S,
) where
    S: Fn(&IpAddr) -> bool,
{
    // Check if this IP should be skipped (e.g., trusted)
    if let Ok(parsed_ip) = ip.parse::<IpAddr>()
        && skip_ip(&parsed_ip)
    {
        return;
    }

    // Get all sessions from this IP
    let sessions: Vec<UserSession> = user_manager
        .get_all_users()
        .await
        .into_iter()
        .filter(|u| u.address.ip().to_string() == ip)
        .collect();

    // Clean up voice for each session
    for user in sessions {
        cleanup_voice_for_session(user_manager, voice_registry, channel_manager, &user).await;
    }
}

/// Clean up voice sessions for all users whose IP falls within a CIDR range.
///
/// This should be called before disconnecting users via ban to ensure they are
/// properly removed from voice and other participants are notified.
///
/// The `skip_ip` predicate can skip certain IPs (e.g., trusted IPs that won't be banned).
pub async fn cleanup_voice_for_range<S>(
    user_manager: &UserManager,
    voice_registry: &VoiceRegistry,
    channel_manager: &ChannelManager,
    range: &IpNet,
    skip_ip: S,
) where
    S: Fn(&IpAddr) -> bool,
{
    // Get all sessions in the range (excluding skipped IPs)
    let sessions: Vec<UserSession> = user_manager
        .get_all_users()
        .await
        .into_iter()
        .filter(|u| {
            let ip = u.address.ip();
            range.contains(&ip) && !skip_ip(&ip)
        })
        .collect();

    // Clean up voice for each session
    for user in sessions {
        cleanup_voice_for_session(user_manager, voice_registry, channel_manager, &user).await;
    }
}

/// Clean up voice for a single user session (helper for the above functions).
async fn cleanup_voice_for_session(
    user_manager: &UserManager,
    voice_registry: &VoiceRegistry,
    channel_manager: &ChannelManager,
    user: &UserSession,
) {
    if let Some(info) = voice_registry.remove_by_session_id(user.session_id).await {
        // Notify the leaving user and broadcast to remaining participants
        send_voice_leave_notifications(&info, Some(&user.tx), user_manager, channel_manager).await;
    }
}
