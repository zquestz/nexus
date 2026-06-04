use std::{num::NonZeroUsize, sync::Arc};

use nexus_common::framing::MessageId;
use nexus_common::protocol::ServerMessage;
use tokio::sync::{mpsc, oneshot};
use tokio::time::{self, Duration, Instant};

use crate::egress::{
    DispatchOutcome, EgressDispatchTx, EgressEnqueueError, EgressManager, EgressMessagePriority,
    EgressRegistration, StagingError, stage_server_message_with_priority,
};
use crate::scheduler::{ConnectionClass, ConnectionId};

pub const DEFAULT_EGRESS_COMMAND_QUEUE_CAPACITY: usize = 1024;
const EGRESS_SETTINGS_BATCH_LIMIT: usize = 128;
const EGRESS_RATE_BURST_CHUNKS: usize = 4;
const NANOS_PER_SECOND: u128 = 1_000_000_000;

pub type EgressCommandTx = mpsc::Sender<EgressCommand>;
pub type EgressCommandRx = mpsc::Receiver<EgressCommand>;
pub type EgressSettingsCommandTx = mpsc::UnboundedSender<EgressSettingsCommand>;
pub type EgressSettingsCommandRx = mpsc::UnboundedReceiver<EgressSettingsCommand>;
pub type StageMessageResult = Result<usize, StageMessageError>;
pub type StageFrameResult = Result<usize, EgressEnqueueError>;

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
        priority: EgressMessagePriority,
        reply_tx: oneshot::Sender<StageMessageResult>,
    },
    StageFrame {
        connection_id: ConnectionId,
        frame: Arc<[u8]>,
        reply_tx: oneshot::Sender<StageFrameResult>,
    },
    StagePriorityFrame {
        connection_id: ConnectionId,
        frame: Arc<[u8]>,
        reply_tx: oneshot::Sender<StageFrameResult>,
    },
    BeginStreamFrame {
        connection_id: ConnectionId,
        header: Arc<[u8]>,
        reply_tx: oneshot::Sender<StageFrameResult>,
    },
    StageStreamChunk {
        connection_id: ConnectionId,
        chunk: Arc<[u8]>,
        reply_tx: oneshot::Sender<StageFrameResult>,
    },
    FinishStreamFrame {
        connection_id: ConnectionId,
        reply_tx: oneshot::Sender<StageFrameResult>,
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
}

pub enum EgressSettingsCommand {
    // This may live on the settings queue because registration is still a
    // request/reply command on the bounded queue and is awaited before login.
    // Do not make registration fire-and-forget without reworking this ordering.
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
    settings_tx: EgressSettingsCommandTx,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EgressTaskError {
    Closed,
    ReplyDropped,
}

impl EgressHandle {
    pub fn new(tx: EgressCommandTx, settings_tx: EgressSettingsCommandTx) -> Self {
        Self { tx, settings_tx }
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
        self.stage_message_with_priority(
            connection_id,
            message,
            message_id,
            EgressMessagePriority::Normal,
        )
        .await
    }

    pub async fn stage_priority_message(
        &self,
        connection_id: ConnectionId,
        message: Box<ServerMessage>,
        message_id: MessageId,
    ) -> Result<StageMessageResult, EgressTaskError> {
        self.stage_message_with_priority(
            connection_id,
            message,
            message_id,
            EgressMessagePriority::Priority,
        )
        .await
    }

    pub async fn stage_priority_frame(
        &self,
        connection_id: ConnectionId,
        frame: Arc<[u8]>,
    ) -> Result<StageFrameResult, EgressTaskError> {
        self.stage_frame_with_priority(connection_id, frame, EgressMessagePriority::Priority)
            .await
    }

    pub async fn stage_frame(
        &self,
        connection_id: ConnectionId,
        frame: Arc<[u8]>,
    ) -> Result<StageFrameResult, EgressTaskError> {
        self.stage_frame_with_priority(connection_id, frame, EgressMessagePriority::Normal)
            .await
    }

    async fn stage_frame_with_priority(
        &self,
        connection_id: ConnectionId,
        frame: Arc<[u8]>,
        priority: EgressMessagePriority,
    ) -> Result<StageFrameResult, EgressTaskError> {
        match priority {
            EgressMessagePriority::Normal => {
                self.request(|reply_tx| EgressCommand::StageFrame {
                    connection_id,
                    frame,
                    reply_tx,
                })
                .await
            }
            EgressMessagePriority::Priority => {
                self.request(|reply_tx| EgressCommand::StagePriorityFrame {
                    connection_id,
                    frame,
                    reply_tx,
                })
                .await
            }
        }
    }

