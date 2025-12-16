//! Message handlers for client commands

mod broadcast;
mod chat;
mod chat_topic_update;
pub mod errors;
mod handshake;
mod login;
mod news_create;
mod news_delete;
mod news_edit;
mod news_list;
mod news_show;
mod news_update;
mod server_info_update;
mod user_create;
mod user_delete;
mod user_edit;
mod user_info;
mod user_kick;
mod user_list;
mod user_message;
mod user_update;

#[cfg(test)]
pub mod testing;

pub use broadcast::handle_user_broadcast;
pub use chat::handle_chat_send;
pub use chat_topic_update::handle_chat_topic_update;
pub use errors::*;
pub use handshake::handle_handshake;
pub use login::{LoginRequest, handle_login};
pub use news_create::handle_news_create;
pub use news_delete::handle_news_delete;
pub use news_edit::handle_news_edit;
pub use news_list::handle_news_list;
pub use news_show::handle_news_show;
pub use news_update::handle_news_update;
pub use server_info_update::handle_server_info_update;
pub use user_create::handle_user_create;
pub use user_delete::handle_user_delete;
pub use user_edit::handle_user_edit;
pub use user_info::handle_user_info;
pub use user_kick::handle_user_kick;
pub use user_list::handle_user_list;
pub use user_message::handle_user_message;
pub use user_update::{UserUpdateRequest, handle_user_update};

use std::io;
use std::net::SocketAddr;

use tokio::io::AsyncWrite;
use tokio::sync::mpsc;

use nexus_common::framing::{FrameWriter, MessageId};
use nexus_common::io::send_server_message_with_id;
use nexus_common::protocol::ServerMessage;

use crate::db::Database;
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
}

impl<'a, W: AsyncWrite + Unpin> HandlerContext<'a, W> {
    /// Send a message to the client, echoing the request's message ID
    pub async fn send_message(&mut self, message: &ServerMessage) -> io::Result<()> {
        send_server_message_with_id(self.writer, message, self.message_id).await
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
