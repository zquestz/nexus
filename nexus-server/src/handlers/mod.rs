//! Message handlers for client commands

mod broadcast;
mod chat;
mod errors;
mod handshake;
mod login;
mod usercreate;
mod userdelete;
mod userinfo;
mod userlist;

#[cfg(test)]
pub mod testing;

pub use broadcast::handle_user_broadcast;
pub use chat::handle_chat_send;
pub use errors::*;
pub use handshake::handle_handshake;
pub use login::handle_login;
pub use usercreate::handle_usercreate;
pub use userdelete::handle_userdelete;
pub use userinfo::handle_userinfo;
pub use userlist::handle_userlist;

use crate::db::UserDb;
use crate::users::UserManager;
use nexus_common::io::send_server_message;
use nexus_common::protocol::ServerMessage;
use std::io;
use std::net::SocketAddr;
use tokio::net::tcp::OwnedWriteHalf;
use tokio::sync::mpsc;

/// Context passed to all handlers with shared resources
pub struct HandlerContext<'a> {
    pub writer: &'a mut OwnedWriteHalf,
    pub peer_addr: SocketAddr,
    pub user_manager: &'a UserManager,
    pub user_db: &'a UserDb,
    pub tx: &'a mpsc::UnboundedSender<ServerMessage>,
    pub debug: bool,
}

impl<'a> HandlerContext<'a> {
    /// Send a message to the client
    pub async fn send_message(&mut self, message: &ServerMessage) -> io::Result<()> {
        send_server_message(self.writer, message).await
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
        Err(io::Error::new(io::ErrorKind::Other, message))
    }
}

/// Get current Unix timestamp in seconds
pub fn current_timestamp() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs()
}
