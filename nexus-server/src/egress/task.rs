use std::num::NonZeroUsize;

use nexus_common::framing::MessageId;
use nexus_common::protocol::ServerMessage;
use tokio::sync::{mpsc, oneshot};

use crate::egress::{
    DispatchOutcome, EgressDispatchTx, EgressManager, EgressRegistration, StagingError,
    stage_server_message,
};
use crate::scheduler::{ConnectionClass, ConnectionId};

pub const DEFAULT_EGRESS_COMMAND_QUEUE_CAPACITY: usize = 1024;

pub type EgressCommandTx = mpsc::Sender<EgressCommand>;
pub type EgressCommandRx = mpsc::Receiver<EgressCommand>;
pub type StageMessageResult = Result<usize, StagingError>;

pub enum EgressCommand {
    Register {
        registration: EgressRegistration,
        reply_tx: oneshot::Sender<bool>,
    },
    RegisterAnon {
        connection_id: ConnectionId,
        class: ConnectionClass,
        dispatch_tx: EgressDispatchTx,
        reply_tx: oneshot::Sender<bool>,
    },
    Unregister {
        connection_id: ConnectionId,
    },
    StageMessage {
        connection_id: ConnectionId,
        message: Box<ServerMessage>,
        message_id: MessageId,
        reply_tx: oneshot::Sender<StageMessageResult>,
    },
    Ack {
        connection_id: ConnectionId,
    },
    WriteFailed {
        connection_id: ConnectionId,
    },
    SetBlocked {
        connection_id: ConnectionId,
        blocked: bool,
    },
    TransitionToUser {
        connection_id: ConnectionId,
        user_id: i64,
        weight: u16,
    },
    UpdateUserWeight {
        user_id: i64,
        weight: u16,
    },
}

#[derive(Clone)]
pub struct EgressHandle {
    tx: EgressCommandTx,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EgressTaskError {
    Closed,
    ReplyDropped,
}

impl EgressHandle {
    pub fn new(tx: EgressCommandTx) -> Self {
        Self { tx }
    }

    pub fn sender(&self) -> &EgressCommandTx {
        &self.tx
    }

    pub async fn register(
        &self,
        registration: EgressRegistration,
    ) -> Result<bool, EgressTaskError> {
        self.request(|reply_tx| EgressCommand::Register {
            registration,
            reply_tx,
        })
        .await
    }

    pub async fn register_anon(
        &self,
        connection_id: ConnectionId,
        class: ConnectionClass,
        dispatch_tx: EgressDispatchTx,
    ) -> Result<bool, EgressTaskError> {
        self.request(|reply_tx| EgressCommand::RegisterAnon {
            connection_id,
            class,
            dispatch_tx,
            reply_tx,
        })
        .await
    }

    pub async fn unregister(&self, connection_id: ConnectionId) -> Result<(), EgressTaskError> {
        self.send(EgressCommand::Unregister { connection_id }).await
    }

    pub async fn stage_message(
        &self,
        connection_id: ConnectionId,
        message: Box<ServerMessage>,
        message_id: MessageId,
    ) -> Result<StageMessageResult, EgressTaskError> {
        self.request(|reply_tx| EgressCommand::StageMessage {
            connection_id,
            message,
            message_id,
            reply_tx,
        })
        .await
    }

    pub async fn ack(&self, connection_id: ConnectionId) -> Result<(), EgressTaskError> {
        self.send(EgressCommand::Ack { connection_id }).await
    }

    pub async fn write_failed(&self, connection_id: ConnectionId) -> Result<(), EgressTaskError> {
        self.send(EgressCommand::WriteFailed { connection_id })
            .await
    }

    pub async fn set_blocked(
        &self,
        connection_id: ConnectionId,
        blocked: bool,
    ) -> Result<(), EgressTaskError> {
        self.send(EgressCommand::SetBlocked {
            connection_id,
            blocked,
        })
        .await
    }

    pub async fn transition_to_user(
        &self,
        connection_id: ConnectionId,
        user_id: i64,
        weight: u16,
    ) -> Result<(), EgressTaskError> {
        self.send(EgressCommand::TransitionToUser {
            connection_id,
            user_id,
            weight,
        })
        .await
    }

    pub async fn update_user_weight(
        &self,
        user_id: i64,
        weight: u16,
    ) -> Result<(), EgressTaskError> {
        self.send(EgressCommand::UpdateUserWeight { user_id, weight })
            .await
    }

