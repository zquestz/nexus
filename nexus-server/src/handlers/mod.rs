//! Message handlers for client commands

mod broadcast;
mod chat;
mod chattopicupdate;
pub mod errors;
mod handshake;
mod login;
mod usercreate;
mod userdelete;
mod useredit;
mod userinfo;
mod userkick;
mod userlist;
mod usermessage;
mod userupdate;

#[cfg(test)]
pub mod testing;

pub use broadcast::handle_user_broadcast;
pub use chat::handle_chat_send;
pub use chattopicupdate::handle_chattopicupdate;
pub use errors::*;
pub use handshake::handle_handshake;
pub use login::{LoginRequest, handle_login};
pub use usercreate::handle_usercreate;
pub use userdelete::handle_userdelete;
pub use useredit::handle_useredit;
pub use userinfo::handle_userinfo;
pub use userkick::handle_userkick;
pub use userlist::handle_userlist;
pub use usermessage::handle_usermessage;
pub use userupdate::{UserUpdateRequest, handle_userupdate};

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
