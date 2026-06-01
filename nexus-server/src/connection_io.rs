use std::io;

use tokio::io::AsyncWrite;
use tokio::time;

use nexus_common::framing::{FrameWriter, MessageId};
use nexus_common::io::send_server_message_with_id;
use nexus_common::protocol::ServerMessage;

use crate::constants::{BBS_WRITE_TIMEOUT, ERR_BBS_WRITE_TIMEOUT};

pub(crate) async fn send_server_message_with_write_timeout<W>(
    writer: &mut FrameWriter<W>,
    message: &ServerMessage,
    message_id: MessageId,
) -> io::Result<()>
where
    W: AsyncWrite + Unpin,
{
    match time::timeout(
        BBS_WRITE_TIMEOUT,
        send_server_message_with_id(writer, message, message_id),
    )
    .await
    {
        Ok(result) => result,
        Err(_) => Err(io::Error::new(
            io::ErrorKind::TimedOut,
            ERR_BBS_WRITE_TIMEOUT,
        )),
    }
}