    async fn request<T>(
        &self,
        build: impl FnOnce(oneshot::Sender<T>) -> EgressCommand,
    ) -> Result<T, EgressTaskError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.send(build(reply_tx)).await?;
        reply_rx.await.map_err(|_| EgressTaskError::ReplyDropped)
    }

    async fn send(&self, command: EgressCommand) -> Result<(), EgressTaskError> {
        self.tx
            .send(command)
            .await
            .map_err(|_| EgressTaskError::Closed)
    }
}

pub struct EgressTask {
    manager: EgressManager,
    rx: EgressCommandRx,
}

impl EgressTask {
    pub fn new(manager: EgressManager, rx: EgressCommandRx) -> Self {
        Self { manager, rx }
    }

    pub fn channel(manager: EgressManager) -> (EgressHandle, Self) {
        let (tx, rx) = mpsc::channel(DEFAULT_EGRESS_COMMAND_QUEUE_CAPACITY);
        (EgressHandle::new(tx), Self::new(manager, rx))
    }

    pub fn channel_with_capacity(
        manager: EgressManager,
        command_capacity: NonZeroUsize,
    ) -> (EgressHandle, Self) {
        let (tx, rx) = mpsc::channel(command_capacity.get());
        (EgressHandle::new(tx), Self::new(manager, rx))
    }

    pub async fn run(mut self) {
        while let Some(command) = self.rx.recv().await {
            self.handle_command(command);
            self.pump_dispatch();
        }
    }

    fn handle_command(&mut self, command: EgressCommand) {
        match command {
            EgressCommand::Register {
                registration,
                reply_tx,
            } => {
                let _ = reply_tx.send(self.manager.register(registration));
            }
            EgressCommand::RegisterAnon {
                connection_id,
                class,
                dispatch_tx,
                reply_tx,
            } => {
                let _ = reply_tx.send(self.manager.register_anon(
                    connection_id,
                    class,
                    dispatch_tx,
                ));
            }
            EgressCommand::Unregister { connection_id } => {
                self.manager.unregister(connection_id);
            }
            EgressCommand::StageMessage {
                connection_id,
                message,
                message_id,
                reply_tx,
            } => {
                let result =
                    stage_server_message(&mut self.manager, connection_id, &message, message_id);
                let _ = reply_tx.send(result);
            }
            EgressCommand::Ack { connection_id } => {
                self.manager.ack(connection_id);
            }
            EgressCommand::WriteFailed { connection_id } => {
                self.manager.write_failed(connection_id);
            }
            EgressCommand::SetBlocked {
                connection_id,
                blocked,
            } => {
                self.manager.set_blocked(connection_id, blocked);
            }
            EgressCommand::TransitionToUser {
                connection_id,
                user_id,
                weight,
            } => {
                self.manager
                    .transition_to_user(connection_id, user_id, weight);
            }
            EgressCommand::UpdateUserWeight { user_id, weight } => {
                self.manager.update_user_weight(user_id, weight);
            }
        }
    }

