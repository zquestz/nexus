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
mod duration;
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
mod handshake;
mod login;
mod news_create;
mod news_delete;
mod news_edit;
mod news_list;
mod news_show;
mod news_update;
mod server_info_update;
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
pub use handshake::handle_handshake;
pub use login::{LoginRequest, handle_login};
pub use news_create::handle_news_create;
pub use news_delete::handle_news_delete;
pub use news_edit::handle_news_edit;
pub use news_list::handle_news_list;
pub use news_show::handle_news_show;
pub use news_update::handle_news_update;
pub use server_info_update::{ServerInfoUpdateRequest, handle_server_info_update};
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

use std::io;
use std::net::SocketAddr;
use std::path::Path;
use std::sync::{Arc, RwLock};

use crate::constants::ERR_CHANNEL_CLOSED;

use tokio::io::AsyncWrite;
use tokio::sync::mpsc;

use nexus_common::framing::{FrameWriter, MessageId};
use nexus_common::io::send_server_message_with_id;
use nexus_common::protocol::ServerMessage;

use crate::channels::ChannelManager;
use crate::connection_tracker::ConnectionTracker;
use crate::db::Database;
use crate::files::FileIndex;
use crate::ip_rule_cache::IpRuleCache;
use crate::users::UserManager;

/// Context passed to all handlers with shared resources
pub struct HandlerContext<'a, W> {
    pub writer: &'a mut FrameWriter<W>,
    pub peer_addr: SocketAddr,
    pub user_manager: &'a UserManager,
    pub db: &'a Database,
    pub tx: &'a mpsc::UnboundedSender<(ServerMessage, Option<MessageId>)>,
    pub debug: bool,
    pub locale: &'a str,
    /// Message ID from the incoming request (for response correlation)
    pub message_id: MessageId,
    /// File area root path (None if file area is not configured)
    pub file_root: Option<&'static Path>,
    /// Port for file transfers (typically 7501)
    pub transfer_port: u16,
    /// Connection tracker for both main and transfer connections
    pub connection_tracker: Arc<ConnectionTracker>,
    /// In-memory IP rule cache for fast lookups and cache updates (bans and trusts)
    pub ip_rule_cache: Arc<RwLock<IpRuleCache>>,
    /// File index for searching files
    pub file_index: Arc<FileIndex>,
    /// Channel manager for multi-channel chat
    pub channel_manager: &'a ChannelManager,
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

/// Get current Unix timestamp in seconds
pub fn current_timestamp() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("System time should be after UNIX_EPOCH")
        .as_secs() as i64
}