    pub async fn begin_stream_frame(
        &self,
        connection_id: ConnectionId,
        header: Arc<[u8]>,
    ) -> Result<StageFrameResult, EgressTaskError> {
        self.request(|reply_tx| EgressCommand::BeginStreamFrame {
            connection_id,
            header,
            reply_tx,
        })
        .await
    }

    pub async fn stage_stream_chunk(
        &self,
        connection_id: ConnectionId,
        chunk: Arc<[u8]>,
    ) -> Result<StageFrameResult, EgressTaskError> {
        self.request(|reply_tx| EgressCommand::StageStreamChunk {
            connection_id,
            chunk,
            reply_tx,
        })
        .await
    }

    pub async fn finish_stream_frame(
        &self,
        connection_id: ConnectionId,
    ) -> Result<StageFrameResult, EgressTaskError> {
        self.request(|reply_tx| EgressCommand::FinishStreamFrame {
            connection_id,
            reply_tx,
        })
        .await
    }

    async fn stage_message_with_priority(
        &self,
        connection_id: ConnectionId,
        message: Box<ServerMessage>,
        message_id: MessageId,
        priority: EgressMessagePriority,
    ) -> Result<StageMessageResult, EgressTaskError> {
        self.request(|reply_tx| EgressCommand::StageMessage {
            connection_id,
            message,
            message_id,
            priority,
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

    pub fn transition_to_user(
        &self,
        connection_id: ConnectionId,
        user_id: i64,
        weight: u16,
    ) -> Result<(), EgressTaskError> {
        self.send_settings(EgressSettingsCommand::TransitionToUser {
            connection_id,
            user_id,
            weight,
        })
    }

    pub fn update_user_weight(&self, user_id: i64, weight: u16) -> Result<(), EgressTaskError> {
        self.send_settings(EgressSettingsCommand::UpdateUserWeight { user_id, weight })
    }

    pub fn set_max_outbound_rate(&self, bytes_per_second: u64) -> Result<(), EgressTaskError> {
        self.send_settings(EgressSettingsCommand::SetMaxOutboundRate { bytes_per_second })
    }

    pub fn set_chunk_size(&self, chunk_size: NonZeroUsize) -> Result<(), EgressTaskError> {
        self.send_settings(EgressSettingsCommand::SetChunkSize { chunk_size })
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

    fn send_settings(&self, command: EgressSettingsCommand) -> Result<(), EgressTaskError> {
        self.settings_tx
            .send(command)
            .map_err(|_| EgressTaskError::Closed)
    }
}

pub struct EgressTask {
    manager: EgressManager,
    rx: EgressCommandRx,
    settings_rx: EgressSettingsCommandRx,
    rate_limiter: EgressRateLimiter,
}

impl EgressTask {
    pub fn new(
        manager: EgressManager,
        rx: EgressCommandRx,
        settings_rx: EgressSettingsCommandRx,
    ) -> Self {
        Self::with_rate(manager, rx, settings_rx, 0)
    }

    pub fn with_rate(
        manager: EgressManager,
        rx: EgressCommandRx,
        settings_rx: EgressSettingsCommandRx,
        bytes_per_second: u64,
    ) -> Self {
        let rate_limiter = EgressRateLimiter::new(bytes_per_second, manager.chunk_size());
        Self {
            manager,
            rx,
            settings_rx,
            rate_limiter,
        }
    }

    pub fn channel(manager: EgressManager) -> (EgressHandle, Self) {
        let (tx, rx) = mpsc::channel(DEFAULT_EGRESS_COMMAND_QUEUE_CAPACITY);
        let (settings_tx, settings_rx) = mpsc::unbounded_channel();
        (
            EgressHandle::new(tx, settings_tx),
            Self::new(manager, rx, settings_rx),
        )
    }

    pub fn channel_with_rate(
        manager: EgressManager,
        bytes_per_second: u64,
    ) -> (EgressHandle, Self) {
        let (tx, rx) = mpsc::channel(DEFAULT_EGRESS_COMMAND_QUEUE_CAPACITY);
        let (settings_tx, settings_rx) = mpsc::unbounded_channel();
        (
            EgressHandle::new(tx, settings_tx),
            Self::with_rate(manager, rx, settings_rx, bytes_per_second),
        )
    }

    pub fn channel_with_capacity(
        manager: EgressManager,
        command_capacity: NonZeroUsize,
    ) -> (EgressHandle, Self) {
        let (tx, rx) = mpsc::channel(command_capacity.get());
        let (settings_tx, settings_rx) = mpsc::unbounded_channel();
        (
            EgressHandle::new(tx, settings_tx),
            Self::new(manager, rx, settings_rx),
        )
    }

    pub fn channel_with_capacity_and_rate(
        manager: EgressManager,
        command_capacity: NonZeroUsize,
        bytes_per_second: u64,
    ) -> (EgressHandle, Self) {
        let (tx, rx) = mpsc::channel(command_capacity.get());
        let (settings_tx, settings_rx) = mpsc::unbounded_channel();
        (
            EgressHandle::new(tx, settings_tx),
            Self::with_rate(manager, rx, settings_rx, bytes_per_second),
        )
    }

    pub async fn run(mut self) {
        let mut dispatch_deferred = false;
        let mut commands_closed = false;
        let mut settings_closed = false;

        loop {
            if !settings_closed && !self.drain_settings_batch() {
                settings_closed = true;
            }

            if commands_closed && settings_closed {
                break;
            }

            if dispatch_deferred {
                let delay = self
                    .rate_limiter
                    .delay_until_ready(Instant::now())
                    .unwrap_or(Duration::ZERO);

                tokio::select! {
                    setting = self.settings_rx.recv(), if !settings_closed => {
                        match setting {
                            Some(setting) => self.handle_settings_command(setting),
                            None => settings_closed = true,
                        }
                    }
                    command = self.rx.recv(), if !commands_closed => {
                        let Some(command) = command else {
                            commands_closed = true;
                            dispatch_deferred = self.pump_dispatch().is_rate_limited();
                            continue;
                        };
                        self.handle_command(command);
                    }
                    () = time::sleep(delay) => {}
                }
            } else {
                tokio::select! {
                    biased;
                    setting = self.settings_rx.recv(), if !settings_closed => {
                        match setting {
                            Some(setting) => self.handle_settings_command(setting),
                            None => settings_closed = true,
                        }
                    }
                    command = self.rx.recv(), if !commands_closed => {
                        match command {
                            Some(command) => self.handle_command(command),
                            None => commands_closed = true,
                        }
                    }
                }
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
                priority,
                reply_tx,
            } => {
                let result = match stage_server_message_with_priority(
                    &mut self.manager,
                    connection_id,
                    &message,
                    message_id,
                    priority,
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
            EgressCommand::StageFrame {
                connection_id,
                frame,
                reply_tx,
            } => {
                let _ = reply_tx.send(self.manager.enqueue_frame(connection_id, frame));
            }
            EgressCommand::StagePriorityFrame {
                connection_id,
                frame,
                reply_tx,
            } => {
                let _ = reply_tx.send(self.manager.enqueue_priority_frame(connection_id, frame));
            }
            EgressCommand::BeginStreamFrame {
                connection_id,
                header,
                reply_tx,
            } => {
                let _ = reply_tx.send(self.manager.begin_stream_frame(connection_id, header));
            }
            EgressCommand::StageStreamChunk {
                connection_id,
                chunk,
                reply_tx,
            } => {
                let _ = reply_tx.send(self.manager.stage_stream_chunk(connection_id, chunk));
            }
            EgressCommand::FinishStreamFrame {
                connection_id,
                reply_tx,
            } => {
                let _ = reply_tx.send(self.manager.finish_stream_frame(connection_id));
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
        }
    }

    fn handle_settings_command(&mut self, command: EgressSettingsCommand) {
        match command {
            EgressSettingsCommand::TransitionToUser {
                connection_id,
                user_id,
                weight,
            } => {
                self.manager
                    .transition_to_user(connection_id, user_id, weight);
            }
            EgressSettingsCommand::UpdateUserWeight { user_id, weight } => {
                self.manager.update_user_weight(user_id, weight);
            }
            EgressSettingsCommand::SetMaxOutboundRate { bytes_per_second } => {
                self.rate_limiter
                    .set_rate(bytes_per_second, self.manager.chunk_size());
            }
            EgressSettingsCommand::SetChunkSize { chunk_size } => {
                self.manager.set_chunk_size(chunk_size);
                self.rate_limiter.set_chunk_size(chunk_size);
            }
        }
    }

    fn drain_settings_batch(&mut self) -> bool {
        for _ in 0..EGRESS_SETTINGS_BATCH_LIMIT {
            match self.settings_rx.try_recv() {
                Ok(command) => self.handle_settings_command(command),
                Err(mpsc::error::TryRecvError::Empty) => return true,
                Err(mpsc::error::TryRecvError::Disconnected) => return false,
            }
        }

        true
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
    use std::sync::Arc;
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

    fn frame_with_chunks(chunks: usize, chunk_size: usize) -> Arc<[u8]> {
        vec![0_u8; chunks * chunk_size].into()
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
        handle.set_max_outbound_rate(0).unwrap();
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

        handle.set_chunk_size(nonzero(50)).unwrap();
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

    #[test]
    fn settings_send_does_not_use_bounded_command_capacity() {
        let (tx, _rx) = mpsc::channel(1);
        tx.try_send(EgressCommand::Unregister {
            connection_id: conn(99),
        })
        .unwrap();
        let (settings_tx, _settings_rx) = mpsc::unbounded_channel();
        let handle = EgressHandle::new(tx, settings_tx);

        handle.set_max_outbound_rate(1024).unwrap();
        handle.set_chunk_size(nonzero(2048)).unwrap();
        handle.update_user_weight(42, 9).unwrap();
        handle.transition_to_user(conn(1), 42, 9).unwrap();
    }

    #[test]
    fn settings_transition_then_weight_update_drives_dispatch_share() {
        let connection = conn(1);
        let peer = conn(2);
        let (dispatch_tx, mut dispatch_rx) = dispatch_channel();
        let mut manager = EgressManager::new(nonzero(100));
        assert!(manager.register_anon(connection, ConnectionClass::Protocol, dispatch_tx.clone(),));
        assert!(manager.register(user_registration(peer, 2, dispatch_tx)));

        let (_tx, rx) = mpsc::channel(1);
        let (settings_tx, settings_rx) = mpsc::unbounded_channel();
        let mut task = EgressTask::new(manager, rx, settings_rx);

        settings_tx
            .send(EgressSettingsCommand::TransitionToUser {
                connection_id: connection,
                user_id: 1,
                weight: 1,
            })
            .unwrap();
        settings_tx
            .send(EgressSettingsCommand::UpdateUserWeight {
                user_id: 1,
                weight: 10,
            })
            .unwrap();
        assert!(task.drain_settings_batch());

        task.manager
            .enqueue_frame(connection, frame_with_chunks(80, 100))
            .unwrap();
        task.manager
            .enqueue_frame(peer, frame_with_chunks(80, 100))
            .unwrap();

        let mut connection_seen = 0;
        let mut peer_seen = 0;
        for _ in 0..80 {
            match task.manager.dispatch_next() {
                DispatchOutcome::Dispatched { connection_id, .. } => {
                    let dispatch = dispatch_rx
                        .try_recv()
                        .expect("dispatch should reach the shared receiver");
                    assert_eq!(dispatch.connection_id, connection_id);
                    if connection_id == connection {
                        connection_seen += 1;
                    } else {
                        assert_eq!(connection_id, peer);
                        peer_seen += 1;
                    }
                    assert!(task.manager.ack(connection_id));
                }
                other => panic!("expected dispatch, got {other:?}"),
            }
        }

        assert!(
            connection_seen > peer_seen * 3,
            "expected updated 10:1 flow share, got {connection_seen}:{peer_seen}"
        );
    }

    #[test]
    fn settings_update_before_transition_uses_transition_weight() {
        let connection = conn(1);
        let peer = conn(2);
        let (dispatch_tx, mut dispatch_rx) = dispatch_channel();
        let mut manager = EgressManager::new(nonzero(100));
        assert!(manager.register_anon(connection, ConnectionClass::Protocol, dispatch_tx.clone(),));
        assert!(manager.register(user_registration(peer, 2, dispatch_tx)));

        let (_tx, rx) = mpsc::channel(1);
        let (settings_tx, settings_rx) = mpsc::unbounded_channel();
        let mut task = EgressTask::new(manager, rx, settings_rx);

        settings_tx
            .send(EgressSettingsCommand::UpdateUserWeight {
                user_id: 1,
                weight: 10,
            })
            .unwrap();
        settings_tx
            .send(EgressSettingsCommand::TransitionToUser {
                connection_id: connection,
                user_id: 1,
                weight: 10,
            })
            .unwrap();
        assert!(task.drain_settings_batch());

        task.manager
            .enqueue_frame(connection, frame_with_chunks(80, 100))
            .unwrap();
        task.manager
            .enqueue_frame(peer, frame_with_chunks(80, 100))
            .unwrap();

        let mut connection_seen = 0;
        let mut peer_seen = 0;
        for _ in 0..80 {
            match task.manager.dispatch_next() {
                DispatchOutcome::Dispatched { connection_id, .. } => {
                    let dispatch = dispatch_rx
                        .try_recv()
                        .expect("dispatch should reach the shared receiver");
                    assert_eq!(dispatch.connection_id, connection_id);
                    if connection_id == connection {
                        connection_seen += 1;
                    } else {
                        assert_eq!(connection_id, peer);
                        peer_seen += 1;
                    }
                    assert!(task.manager.ack(connection_id));
                }
                other => panic!("expected dispatch, got {other:?}"),
            }
        }

        assert!(
            connection_seen > peer_seen * 3,
            "expected transition-applied 10:1 flow share, got {connection_seen}:{peer_seen}"
        );
    }

    #[test]
    fn settings_batch_cap_allows_dispatch_before_remaining_settings() {
        let connection = conn(1);
        let (dispatch_tx, mut dispatch_rx) = dispatch_channel();
        let mut manager = EgressManager::new(nonzero(100));
        assert!(manager.register_anon(connection, ConnectionClass::Protocol, dispatch_tx));
        assert_eq!(
            manager
                .enqueue_frame(connection, frame_with_chunks(1, 100))
                .unwrap(),
            1
        );

        let (_tx, rx) = mpsc::channel(1);
        let (settings_tx, settings_rx) = mpsc::unbounded_channel();
        for _ in 0..EGRESS_SETTINGS_BATCH_LIMIT {
            settings_tx
                .send(EgressSettingsCommand::SetMaxOutboundRate {
                    bytes_per_second: 0,
                })
                .unwrap();
        }
        settings_tx
            .send(EgressSettingsCommand::SetChunkSize {
                chunk_size: nonzero(777),
            })
            .unwrap();
        let mut task = EgressTask::new(manager, rx, settings_rx);

        assert!(task.drain_settings_batch());
        assert_eq!(task.manager.chunk_size(), nonzero(100));
        assert_eq!(task.pump_dispatch(), PumpResult::Empty);
        let dispatch = dispatch_rx
            .try_recv()
            .expect("dispatch should progress after one settings batch");
        assert_eq!(dispatch.connection_id, connection);
        assert!(task.manager.ack(connection));

        assert!(task.drain_settings_batch());
        assert_eq!(task.manager.chunk_size(), nonzero(777));
    }

    #[test]
    fn finite_rate_limiter_preserves_weighted_fairness() {
        let mut manager = EgressManager::new(nonzero(100));
        let heavy = conn(1);
        let light = conn(2);
        let (dispatch_tx, mut dispatch_rx) = dispatch_channel();

        assert!(manager.register(user_registration(heavy, 1, dispatch_tx.clone())));
        assert!(manager.register(user_registration(light, 2, dispatch_tx)));
        assert!(manager.update_user_weight(1, 3));
        assert!(manager.update_user_weight(2, 1));

        assert_eq!(
            manager
                .enqueue_frame(heavy, frame_with_chunks(128, 100))
                .unwrap(),
            128
        );
        assert_eq!(
            manager
                .enqueue_frame(light, frame_with_chunks(128, 100))
                .unwrap(),
            128
        );

        let (_tx, rx) = mpsc::channel(1);
        let (_settings_tx, settings_rx) = mpsc::unbounded_channel();
        let mut task = EgressTask::with_rate(manager, rx, settings_rx, 1);

        let mut heavy_seen = 0;
        let mut light_seen = 0;
        for _ in 0..128 {
            task.rate_limiter.tokens = 1;
            task.rate_limiter.last_refill = Instant::now();
            assert_eq!(task.pump_dispatch(), PumpResult::RateLimited);

            let dispatch = dispatch_rx
                .try_recv()
                .expect("one dispatch should be emitted for each token grant");
            assert_eq!(dispatch.chunk.len(), 100);
            if dispatch.connection_id == heavy {
                heavy_seen += 1;
            } else {
                assert_eq!(dispatch.connection_id, light);
                light_seen += 1;
            }
            assert!(task.manager.ack(dispatch.connection_id));
        }

        assert!(
            (92..=100).contains(&heavy_seen),
            "expected heavy flow near 3:1 share, got {heavy_seen}:{light_seen}"
        );
        assert!(
            (28..=36).contains(&light_seen),
            "expected light flow near 3:1 share, got {heavy_seen}:{light_seen}"
        );
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

        handle.transition_to_user(connection, 42, 5).unwrap();
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
    async fn stage_priority_frame_preserves_queue_full() {
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
                    message_id(b"000000000213"),
                )
                .await
                .unwrap(),
            Ok(1)
        ));
        let priority_frame =
            server_message_to_frame_bytes(&ServerMessage::Pong, message_id(b"000000000214"))
                .unwrap();

        let result = handle
            .stage_priority_frame(connection, priority_frame)
            .await
            .unwrap();

        assert_eq!(result, Err(EgressEnqueueError::QueueFull));

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