    fn pump_dispatch(&mut self) {
        while let DispatchOutcome::Dispatched { .. } | DispatchOutcome::Unregistered { .. } =
            self.manager.dispatch_next()
        {}
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use nexus_common::framing::MessageId;
    use nexus_common::io::server_message_to_frame_bytes;
    use nexus_common::protocol::{ChatAction, ServerMessage};
    use nexus_common::validators::DEFAULT_CHANNEL;
    use tokio::task::JoinHandle;
    use tokio::time;

    use super::*;
    use crate::egress::{EgressDispatchRx, EgressEnqueueError};
    use crate::scheduler::FlowId;

    fn conn(id: u64) -> ConnectionId {
        ConnectionId::new(id)
    }

    fn message_id(bytes: &[u8]) -> MessageId {
        MessageId::from_bytes(bytes).unwrap()
    }

    fn nonzero(value: usize) -> NonZeroUsize {
        NonZeroUsize::new(value).unwrap()
    }

    fn dispatch_channel() -> (EgressDispatchTx, EgressDispatchRx) {
        mpsc::channel(8)
    }

    fn user_registration(
        connection_id: ConnectionId,
        user_id: i64,
        dispatch_tx: EgressDispatchTx,
    ) -> EgressRegistration {
        EgressRegistration {
            connection_id,
            flow_id: FlowId::User(user_id),
            class: ConnectionClass::Protocol,
            weight: 1,
            dispatch_tx,
        }
    }

    fn spawn_task(manager: EgressManager) -> (EgressHandle, JoinHandle<()>) {
        let (handle, task) = EgressTask::channel_with_capacity(manager, nonzero(16));
        let task = tokio::spawn(task.run());
        (handle, task)
    }

    async fn stop_task(handle: EgressHandle, task: JoinHandle<()>) {
        drop(handle);
        task.await.unwrap();
    }

    async fn recv_dispatch(rx: &mut EgressDispatchRx) -> crate::egress::EgressDispatch {
        time::timeout(Duration::from_secs(1), rx.recv())
            .await
            .unwrap()
            .unwrap()
    }

    fn large_chat_message() -> ServerMessage {
        ServerMessage::ChatMessage {
            session_id: 42,
            nickname: "alice".to_string(),
            is_admin: true,
            is_shared: false,
            message: "x".repeat(16 * 1024),
            action: ChatAction::Normal,
            channel: DEFAULT_CHANNEL.to_string(),
            timestamp: 1234,
        }
    }

    #[tokio::test]
    async fn register_replies_and_duplicate_register_returns_false() {
        let (handle, task) = spawn_task(EgressManager::new(nonzero(512)));
        let connection = conn(1);
        let (first_tx, _first_rx) = dispatch_channel();
        let (second_tx, _second_rx) = dispatch_channel();

        assert!(
            handle
                .register_anon(connection, ConnectionClass::Protocol, first_tx)
                .await
                .unwrap()
        );
        assert!(
            !handle
                .register_anon(connection, ConnectionClass::Protocol, second_tx)
                .await
                .unwrap()
        );

        stop_task(handle, task).await;
    }

    #[tokio::test]
    async fn stage_message_dispatches_wire_identical_chunks() {
        let (handle, task) = spawn_task(EgressManager::new(nonzero(512)));
        let connection = conn(1);
        let (dispatch_tx, mut dispatch_rx) = dispatch_channel();
        assert!(
            handle
                .register_anon(connection, ConnectionClass::Protocol, dispatch_tx)
                .await
                .unwrap()
        );
        let message = large_chat_message();
        let message_id = message_id(b"000000000201");
        let expected = server_message_to_frame_bytes(&message, message_id).unwrap();

        let chunks = handle
            .stage_message(connection, Box::new(message), message_id)
            .await
            .unwrap()
            .unwrap();
        assert!(chunks > 1);

        let mut reassembled = Vec::new();
        for _ in 0..chunks {
            let dispatch = recv_dispatch(&mut dispatch_rx).await;
            assert_eq!(dispatch.connection_id, connection);
            reassembled.extend_from_slice(dispatch.chunk.as_bytes());
            handle.ack(connection).await.unwrap();
        }

        assert_eq!(reassembled.as_slice(), expected.as_ref());
        stop_task(handle, task).await;
    }

    #[tokio::test]
    async fn task_dispatches_multiple_connections_fairly() {
        let (handle, task) = spawn_task(EgressManager::new(nonzero(512)));
        let first = conn(1);
        let second = conn(2);
        let (first_tx, mut first_rx) = dispatch_channel();
        let (second_tx, mut second_rx) = dispatch_channel();
        assert!(
            handle
                .register(user_registration(first, 1, first_tx))
                .await
                .unwrap()
        );
        assert!(
            handle
                .register(user_registration(second, 2, second_tx))
                .await
                .unwrap()
        );

        let first_chunks = handle
            .stage_message(
                first,
                Box::new(large_chat_message()),
                message_id(b"000000000207"),
            )
            .await
            .unwrap()
            .unwrap();
        let second_chunks = handle
            .stage_message(
                second,
                Box::new(large_chat_message()),
                message_id(b"000000000208"),
            )
            .await
            .unwrap()
            .unwrap();
        assert_eq!(first_chunks, second_chunks);

        let mut first_seen = 1;
        let mut second_seen = 1;
        assert_eq!(recv_dispatch(&mut first_rx).await.connection_id, first);
        assert_eq!(recv_dispatch(&mut second_rx).await.connection_id, second);
        assert!(first_rx.try_recv().is_err());
        assert!(second_rx.try_recv().is_err());

        while first_seen < first_chunks || second_seen < second_chunks {
            if first_seen < first_chunks {
                handle.ack(first).await.unwrap();
                assert_eq!(recv_dispatch(&mut first_rx).await.connection_id, first);
                first_seen += 1;
            }

            if second_seen < second_chunks {
                handle.ack(second).await.unwrap();
                assert_eq!(recv_dispatch(&mut second_rx).await.connection_id, second);
                second_seen += 1;
            }
        }

        handle.ack(first).await.unwrap();
        handle.ack(second).await.unwrap();
        assert_eq!(first_seen, second_seen);
        stop_task(handle, task).await;
    }

    #[tokio::test]
    async fn transition_to_user_command_preserves_staged_message() {
        let (handle, task) = spawn_task(EgressManager::new(nonzero(512)));
        let connection = conn(1);
        let (dispatch_tx, mut dispatch_rx) = dispatch_channel();
        assert!(
            handle
                .register_anon(connection, ConnectionClass::Protocol, dispatch_tx)
                .await
                .unwrap()
        );
        handle.set_blocked(connection, true).await.unwrap();
        let message = large_chat_message();
        let message_id = message_id(b"000000000209");
        let expected = server_message_to_frame_bytes(&message, message_id).unwrap();
        let chunks = handle
            .stage_message(connection, Box::new(message), message_id)
            .await
            .unwrap()
            .unwrap();
        assert!(chunks > 1);
        assert!(dispatch_rx.try_recv().is_err());

        handle.transition_to_user(connection, 42, 5).await.unwrap();
        handle.set_blocked(connection, false).await.unwrap();

        let mut reassembled = Vec::new();
        for _ in 0..chunks {
            let dispatch = recv_dispatch(&mut dispatch_rx).await;
            assert_eq!(dispatch.connection_id, connection);
            reassembled.extend_from_slice(dispatch.chunk.as_bytes());
            handle.ack(connection).await.unwrap();
        }

        assert_eq!(reassembled.as_slice(), expected.as_ref());
        stop_task(handle, task).await;
    }

    #[tokio::test]
    async fn stage_message_preserves_queue_full() {
        let (handle, task) = spawn_task(EgressManager::with_frame_limit(nonzero(512), 1));
        let connection = conn(1);
        let (dispatch_tx, _dispatch_rx) = dispatch_channel();
        assert!(
            handle
                .register_anon(connection, ConnectionClass::Protocol, dispatch_tx)
                .await
                .unwrap()
        );
        assert!(matches!(
            handle
                .stage_message(
                    connection,
                    Box::new(ServerMessage::Pong),
                    message_id(b"000000000202"),
                )
                .await
                .unwrap(),
            Ok(1)
        ));

        let result = handle
            .stage_message(
                connection,
                Box::new(ServerMessage::Pong),
                message_id(b"000000000203"),
            )
            .await
            .unwrap();

        assert!(matches!(
            result,
            Err(StagingError::Enqueue(EgressEnqueueError::QueueFull))
        ));
        stop_task(handle, task).await;
    }

    #[tokio::test]
    async fn blocked_connection_dispatches_after_unblock_command() {
        let (handle, task) = spawn_task(EgressManager::new(nonzero(512)));
        let connection = conn(1);
        let (dispatch_tx, mut dispatch_rx) = dispatch_channel();
        assert!(
            handle
                .register_anon(connection, ConnectionClass::Protocol, dispatch_tx)
                .await
                .unwrap()
        );
        handle.set_blocked(connection, true).await.unwrap();
        assert!(matches!(
            handle
                .stage_message(
                    connection,
                    Box::new(ServerMessage::Pong),
                    message_id(b"000000000204"),
                )
                .await
                .unwrap(),
            Ok(1)
        ));
        assert!(dispatch_rx.try_recv().is_err());

        handle.set_blocked(connection, false).await.unwrap();
        let dispatch = recv_dispatch(&mut dispatch_rx).await;
        assert_eq!(dispatch.connection_id, connection);
        handle.ack(connection).await.unwrap();

        stop_task(handle, task).await;
    }

    #[tokio::test]
    async fn write_failed_unregisters_before_later_stage_command() {
        let (handle, task) = spawn_task(EgressManager::new(nonzero(512)));
        let connection = conn(1);
        let (dispatch_tx, mut dispatch_rx) = dispatch_channel();
        assert!(
            handle
                .register_anon(connection, ConnectionClass::Protocol, dispatch_tx)
                .await
                .unwrap()
        );
        assert!(matches!(
            handle
                .stage_message(
                    connection,
                    Box::new(large_chat_message()),
                    message_id(b"000000000205"),
                )
                .await
                .unwrap(),
            Ok(chunks) if chunks > 1
        ));
        let dispatch = recv_dispatch(&mut dispatch_rx).await;
        assert_eq!(dispatch.connection_id, connection);

        handle.write_failed(connection).await.unwrap();
        let result = handle
            .stage_message(
                connection,
                Box::new(ServerMessage::Pong),
                message_id(b"000000000206"),
            )
            .await
            .unwrap();

        assert!(matches!(
            result,
            Err(StagingError::Enqueue(EgressEnqueueError::UnknownConnection))
        ));
        stop_task(handle, task).await;
    }
}
