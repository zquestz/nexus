use std::num::NonZeroUsize;

use nexus_common::framing::MessageId;
use nexus_common::protocol::ServerMessage;
use tokio::sync::{mpsc, oneshot};
use tokio::time::{self, Duration, Instant};

use crate::egress::{
    DispatchOutcome, EgressDispatchTx, EgressEnqueueError, EgressManager, EgressRegistration,
    StagingError, stage_server_message,
};
use crate::scheduler::{ConnectionClass, ConnectionId};

pub const DEFAULT_EGRESS_COMMAND_QUEUE_CAPACITY: usize = 1024;
const EGRESS_RATE_BURST_CHUNKS: usize = 4;
const NANOS_PER_SECOND: u128 = 1_000_000_000;

pub type EgressCommandTx = mpsc::Sender<EgressCommand>;
pub type EgressCommandRx = mpsc::Receiver<EgressCommand>;
pub type StageMessageResult = Result<usize, StageMessageError>;

#[derive(Debug)]
pub enum StageMessageError {
    QueueFull {
        message: Box<ServerMessage>,
        message_id: MessageId,
    },
    Failed(StagingError),
}

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
    SetMaxOutboundRate {
        bytes_per_second: u64,
    },
    SetChunkSize {
        chunk_size: NonZeroUsize,
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

    pub async fn set_max_outbound_rate(
        &self,
        bytes_per_second: u64,
    ) -> Result<(), EgressTaskError> {
        self.send(EgressCommand::SetMaxOutboundRate { bytes_per_second })
            .await
    }

    pub async fn set_chunk_size(&self, chunk_size: NonZeroUsize) -> Result<(), EgressTaskError> {
        self.send(EgressCommand::SetChunkSize { chunk_size }).await
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
    rate_limiter: EgressRateLimiter,
}

impl EgressTask {
    pub fn new(manager: EgressManager, rx: EgressCommandRx) -> Self {
        Self::with_rate(manager, rx, 0)
    }

    pub fn with_rate(manager: EgressManager, rx: EgressCommandRx, bytes_per_second: u64) -> Self {
        let rate_limiter = EgressRateLimiter::new(bytes_per_second, manager.chunk_size());
        Self {
            manager,
            rx,
            rate_limiter,
        }
    }

    pub fn channel(manager: EgressManager) -> (EgressHandle, Self) {
        let (tx, rx) = mpsc::channel(DEFAULT_EGRESS_COMMAND_QUEUE_CAPACITY);
        (EgressHandle::new(tx), Self::new(manager, rx))
    }

    pub fn channel_with_rate(
        manager: EgressManager,
        bytes_per_second: u64,
    ) -> (EgressHandle, Self) {
        let (tx, rx) = mpsc::channel(DEFAULT_EGRESS_COMMAND_QUEUE_CAPACITY);
        (
            EgressHandle::new(tx),
            Self::with_rate(manager, rx, bytes_per_second),
        )
    }

    pub fn channel_with_capacity(
        manager: EgressManager,
        command_capacity: NonZeroUsize,
    ) -> (EgressHandle, Self) {
        let (tx, rx) = mpsc::channel(command_capacity.get());
        (EgressHandle::new(tx), Self::new(manager, rx))
    }

    pub fn channel_with_capacity_and_rate(
        manager: EgressManager,
        command_capacity: NonZeroUsize,
        bytes_per_second: u64,
    ) -> (EgressHandle, Self) {
        let (tx, rx) = mpsc::channel(command_capacity.get());
        (
            EgressHandle::new(tx),
            Self::with_rate(manager, rx, bytes_per_second),
        )
    }

    pub async fn run(mut self) {
        let mut dispatch_deferred = false;

        loop {
            if dispatch_deferred {
                let delay = self
                    .rate_limiter
                    .delay_until_ready(Instant::now())
                    .unwrap_or(Duration::ZERO);

                tokio::select! {
                    command = self.rx.recv() => {
                        let Some(command) = command else {
                            break;
                        };
                        self.handle_command(command);
                    }
                    () = time::sleep(delay) => {}
                }
            } else {
                let Some(command) = self.rx.recv().await else {
                    break;
                };
                self.handle_command(command);
            }

            dispatch_deferred = self.pump_dispatch().is_rate_limited();
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
                let result = match stage_server_message(
                    &mut self.manager,
                    connection_id,
                    &message,
                    message_id,
                ) {
                    Ok(chunks) => Ok(chunks),
                    Err(StagingError::Enqueue(EgressEnqueueError::QueueFull)) => {
                        Err(StageMessageError::QueueFull {
                            message,
                            message_id,
                        })
                    }
                    Err(err) => Err(StageMessageError::Failed(err)),
                };
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
            EgressCommand::SetMaxOutboundRate { bytes_per_second } => {
                self.rate_limiter
                    .set_rate(bytes_per_second, self.manager.chunk_size());
            }
            EgressCommand::SetChunkSize { chunk_size } => {
                self.manager.set_chunk_size(chunk_size);
                self.rate_limiter.set_chunk_size(chunk_size);
            }
        }
    }

    fn pump_dispatch(&mut self) -> PumpResult {
        loop {
            if !self.manager.has_dispatchable_packet() {
                return PumpResult::Empty;
            }

            if !self.rate_limiter.can_dispatch(Instant::now()) {
                return PumpResult::RateLimited;
            }

            match self.manager.dispatch_next() {
                DispatchOutcome::Dispatched { bytes, .. } => {
                    self.rate_limiter.record_dispatch(bytes);
                }
                DispatchOutcome::Unregistered { .. } => {}
                DispatchOutcome::Empty => return PumpResult::Empty,
            }
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PumpResult {
    Empty,
    RateLimited,
}

impl PumpResult {
    fn is_rate_limited(self) -> bool {
        matches!(self, Self::RateLimited)
    }
}

#[derive(Clone, Debug)]
struct EgressRateLimiter {
    bytes_per_second: u64,
    burst_capacity: i128,
    tokens: i128,
    refill_remainder: u128,
    last_refill: Instant,
}

impl EgressRateLimiter {
    fn new(bytes_per_second: u64, chunk_size: NonZeroUsize) -> Self {
        let burst_capacity = burst_capacity(chunk_size);
        Self {
            bytes_per_second,
            burst_capacity,
            tokens: burst_capacity,
            refill_remainder: 0,
            last_refill: Instant::now(),
        }
    }

    fn set_rate(&mut self, bytes_per_second: u64, chunk_size: NonZeroUsize) {
        let now = Instant::now();
        self.refill(now);
        self.bytes_per_second = bytes_per_second;
        self.burst_capacity = burst_capacity(chunk_size);
        self.tokens = if bytes_per_second == 0 {
            self.burst_capacity
        } else {
            self.tokens.min(self.burst_capacity)
        };
        self.refill_remainder = 0;
        self.last_refill = now;
    }

    fn set_chunk_size(&mut self, chunk_size: NonZeroUsize) {
        let now = Instant::now();
        self.refill(now);
        self.burst_capacity = burst_capacity(chunk_size);
        self.tokens = self.tokens.min(self.burst_capacity);
    }

    fn can_dispatch(&mut self, now: Instant) -> bool {
        if self.is_unlimited() {
            return true;
        }

        self.refill(now);
        self.tokens > 0
    }

    fn record_dispatch(&mut self, bytes: usize) {
        if self.is_unlimited() {
            return;
        }

        let bytes = i128::try_from(bytes).unwrap_or(i128::MAX);
        self.tokens = self.tokens.saturating_sub(bytes);
    }

    fn delay_until_ready(&mut self, now: Instant) -> Option<Duration> {
        if self.is_unlimited() {
            return None;
        }

        self.refill(now);
        if self.tokens > 0 {
            return Some(Duration::ZERO);
        }

        let needed = u128::try_from(1_i128.saturating_sub(self.tokens)).unwrap_or(u128::MAX);
        let nanos = needed
            .saturating_mul(NANOS_PER_SECOND)
            .div_ceil(u128::from(self.bytes_per_second));
        let nanos = u64::try_from(nanos).unwrap_or(u64::MAX);
        Some(Duration::from_nanos(nanos))
    }

    fn refill(&mut self, now: Instant) {
        if self.is_unlimited() {
            self.tokens = self.burst_capacity;
            self.last_refill = now;
            self.refill_remainder = 0;
            return;
        }

        let elapsed = now.saturating_duration_since(self.last_refill);
        self.last_refill = now;
        let scaled = elapsed
            .as_nanos()
            .saturating_mul(u128::from(self.bytes_per_second))
            .saturating_add(self.refill_remainder);
        let add = scaled / NANOS_PER_SECOND;
        self.refill_remainder = scaled % NANOS_PER_SECOND;
        let add = i128::try_from(add).unwrap_or(i128::MAX);
        self.tokens = self.tokens.saturating_add(add).min(self.burst_capacity);
    }

    fn is_unlimited(&self) -> bool {
        self.bytes_per_second == 0
    }
}

fn burst_capacity(chunk_size: NonZeroUsize) -> i128 {
    let bytes = chunk_size.get().saturating_mul(EGRESS_RATE_BURST_CHUNKS);
    i128::try_from(bytes.max(chunk_size.get())).unwrap_or(i128::MAX)
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

    fn spawn_task_with_rate(
        manager: EgressManager,
        bytes_per_second: u64,
    ) -> (EgressHandle, JoinHandle<()>) {
        let (handle, task) =
            EgressTask::channel_with_capacity_and_rate(manager, nonzero(16), bytes_per_second);
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

    async fn recv_either_dispatch(
        first_rx: &mut EgressDispatchRx,
        second_rx: &mut EgressDispatchRx,
    ) -> crate::egress::EgressDispatch {
        time::timeout(Duration::from_secs(1), async {
            tokio::select! {
                dispatch = first_rx.recv() => dispatch,
                dispatch = second_rx.recv() => dispatch,
            }
        })
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

    #[test]
    fn token_bucket_refill_is_clamped_to_burst_capacity() {
        let mut limiter = EgressRateLimiter::new(100, nonzero(100));
        limiter.tokens = -10_000;

        limiter.refill(limiter.last_refill + Duration::from_secs(1_000));

        assert_eq!(limiter.tokens, burst_capacity(nonzero(100)));
    }

    #[test]
    fn token_bucket_refill_tracks_configured_rate_over_time() {
        let mut limiter = EgressRateLimiter::new(3, nonzero(100));
        limiter.tokens = 0;
        limiter.refill_remainder = 0;
        let mut now = limiter.last_refill;

        for _ in 0..10 {
            now += Duration::from_millis(100);
            limiter.refill(now);
        }

        assert_eq!(limiter.tokens, 3);
        assert_eq!(limiter.refill_remainder, 0);

        limiter.record_dispatch(100);

        assert_eq!(limiter.tokens, -97);
        assert_eq!(
            limiter.delay_until_ready(now),
            Some(Duration::from_nanos(32_666_666_667))
        );
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
    async fn rate_limiter_defers_dispatch_after_burst_is_spent() {
        let (handle, task) = spawn_task_with_rate(EgressManager::new(nonzero(100)), 10);
        let connection = conn(1);
        let (dispatch_tx, mut dispatch_rx) = dispatch_channel();
        assert!(
            handle
                .register_anon(connection, ConnectionClass::Protocol, dispatch_tx)
                .await
                .unwrap()
        );
        let chunks = handle
            .stage_message(
                connection,
                Box::new(large_chat_message()),
                message_id(b"000000000210"),
            )
            .await
            .unwrap()
            .unwrap();
        assert!(chunks > EGRESS_RATE_BURST_CHUNKS);

        for _ in 0..EGRESS_RATE_BURST_CHUNKS {
            let dispatch = recv_dispatch(&mut dispatch_rx).await;
            assert_eq!(dispatch.chunk.len(), 100);
            handle.ack(connection).await.unwrap();
        }

        assert!(
            time::timeout(Duration::from_millis(50), dispatch_rx.recv())
                .await
                .is_err(),
            "next chunk should wait for tokens after the burst is spent"
        );
        let dispatch = time::timeout(Duration::from_secs(1), dispatch_rx.recv())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(dispatch.connection_id, connection);
        handle.ack(connection).await.unwrap();

        stop_task(handle, task).await;
    }

    #[tokio::test]
    async fn live_rate_update_to_unlimited_resumes_deferred_dispatch() {
        let (handle, task) = spawn_task_with_rate(EgressManager::new(nonzero(100)), 10);
        let connection = conn(1);
        let (dispatch_tx, mut dispatch_rx) = dispatch_channel();
        assert!(
            handle
                .register_anon(connection, ConnectionClass::Protocol, dispatch_tx)
                .await
                .unwrap()
        );
        let chunks = handle
            .stage_message(
                connection,
                Box::new(large_chat_message()),
                message_id(b"000000000211"),
            )
            .await
            .unwrap()
            .unwrap();
        assert!(chunks > EGRESS_RATE_BURST_CHUNKS);

        for _ in 0..EGRESS_RATE_BURST_CHUNKS {
            let dispatch = recv_dispatch(&mut dispatch_rx).await;
            assert_eq!(dispatch.chunk.len(), 100);
            handle.ack(connection).await.unwrap();
        }

        assert!(
            time::timeout(Duration::from_millis(50), dispatch_rx.recv())
                .await
                .is_err()
        );
        handle.set_max_outbound_rate(0).await.unwrap();
        let dispatch = time::timeout(Duration::from_millis(100), dispatch_rx.recv())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(dispatch.connection_id, connection);
        handle.ack(connection).await.unwrap();

        stop_task(handle, task).await;
    }

    #[tokio::test]
    async fn live_chunk_size_update_affects_future_staged_messages() {
        let (handle, task) = spawn_task(EgressManager::new(nonzero(100)));
        let connection = conn(1);
        let (dispatch_tx, mut dispatch_rx) = dispatch_channel();
        assert!(
            handle
                .register_anon(connection, ConnectionClass::Protocol, dispatch_tx)
                .await
                .unwrap()
        );

        handle.set_chunk_size(nonzero(50)).await.unwrap();
        let chunks = handle
            .stage_message(
                connection,
                Box::new(large_chat_message()),
                message_id(b"000000000212"),
            )
            .await
            .unwrap()
            .unwrap();
        assert!(chunks > 1);

        let dispatch = recv_dispatch(&mut dispatch_rx).await;
        assert_eq!(dispatch.connection_id, connection);
        assert_eq!(dispatch.chunk.len(), 50);
        handle.ack(connection).await.unwrap();

        stop_task(handle, task).await;
    }

    #[tokio::test]
    async fn finite_rate_limiter_preserves_weighted_fairness() {
        let (handle, task) = spawn_task_with_rate(EgressManager::new(nonzero(100)), 20_000);
        let heavy = conn(1);
        let light = conn(2);
        let (heavy_tx, mut heavy_rx) = dispatch_channel();
        let (light_tx, mut light_rx) = dispatch_channel();

        assert!(
            handle
                .register(user_registration(heavy, 1, heavy_tx))
                .await
                .unwrap()
        );
        assert!(
            handle
                .register(user_registration(light, 2, light_tx))
                .await
                .unwrap()
        );
        handle.update_user_weight(1, 3).await.unwrap();
        handle.update_user_weight(2, 1).await.unwrap();

        let heavy_chunks = handle
            .stage_message(
                heavy,
                Box::new(large_chat_message()),
                message_id(b"000000000213"),
            )
            .await
            .unwrap()
            .unwrap();
        let light_chunks = handle
            .stage_message(
                light,
                Box::new(large_chat_message()),
                message_id(b"000000000214"),
            )
            .await
            .unwrap()
            .unwrap();
        assert!(heavy_chunks >= 32);
        assert!(light_chunks >= 32);

        let mut heavy_seen = 0;
        let mut light_seen = 0;
        for _ in 0..32 {
            let dispatch = recv_either_dispatch(&mut heavy_rx, &mut light_rx).await;
            assert_eq!(dispatch.chunk.len(), 100);
            if dispatch.connection_id == heavy {
                heavy_seen += 1;
            } else {
                assert_eq!(dispatch.connection_id, light);
                light_seen += 1;
            }
            handle.ack(dispatch.connection_id).await.unwrap();
        }

        assert!(
            (22..=26).contains(&heavy_seen),
            "expected heavy flow near 3:1 share, got {heavy_seen}:{light_seen}"
        );
        assert!(
            (6..=10).contains(&light_seen),
            "expected light flow near 3:1 share, got {heavy_seen}:{light_seen}"
        );

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
                    Box::new(ServerMessage::Pong),
                    message_id(b"000000000202"),
                )
                .await
                .unwrap(),
            Ok(1)
        ));
        let full_message_id = message_id(b"000000000203");

        let result = handle
            .stage_message(connection, Box::new(ServerMessage::Pong), full_message_id)
            .await
            .unwrap();

        match result {
            Err(StageMessageError::QueueFull {
                message,
                message_id,
            }) => {
                assert!(matches!(*message, ServerMessage::Pong));
                assert_eq!(message_id, full_message_id);
            }
            other => panic!("expected returned QueueFull message, got {other:?}"),
        }

        let dispatch = recv_dispatch(&mut dispatch_rx).await;
        assert_eq!(dispatch.connection_id, connection);
        handle.ack(connection).await.unwrap();
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
            Err(StageMessageError::Failed(StagingError::Enqueue(
                EgressEnqueueError::UnknownConnection
            )))
        ));
        stop_task(handle, task).await;
    }
}
