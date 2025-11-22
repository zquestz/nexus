//! I/O utilities for sending and receiving protocol messages

use crate::protocol::{ClientMessage, ServerMessage};
use serde::Serialize;
use std::io;
use tokio::io::AsyncWriteExt;

/// Send a message over a TCP writer with newline-delimited JSON encoding
///
/// This is the shared implementation used by both client and server to send
/// protocol messages. It serializes the message to JSON, writes it to the
/// stream, appends a newline, and flushes.
pub async fn send_message<W, M>(writer: &mut W, message: &M) -> io::Result<()>
where
    W: AsyncWriteExt + Unpin,
    M: Serialize,
{
    let json = serde_json::to_string(message)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    writer.write_all(json.as_bytes()).await?;
    writer.write_all(b"\n").await?;
    writer.flush().await?;
    Ok(())
}

/// Send a ClientMessage to the server
#[inline]
pub async fn send_client_message<W>(writer: &mut W, message: &ClientMessage) -> io::Result<()>
where
    W: AsyncWriteExt + Unpin,
{
    send_message(writer, message).await
}

/// Send a ServerMessage to a client
#[inline]
pub async fn send_server_message<W>(writer: &mut W, message: &ServerMessage) -> io::Result<()>
where
    W: AsyncWriteExt + Unpin,
{
    send_message(writer, message).await
}
