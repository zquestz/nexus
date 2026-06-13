//! Egress manager for BBS-port scheduling.
//!
//! This module owns the manager-side lifecycle around the WF2Q+ scheduler:
//! registration, frame staging, chunking, dispatch, ack, and write-failure
//! cleanup.

pub mod task;

use std::{collections::HashMap, io, num::NonZeroUsize, sync::Arc};

use nexus_common::framing::MessageId;
use nexus_common::io::server_message_to_frame_bytes;
use nexus_common::protocol::ServerMessage;
use tokio::sync::mpsc;

use crate::scheduler::{
    ANON_FLOW_WEIGHT, ConnectionClass, ConnectionId, EnqueueError, FlowId, SchedulerPacket,
    Wf2qScheduler,
};

pub const DEFAULT_EGRESS_QUEUED_FRAMES_PER_CONNECTION: usize = 32;
pub const EGRESS_DISPATCH_QUEUE_CAPACITY: usize = 1;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EgressChunk {
    frame: Arc<[u8]>,
    offset: usize,
    len: usize,
    is_final_frame_chunk: bool,
}

impl EgressChunk {
    pub fn new(
        frame: Arc<[u8]>,
        offset: usize,
        len: usize,
        is_final_frame_chunk: bool,
    ) -> Option<Self> {
        if len == 0 {
            return None;
        }

        let end = offset.checked_add(len)?;
        if end > frame.len() {
            return None;
        }

        Some(Self {
            frame,
            offset,
            len,
            is_final_frame_chunk,
        })
    }

    pub fn as_bytes(&self) -> &[u8] {
        &self.frame[self.offset..self.offset + self.len]
    }

    pub fn len(&self) -> usize {
        self.len
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    pub fn is_final_frame_chunk(&self) -> bool {
        self.is_final_frame_chunk
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct EgressDispatch {
    pub connection_id: ConnectionId,
    pub chunk: EgressChunk,
}

pub type EgressDispatchTx = mpsc::Sender<EgressDispatch>;
pub type EgressDispatchRx = mpsc::Receiver<EgressDispatch>;

pub struct EgressRegistration {
    pub connection_id: ConnectionId,
    pub flow_id: FlowId,
    pub class: ConnectionClass,
    pub weight: u16,
    pub dispatch_tx: EgressDispatchTx,
}

#[derive(Debug, PartialEq, Eq)]
pub enum EgressEnqueueError {
    UnknownConnection,
    EmptyFrame,
    QueueFull,
    FrameInProgress,
    NoFrameInProgress,
}

#[derive(Debug)]
pub enum StagingError {
    Serialize(io::Error),
    Enqueue(EgressEnqueueError),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EgressMessagePriority {
    Normal,
    Priority,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DispatchOutcome {
    Dispatched {
        connection_id: ConnectionId,
        bytes: usize,
    },
    Unregistered {
        connection_id: ConnectionId,
    },
    Empty,
}

pub fn stage_server_message(
    manager: &mut EgressManager,
    connection_id: ConnectionId,
    message: &ServerMessage,
    message_id: MessageId,
) -> Result<usize, StagingError> {
    stage_server_message_with_priority(
        manager,
        connection_id,
        message,
        message_id,
        EgressMessagePriority::Normal,
    )
}

pub fn stage_server_message_with_priority(
    manager: &mut EgressManager,
    connection_id: ConnectionId,
    message: &ServerMessage,
    message_id: MessageId,
    priority: EgressMessagePriority,
) -> Result<usize, StagingError> {
    let frame =
        server_message_to_frame_bytes(message, message_id).map_err(StagingError::Serialize)?;
    manager
        .enqueue_frame_with_priority(connection_id, frame, priority)
        .map_err(StagingError::Enqueue)
}

pub struct EgressManager {
    scheduler: Wf2qScheduler<EgressChunk>,
    connections: HashMap<ConnectionId, EgressConnection>,
    chunk_size: NonZeroUsize,
    frame_limit: usize,
}

impl EgressManager {
    pub fn new(chunk_size: NonZeroUsize) -> Self {
        Self::with_frame_limit(chunk_size, DEFAULT_EGRESS_QUEUED_FRAMES_PER_CONNECTION)
    }

    pub fn with_frame_limit(chunk_size: NonZeroUsize, frame_limit: usize) -> Self {
        Self {
            scheduler: Wf2qScheduler::new(),
            connections: HashMap::new(),
            chunk_size,
            frame_limit,
        }
    }

    pub fn chunk_size(&self) -> NonZeroUsize {
        self.chunk_size
    }

    pub fn set_chunk_size(&mut self, chunk_size: NonZeroUsize) {
        self.chunk_size = chunk_size;
    }

    pub fn register(&mut self, registration: EgressRegistration) -> bool {
        if self.connections.contains_key(&registration.connection_id) {
            return false;
        }

        if !self.scheduler.register_connection(
            registration.connection_id,
            registration.flow_id,
            registration.class,
            registration.weight,
        ) {
            return false;
        }

        self.connections.insert(
            registration.connection_id,
            EgressConnection {
                dispatch_tx: registration.dispatch_tx,
                staged_frames: 0,
                in_flight_is_final: None,
                stream_frame_active: false,
            },
        );
        true
    }

    pub fn register_anon(
        &mut self,
        connection_id: ConnectionId,
        class: ConnectionClass,
        dispatch_tx: EgressDispatchTx,
    ) -> bool {
        self.register(EgressRegistration {
            connection_id,
            flow_id: FlowId::Anon,
            class,
            weight: ANON_FLOW_WEIGHT,
            dispatch_tx,
        })
    }

    pub fn unregister(&mut self, connection_id: ConnectionId) -> bool {
        let removed = self.connections.remove(&connection_id).is_some();
        self.scheduler.unregister_connection(connection_id) || removed
    }

    pub fn set_blocked(&mut self, connection_id: ConnectionId, blocked: bool) -> bool {
        self.scheduler
            .set_connection_blocked(connection_id, blocked)
    }

    pub fn transition_to_user(
        &mut self,
        connection_id: ConnectionId,
        user_id: i64,
        weight: u16,
    ) -> bool {
        self.scheduler
            .transition_to_user(connection_id, user_id, weight)
    }

    pub fn update_user_weight(&mut self, user_id: i64, weight: u16) -> bool {
        self.scheduler.update_user_weight(user_id, weight)
    }

    pub fn enqueue_frame(
        &mut self,
        connection_id: ConnectionId,
        frame: Arc<[u8]>,
    ) -> Result<usize, EgressEnqueueError> {
        self.enqueue_frame_with_priority(connection_id, frame, EgressMessagePriority::Normal)
    }

    pub fn enqueue_priority_frame(
        &mut self,
        connection_id: ConnectionId,
        frame: Arc<[u8]>,
    ) -> Result<usize, EgressEnqueueError> {
        self.enqueue_frame_with_priority(connection_id, frame, EgressMessagePriority::Priority)
    }

    fn enqueue_frame_with_priority(
        &mut self,
        connection_id: ConnectionId,
        frame: Arc<[u8]>,
        priority: EgressMessagePriority,
    ) -> Result<usize, EgressEnqueueError> {
        if frame.is_empty() {
            return Err(EgressEnqueueError::EmptyFrame);
        }

        let Some(connection) = self.connections.get(&connection_id) else {
            return Err(EgressEnqueueError::UnknownConnection);
        };

        if connection.stream_frame_active {
            return Err(EgressEnqueueError::FrameInProgress);
        }

        if connection.staged_frames >= self.frame_limit {
            return Err(EgressEnqueueError::QueueFull);
        }

        let chunks =
            self.enqueue_chunked_payload(connection_id, frame, priority, FinalChunk::Last)?;

        if let Some(connection) = self.connections.get_mut(&connection_id) {
            connection.staged_frames += 1;
        }
        Ok(chunks)
    }

    pub fn begin_stream_frame(
        &mut self,
        connection_id: ConnectionId,
        header: Arc<[u8]>,
    ) -> Result<usize, EgressEnqueueError> {
        if header.is_empty() {
            return Err(EgressEnqueueError::EmptyFrame);
        }

        let Some(connection) = self.connections.get(&connection_id) else {
            return Err(EgressEnqueueError::UnknownConnection);
        };

        if connection.stream_frame_active {
            return Err(EgressEnqueueError::FrameInProgress);
        }

        if connection.staged_frames >= self.frame_limit {
            return Err(EgressEnqueueError::QueueFull);
        }

        let chunks = self.enqueue_chunked_payload(
            connection_id,
            header,
            EgressMessagePriority::Normal,
            FinalChunk::None,
        )?;

        if let Some(connection) = self.connections.get_mut(&connection_id) {
            connection.staged_frames += 1;
            connection.stream_frame_active = true;
        }
        Ok(chunks)
    }

    pub fn stage_stream_chunk(
        &mut self,
        connection_id: ConnectionId,
        chunk: Arc<[u8]>,
    ) -> Result<usize, EgressEnqueueError> {
        if chunk.is_empty() {
            return Err(EgressEnqueueError::EmptyFrame);
        }

        let Some(connection) = self.connections.get(&connection_id) else {
            return Err(EgressEnqueueError::UnknownConnection);
        };

        if !connection.stream_frame_active {
            return Err(EgressEnqueueError::NoFrameInProgress);
        }

        self.enqueue_chunked_payload(
            connection_id,
            chunk,
            EgressMessagePriority::Normal,
            FinalChunk::None,
        )
    }

    pub fn finish_stream_frame(
        &mut self,
        connection_id: ConnectionId,
    ) -> Result<usize, EgressEnqueueError> {
        let Some(connection) = self.connections.get(&connection_id) else {
            return Err(EgressEnqueueError::UnknownConnection);
        };

        if !connection.stream_frame_active {
            return Err(EgressEnqueueError::NoFrameInProgress);
        }

        self.enqueue_chunked_payload(
            connection_id,
            Arc::from(&b"\n"[..]),
            EgressMessagePriority::Normal,
            FinalChunk::Last,
        )
    }

    fn enqueue_chunked_payload(
        &mut self,
        connection_id: ConnectionId,
        frame: Arc<[u8]>,
        priority: EgressMessagePriority,
        final_chunk: FinalChunk,
    ) -> Result<usize, EgressEnqueueError> {
        let mut chunks = 0;
        let chunk_size = self.chunk_size.get();
        for offset in (0..frame.len()).step_by(chunk_size) {
            let len = chunk_size.min(frame.len() - offset);
            let is_final_frame_chunk =
                final_chunk == FinalChunk::Last && offset + len == frame.len();
            let Some(chunk) =
                EgressChunk::new(Arc::clone(&frame), offset, len, is_final_frame_chunk)
            else {
                self.unregister(connection_id);
                return Err(EgressEnqueueError::EmptyFrame);
            };
            let mut packet = match priority {
                EgressMessagePriority::Normal => SchedulerPacket::new(chunk.len(), chunk),
                EgressMessagePriority::Priority => SchedulerPacket::priority(chunk.len(), chunk),
            };
            // Carry per-chunk finality so the scheduler keeps a partially
            // dispatched frame's chunks contiguous on the wire.
            packet.is_final = is_final_frame_chunk;
            if let Err(err) = self.scheduler.enqueue(connection_id, packet) {
                // This should be unreachable after the manager-map check and
                // non-empty chunk construction. If the manager/scheduler
                // invariant is ever broken, remove the connection so a partial
                // frame cannot later dispatch.
                self.unregister(connection_id);
                return Err(match err {
                    EnqueueError::UnknownConnection { .. } => EgressEnqueueError::UnknownConnection,
                    EnqueueError::EmptyPacket { .. } => EgressEnqueueError::EmptyFrame,
                });
            }
            chunks += 1;
        }

        Ok(chunks)
    }

    pub fn dispatch_next(&mut self) -> DispatchOutcome {
        let Some(packet) = self.scheduler.dequeue() else {
            return DispatchOutcome::Empty;
        };

        let connection_id = packet.connection_id;
        let Some(connection) = self.connections.get_mut(&connection_id) else {
            self.scheduler.unregister_connection(connection_id);
            return DispatchOutcome::Unregistered { connection_id };
        };

        let is_final_frame_chunk = packet.payload.is_final_frame_chunk();
        let bytes = packet.payload.len();
        let dispatch = EgressDispatch {
            connection_id,
            chunk: packet.payload,
        };
        connection.in_flight_is_final = Some(is_final_frame_chunk);
        if connection.dispatch_tx.try_send(dispatch).is_err() {
            self.unregister(connection_id);
            return DispatchOutcome::Unregistered { connection_id };
        }

        DispatchOutcome::Dispatched {
            connection_id,
            bytes,
        }
    }

    pub fn ack(&mut self, connection_id: ConnectionId) -> bool {
        let Some(connection) = self.connections.get_mut(&connection_id) else {
            return false;
        };

        if connection.in_flight_is_final.take() == Some(true) {
            connection.staged_frames = connection.staged_frames.saturating_sub(1);
            connection.stream_frame_active = false;
        }

        self.scheduler
            .set_connection_in_flight(connection_id, false)
    }

    pub fn write_failed(&mut self, connection_id: ConnectionId) -> bool {
        self.unregister(connection_id)
    }

    pub fn queued_packets(&self, connection_id: ConnectionId) -> Option<usize> {
        self.scheduler.queued_packets(connection_id)
    }

    pub fn has_dispatchable_packet(&self) -> bool {
        self.scheduler.has_dispatchable_packet()
    }

    #[cfg(test)]
    fn has_connection(&self, connection_id: ConnectionId) -> bool {
        self.connections.contains_key(&connection_id)
    }
}

struct EgressConnection {
    dispatch_tx: EgressDispatchTx,
    staged_frames: usize,
    in_flight_is_final: Option<bool>,
    stream_frame_active: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum FinalChunk {
    None,
    Last,
}

#[cfg(test)]
mod tests {
    use super::*;
    use nexus_common::framing::MessageId;
    use nexus_common::io::server_message_to_frame_bytes;
    use nexus_common::protocol::{ChatAction, ServerMessage};
    use nexus_common::validators::DEFAULT_CHANNEL;

    fn conn(id: u64) -> ConnectionId {
        ConnectionId::new(id)
    }

    fn manager(chunk_size: usize) -> EgressManager {
        EgressManager::new(NonZeroUsize::new(chunk_size).unwrap())
    }

    fn manager_with_frame_limit(chunk_size: usize, frame_limit: usize) -> EgressManager {
        EgressManager::with_frame_limit(NonZeroUsize::new(chunk_size).unwrap(), frame_limit)
    }

    fn channel() -> (EgressDispatchTx, EgressDispatchRx) {
        mpsc::channel(1)
    }

    fn frame(bytes: &[u8]) -> Arc<[u8]> {
        Arc::from(bytes)
    }

    fn user_registration(
        connection_id: ConnectionId,
        user_id: i64,
        dispatch_tx: EgressDispatchTx,
    ) -> EgressRegistration {
        user_registration_with_class_and_weight(
            connection_id,
            user_id,
            ConnectionClass::Protocol,
            1,
            dispatch_tx,
        )
    }

    fn user_registration_with_class_and_weight(
        connection_id: ConnectionId,
        user_id: i64,
        class: ConnectionClass,
        weight: u16,
        dispatch_tx: EgressDispatchTx,
    ) -> EgressRegistration {
        EgressRegistration {
            connection_id,
            flow_id: FlowId::User(user_id),
            class,
            weight,
            dispatch_tx,
        }
    }

    fn expect_dispatch(manager: &mut EgressManager) -> ConnectionId {
        let DispatchOutcome::Dispatched { connection_id, .. } = manager.dispatch_next() else {
            panic!("expected dispatch");
        };
        connection_id
    }

    fn message_id(bytes: &[u8]) -> MessageId {
        MessageId::from_bytes(bytes).unwrap()
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
    fn chunk_borrows_shared_frame_storage() {
        let frame = frame(b"abcdef");
        let chunk = EgressChunk::new(Arc::clone(&frame), 2, 3, true).unwrap();

        assert_eq!(chunk.as_bytes(), b"cde");
        assert_eq!(chunk.len(), 3);
        assert!(chunk.is_final_frame_chunk());
        assert_eq!(Arc::strong_count(&frame), 2);
        assert!(EgressChunk::new(Arc::clone(&frame), 6, 1, false).is_none());
        assert!(EgressChunk::new(frame, 0, 0, false).is_none());
    }

    #[test]
    fn register_and_unregister_connection() {
        let mut manager = manager(4);
        let connection = conn(1);
        let (tx, _rx) = channel();

        assert!(manager.register(user_registration(connection, 7, tx)));
        assert!(manager.has_connection(connection));
        assert_eq!(manager.queued_packets(connection), Some(0));

        assert!(manager.unregister(connection));
        assert!(!manager.has_connection(connection));
        assert_eq!(manager.queued_packets(connection), None);
    }

    #[test]
    fn register_anon_uses_global_anon_weight() {
        let mut manager = manager(4);
        let anon = conn(1);
        let user = conn(2);
        let (anon_tx, mut anon_rx) = channel();
        let (user_tx, mut user_rx) = channel();
        assert!(manager.register_anon(anon, ConnectionClass::Protocol, anon_tx));
        assert!(manager.register(user_registration(user, 1, user_tx)));
        assert!(manager.update_user_weight(1, ANON_FLOW_WEIGHT));
        assert_eq!(
            manager.enqueue_frame(anon, Arc::from(vec![b'a'; 80])),
            Ok(20)
        );
        assert_eq!(
            manager.enqueue_frame(user, Arc::from(vec![b'u'; 80])),
            Ok(20)
        );

        let mut anon_count: usize = 0;
        let mut user_count: usize = 0;
        for _ in 0..40 {
            let connection_id = expect_dispatch(&mut manager);
            if connection_id == anon {
                anon_count += 1;
                assert_eq!(anon_rx.try_recv().unwrap().chunk.as_bytes(), b"aaaa");
            } else {
                assert_eq!(connection_id, user);
                user_count += 1;
                assert_eq!(user_rx.try_recv().unwrap().chunk.as_bytes(), b"uuuu");
            }
            assert!(manager.ack(connection_id));
        }

        assert_eq!(anon_count, 20);
        assert_eq!(user_count, 20);
    }

    #[test]
    fn duplicate_register_and_absent_unregister_are_rejected() {
        let mut manager = manager(4);
        let connection = conn(1);
        let (first_tx, _first_rx) = channel();
        let (second_tx, _second_rx) = channel();

        assert!(manager.register(user_registration(connection, 7, first_tx)));
        assert!(!manager.register(user_registration(connection, 7, second_tx)));
        assert!(!manager.unregister(conn(999)));
    }

    #[test]
    fn unknown_control_operations_return_false() {
        let mut manager = manager(4);

        assert!(!manager.set_blocked(conn(999), true));
        assert!(!manager.transition_to_user(conn(999), 1, 1));
        assert!(!manager.update_user_weight(999, 1));
    }

    #[test]
    fn enqueue_frame_rejects_unknown_connection_and_empty_frame() {
        let mut manager = manager(4);
        let connection = conn(1);

        assert_eq!(
            manager.enqueue_frame(connection, frame(b"hello")),
            Err(EgressEnqueueError::UnknownConnection)
        );

        let (tx, _rx) = channel();
        assert!(manager.register(user_registration(connection, 7, tx)));
        assert_eq!(
            manager.enqueue_frame(connection, frame(b"")),
            Err(EgressEnqueueError::EmptyFrame)
        );
    }

    #[test]
    fn staged_frame_limit_rejects_without_queueing_chunks() {
        let mut manager = manager_with_frame_limit(4, 2);
        let connection = conn(1);
        let (tx, _rx) = channel();
        assert!(manager.register(user_registration(connection, 1, tx)));

        assert_eq!(manager.enqueue_frame(connection, frame(b"one")), Ok(1));
        assert_eq!(manager.enqueue_frame(connection, frame(b"two")), Ok(1));
        assert_eq!(manager.queued_packets(connection), Some(2));

        assert_eq!(
            manager.enqueue_frame(connection, frame(b"three")),
            Err(EgressEnqueueError::QueueFull)
        );
        assert_eq!(manager.queued_packets(connection), Some(2));
    }

    #[test]
    fn multi_chunk_frame_counts_as_one_staged_frame() {
        let mut manager = manager_with_frame_limit(4, 1);
        let connection = conn(1);
        let (tx, _rx) = channel();
        assert!(manager.register(user_registration(connection, 1, tx)));

        assert_eq!(manager.enqueue_frame(connection, frame(b"abcdef")), Ok(2));
        assert_eq!(manager.queued_packets(connection), Some(2));
        assert_eq!(
            manager.enqueue_frame(connection, frame(b"next")),
            Err(EgressEnqueueError::QueueFull)
        );
    }

    #[test]
    fn non_final_chunk_ack_does_not_free_frame_capacity() {
        let mut manager = manager_with_frame_limit(4, 1);
        let connection = conn(1);
        let (tx, mut rx) = channel();
        assert!(manager.register(user_registration(connection, 1, tx)));
        assert_eq!(manager.enqueue_frame(connection, frame(b"abcdef")), Ok(2));
        assert_eq!(
            manager.enqueue_frame(connection, frame(b"next")),
            Err(EgressEnqueueError::QueueFull)
        );

        assert_eq!(expect_dispatch(&mut manager), connection);
        let dispatch = rx.try_recv().unwrap();
        assert_eq!(dispatch.chunk.as_bytes(), b"abcd");
        assert!(!dispatch.chunk.is_final_frame_chunk());
        assert!(manager.ack(connection));

        assert_eq!(
            manager.enqueue_frame(connection, frame(b"next")),
            Err(EgressEnqueueError::QueueFull)
        );

        assert_eq!(expect_dispatch(&mut manager), connection);
        let dispatch = rx.try_recv().unwrap();
        assert_eq!(dispatch.chunk.as_bytes(), b"ef");
        assert!(dispatch.chunk.is_final_frame_chunk());
        assert!(manager.ack(connection));

        assert_eq!(manager.enqueue_frame(connection, frame(b"next")), Ok(1));
    }

    #[test]
    fn final_chunk_ack_frees_frame_capacity() {
        let mut manager = manager_with_frame_limit(4, 1);
        let connection = conn(1);
        let (tx, mut rx) = channel();
        assert!(manager.register(user_registration(connection, 1, tx)));
        assert_eq!(manager.enqueue_frame(connection, frame(b"abcd")), Ok(1));

        assert_eq!(expect_dispatch(&mut manager), connection);
        let dispatch = rx.try_recv().unwrap();
        assert_eq!(dispatch.chunk.as_bytes(), b"abcd");
        assert!(dispatch.chunk.is_final_frame_chunk());
        assert!(manager.ack(connection));

        assert_eq!(manager.enqueue_frame(connection, frame(b"next")), Ok(1));
    }

    #[test]
    fn streaming_frame_reassembles_and_releases_slot_on_terminator_ack() {
        let mut manager = manager_with_frame_limit(4, 1);
        let connection = conn(1);
        let (tx, mut rx) = channel();
        assert!(manager.register(user_registration(connection, 1, tx)));

        assert_eq!(
            manager.begin_stream_frame(connection, frame(b"header")),
            Ok(2)
        );
        assert_eq!(
            manager.stage_stream_chunk(connection, frame(b"abcdef")),
            Ok(2)
        );
        assert_eq!(manager.finish_stream_frame(connection), Ok(1));
        assert_eq!(
            manager.enqueue_frame(connection, frame(b"control")),
            Err(EgressEnqueueError::FrameInProgress)
        );

        let mut bytes = Vec::new();
        for expected_final in [false, false, false, false, true] {
            assert_eq!(expect_dispatch(&mut manager), connection);
            let dispatch = rx.try_recv().unwrap();
            bytes.extend_from_slice(dispatch.chunk.as_bytes());
            assert_eq!(dispatch.chunk.is_final_frame_chunk(), expected_final);
            assert!(manager.ack(connection));
            if !expected_final {
                assert_eq!(
                    manager.enqueue_frame(connection, frame(b"next")),
                    Err(EgressEnqueueError::FrameInProgress)
                );
            }
        }

        assert_eq!(bytes, b"headerabcdef\n");
        assert_eq!(manager.enqueue_frame(connection, frame(b"next")), Ok(1));
    }

    #[test]
    fn stream_chunk_and_finish_require_active_stream_frame() {
        let mut manager = manager(4);
        let connection = conn(1);
        let (tx, _rx) = channel();
        assert!(manager.register(user_registration(connection, 1, tx)));

        assert_eq!(
            manager.stage_stream_chunk(connection, frame(b"data")),
            Err(EgressEnqueueError::NoFrameInProgress)
        );
        assert_eq!(
            manager.finish_stream_frame(connection),
            Err(EgressEnqueueError::NoFrameInProgress)
        );
    }

    #[test]
    fn stream_frame_active_rejects_normal_and_priority_frames() {
        let mut manager = manager(4);
        let connection = conn(1);
        let (tx, _rx) = channel();
        assert!(manager.register(user_registration(connection, 1, tx)));

        assert_eq!(
            manager.begin_stream_frame(connection, frame(b"header")),
            Ok(2)
        );
        assert_eq!(
            manager.enqueue_frame(connection, frame(b"normal")),
            Err(EgressEnqueueError::FrameInProgress)
        );
        assert_eq!(
            manager.enqueue_priority_frame(connection, frame(b"priority")),
            Err(EgressEnqueueError::FrameInProgress)
        );
        assert_eq!(
            manager.begin_stream_frame(connection, frame(b"other")),
            Err(EgressEnqueueError::FrameInProgress)
        );
    }

    #[test]
    fn unregister_clears_active_stream_frame_state() {
        let mut manager = manager(4);
        let connection = conn(1);
        let (first_tx, _first_rx) = channel();
        assert!(manager.register(user_registration(connection, 1, first_tx)));
        assert_eq!(
            manager.begin_stream_frame(connection, frame(b"header")),
            Ok(2)
        );

        assert!(manager.write_failed(connection));
        assert_eq!(manager.queued_packets(connection), None);

        let (second_tx, _second_rx) = channel();
        assert!(manager.register(user_registration(connection, 1, second_tx)));
        assert_eq!(manager.enqueue_frame(connection, frame(b"next")), Ok(1));
    }

    #[test]
    fn priority_frame_overtakes_normal_frame_at_connection_boundary() {
        let mut manager = manager(8);
        let connection = conn(1);
        let (tx, mut rx) = channel();
        assert!(manager.register(user_registration(connection, 1, tx)));

        assert_eq!(manager.enqueue_frame(connection, frame(b"normal")), Ok(1));
        assert_eq!(
            manager.enqueue_priority_frame(connection, frame(b"priority")),
            Ok(1)
        );

        assert_eq!(expect_dispatch(&mut manager), connection);
        let dispatch = rx.try_recv().unwrap();
        assert_eq!(dispatch.chunk.as_bytes(), b"priority");
        assert!(manager.ack(connection));

        assert_eq!(expect_dispatch(&mut manager), connection);
        let dispatch = rx.try_recv().unwrap();
        assert_eq!(dispatch.chunk.as_bytes(), b"normal");
    }

    #[test]
    fn priority_frame_does_not_interleave_inside_multi_chunk_normal_frame() {
        // Chunk size 4 so an 8-byte frame splits into two chunks.
        let mut manager = manager(4);
        let connection = conn(1);
        let (tx, mut rx) = channel();
        assert!(manager.register(user_registration(connection, 1, tx)));

        // A two-chunk normal frame.
        assert_eq!(manager.enqueue_frame(connection, frame(b"AAAABBBB")), Ok(2));

        // Dispatch chunk 1 of the normal frame.
        assert_eq!(expect_dispatch(&mut manager), connection);
        assert_eq!(rx.try_recv().unwrap().chunk.as_bytes(), b"AAAA");
        assert!(manager.ack(connection));

        // A priority frame is staged while the normal frame is mid-dispatch.
        assert_eq!(
            manager.enqueue_priority_frame(connection, frame(b"PRIO")),
            Ok(1)
        );

        // The next dispatch must finish the normal frame (chunk 2), not let the
        // priority frame's bytes interleave inside the normal frame's payload.
        assert_eq!(expect_dispatch(&mut manager), connection);
        assert_eq!(
            rx.try_recv().unwrap().chunk.as_bytes(),
            b"BBBB",
            "priority frame must not interleave inside the normal frame"
        );
        assert!(manager.ack(connection));

        // At the frame boundary, the priority frame dispatches.
        assert_eq!(expect_dispatch(&mut manager), connection);
        assert_eq!(rx.try_recv().unwrap().chunk.as_bytes(), b"PRIO");
        assert!(manager.ack(connection));
    }

    #[test]
    fn priority_frame_preserves_cross_flow_fairness_at_manager_layer() {
        let mut manager = manager(4);
        let priority_connection = conn(1);
        let other_connection = conn(2);
        let (priority_tx, mut priority_rx) = channel();
        let (other_tx, mut other_rx) = channel();
        assert!(manager.register(user_registration(priority_connection, 1, priority_tx)));
        assert!(manager.register(user_registration(other_connection, 2, other_tx)));

        assert_eq!(
            manager.enqueue_frame(priority_connection, Arc::from(vec![b'a'; 400])),
            Ok(100)
        );
        assert_eq!(
            manager.enqueue_frame(other_connection, Arc::from(vec![b'b'; 400])),
            Ok(100)
        );
        assert_eq!(
            manager.enqueue_priority_frame(priority_connection, frame(b"pppp")),
            Ok(1)
        );

        let mut priority_count = 0usize;
        let mut other_count = 0usize;
        let mut saw_priority_chunk = false;
        for _ in 0..201 {
            let connection_id = expect_dispatch(&mut manager);
            if connection_id == priority_connection {
                priority_count += 1;
                let dispatch = priority_rx.try_recv().unwrap();
                saw_priority_chunk |= dispatch.chunk.as_bytes() == b"pppp";
            } else {
                assert_eq!(connection_id, other_connection);
                other_count += 1;
                assert_eq!(other_rx.try_recv().unwrap().chunk.as_bytes(), b"bbbb");
            }
            assert!(
                priority_count.abs_diff(other_count) <= 2,
                "priority preemption should not starve the other flow; running split was {priority_count}:{other_count}"
            );
            assert!(manager.ack(connection_id));
        }

        assert!(saw_priority_chunk);
        assert_eq!(priority_count, 101);
        assert_eq!(other_count, 100);
        assert_eq!(manager.dispatch_next(), DispatchOutcome::Empty);
    }

    #[test]
    fn resume_cycle_succeeds_after_final_chunk_ack() {
        let mut manager = manager_with_frame_limit(4, 1);
        let connection = conn(1);
        let (tx, mut rx) = channel();
        assert!(manager.register(user_registration(connection, 1, tx)));
        assert_eq!(manager.enqueue_frame(connection, frame(b"abcdef")), Ok(2));
        assert_eq!(
            manager.enqueue_frame(connection, frame(b"next")),
            Err(EgressEnqueueError::QueueFull)
        );

        assert_eq!(expect_dispatch(&mut manager), connection);
        assert_eq!(rx.try_recv().unwrap().chunk.as_bytes(), b"abcd");
        assert!(manager.ack(connection));
        assert_eq!(
            manager.enqueue_frame(connection, frame(b"next")),
            Err(EgressEnqueueError::QueueFull)
        );

        assert_eq!(expect_dispatch(&mut manager), connection);
        let final_dispatch = rx.try_recv().unwrap();
        assert_eq!(final_dispatch.chunk.as_bytes(), b"ef");
        assert!(final_dispatch.chunk.is_final_frame_chunk());
        assert!(manager.ack(connection));
        assert_eq!(manager.enqueue_frame(connection, frame(b"next")), Ok(1));
    }

    #[test]
    fn ack_without_final_in_flight_does_not_underflow_staged_frames() {
        let mut manager = manager_with_frame_limit(4, 1);
        let connection = conn(1);
        let (tx, mut rx) = channel();
        assert!(manager.register(user_registration(connection, 1, tx)));

        assert!(manager.ack(connection));

        assert_eq!(manager.enqueue_frame(connection, frame(b"abcd")), Ok(1));
        assert_eq!(expect_dispatch(&mut manager), connection);
        assert_eq!(rx.try_recv().unwrap().chunk.as_bytes(), b"abcd");
        assert!(manager.ack(connection));
        assert!(manager.ack(connection));
        assert_eq!(manager.enqueue_frame(connection, frame(b"next")), Ok(1));
    }

    #[test]
    fn dispatch_routes_chunk_to_registered_connection() {
        let mut manager = manager(4);
        let first = conn(1);
        let second = conn(2);
        let (first_tx, mut first_rx) = channel();
        let (second_tx, mut second_rx) = channel();
        assert!(manager.register(user_registration(first, 1, first_tx)));
        assert!(manager.register(user_registration(second, 2, second_tx)));

        assert_eq!(manager.enqueue_frame(second, frame(b"hello")), Ok(2));
        assert_eq!(
            manager.dispatch_next(),
            DispatchOutcome::Dispatched {
                connection_id: second,
                bytes: 4
            }
        );

        let dispatch = second_rx.try_recv().unwrap();
        assert_eq!(dispatch.connection_id, second);
        assert_eq!(dispatch.chunk.as_bytes(), b"hell");
        assert!(first_rx.try_recv().is_err());
        assert_eq!(manager.queued_packets(second), Some(1));
    }

    #[test]
    fn dispatch_and_ack_reassembles_full_frame() {
        let mut manager = manager(4);
        let connection = conn(1);
        let (tx, mut rx) = channel();
        assert!(manager.register(user_registration(connection, 1, tx)));
        let original = b"hello world".to_vec();
        assert_eq!(
            manager.enqueue_frame(connection, Arc::from(&original[..])),
            Ok(3)
        );

        let mut reassembled = Vec::new();
        for _ in 0..3 {
            let DispatchOutcome::Dispatched { connection_id, .. } = manager.dispatch_next() else {
                panic!("expected dispatch");
            };
            assert_eq!(connection_id, connection);
            reassembled.extend_from_slice(rx.try_recv().unwrap().chunk.as_bytes());
            assert!(manager.ack(connection));
        }

        assert_eq!(reassembled, original);
        assert_eq!(manager.dispatch_next(), DispatchOutcome::Empty);
    }

    #[test]
    fn stage_server_message_dispatches_wire_identical_chunks() {
        let mut manager = manager(512);
        let connection = conn(1);
        let (tx, mut rx) = channel();
        assert!(manager.register(user_registration(connection, 1, tx)));
        let message = large_chat_message();
        let message_id = message_id(b"000000000101");
        let expected = server_message_to_frame_bytes(&message, message_id).unwrap();

        let chunks = stage_server_message(&mut manager, connection, &message, message_id).unwrap();
        assert!(chunks > 1);

        let mut reassembled = Vec::new();
        for _ in 0..chunks {
            assert_eq!(expect_dispatch(&mut manager), connection);
            reassembled.extend_from_slice(rx.try_recv().unwrap().chunk.as_bytes());
            assert!(manager.ack(connection));
        }

        assert_eq!(reassembled.as_slice(), expected.as_ref());
        assert_eq!(manager.dispatch_next(), DispatchOutcome::Empty);
    }

    #[test]
    fn stage_server_message_counts_multi_chunk_message_as_one_frame() {
        let mut manager = manager_with_frame_limit(512, 1);
        let connection = conn(1);
        let (tx, _rx) = channel();
        assert!(manager.register(user_registration(connection, 1, tx)));
        let message = large_chat_message();

        let chunks = stage_server_message(
            &mut manager,
            connection,
            &message,
            message_id(b"000000000102"),
        )
        .unwrap();

        assert!(chunks > 1);
        assert!(matches!(
            stage_server_message(
                &mut manager,
                connection,
                &ServerMessage::Pong,
                message_id(b"000000000106"),
            ),
            Err(StagingError::Enqueue(EgressEnqueueError::QueueFull))
        ));
    }

    #[test]
    fn stage_server_message_preserves_queue_full_error() {
        let mut manager = manager_with_frame_limit(512, 1);
        let connection = conn(1);
        let (tx, _rx) = channel();
        assert!(manager.register(user_registration(connection, 1, tx)));
        assert!(matches!(
            stage_server_message(
                &mut manager,
                connection,
                &ServerMessage::Pong,
                message_id(b"000000000103"),
            ),
            Ok(1)
        ));

        let result = stage_server_message(
            &mut manager,
            connection,
            &ServerMessage::Pong,
            message_id(b"000000000104"),
        );

        assert!(matches!(
            result,
            Err(StagingError::Enqueue(EgressEnqueueError::QueueFull))
        ));
        assert_eq!(manager.queued_packets(connection), Some(1));
    }

    #[test]
    fn stage_server_message_preserves_unknown_connection_error() {
        let mut manager = manager(512);

        let result = stage_server_message(
            &mut manager,
            conn(999),
            &ServerMessage::Pong,
            message_id(b"000000000105"),
        );

        assert!(matches!(
            result,
            Err(StagingError::Enqueue(EgressEnqueueError::UnknownConnection))
        ));
    }

    #[test]
    fn dispatch_and_ack_preserves_fairness_across_connections() {
        let mut manager = manager(4);
        let first = conn(1);
        let second = conn(2);
        let (first_tx, mut first_rx) = channel();
        let (second_tx, mut second_rx) = channel();
        assert!(manager.register(user_registration(first, 1, first_tx)));
        assert!(manager.register(user_registration(second, 2, second_tx)));
        assert_eq!(
            manager.enqueue_frame(first, Arc::from(vec![b'a'; 40])),
            Ok(10)
        );
        assert_eq!(
            manager.enqueue_frame(second, Arc::from(vec![b'b'; 40])),
            Ok(10)
        );

        let mut order = Vec::new();
        for _ in 0..20 {
            let DispatchOutcome::Dispatched { connection_id, .. } = manager.dispatch_next() else {
                panic!("expected dispatch");
            };
            order.push(connection_id);
            if connection_id == first {
                assert_eq!(first_rx.try_recv().unwrap().chunk.as_bytes(), b"aaaa");
            } else {
                assert_eq!(connection_id, second);
                assert_eq!(second_rx.try_recv().unwrap().chunk.as_bytes(), b"bbbb");
            }
            assert!(manager.ack(connection_id));
        }

        assert_eq!(order.iter().filter(|&&id| id == first).count(), 10);
        assert_eq!(order.iter().filter(|&&id| id == second).count(), 10);
        assert!(order.windows(2).all(|window| window[0] != window[1]));
        assert_eq!(manager.dispatch_next(), DispatchOutcome::Empty);
    }

    #[test]
    fn protocol_class_drains_before_bulk_within_same_user_flow() {
        let mut manager = manager(4);
        let protocol = conn(1);
        let bulk = conn(2);
        let (protocol_tx, mut protocol_rx) = channel();
        let (bulk_tx, mut bulk_rx) = channel();
        assert!(manager.register(user_registration_with_class_and_weight(
            protocol,
            42,
            ConnectionClass::Protocol,
            1,
            protocol_tx,
        )));
        assert!(manager.register(user_registration_with_class_and_weight(
            bulk,
            42,
            ConnectionClass::Bulk,
            1,
            bulk_tx,
        )));
        assert_eq!(
            manager.enqueue_frame(bulk, Arc::from(vec![b'b'; 12])),
            Ok(3)
        );
        assert_eq!(
            manager.enqueue_frame(protocol, Arc::from(vec![b'p'; 12])),
            Ok(3)
        );

        for _ in 0..3 {
            assert_eq!(expect_dispatch(&mut manager), protocol);
            assert_eq!(protocol_rx.try_recv().unwrap().chunk.as_bytes(), b"pppp");
            assert!(manager.ack(protocol));
        }

        for _ in 0..3 {
            assert_eq!(expect_dispatch(&mut manager), bulk);
            assert_eq!(bulk_rx.try_recv().unwrap().chunk.as_bytes(), b"bbbb");
            assert!(manager.ack(bulk));
        }

        assert_eq!(manager.dispatch_next(), DispatchOutcome::Empty);
    }

    #[test]
    fn mixed_protocol_and_bulk_connections_do_not_multiply_user_flow_share() {
        let mut manager = manager(4);
        let user_protocol = conn(1);
        let user_bulk = conn(2);
        let other_user = conn(3);
        let (protocol_tx, mut protocol_rx) = channel();
        let (bulk_tx, mut bulk_rx) = channel();
        let (other_tx, mut other_rx) = channel();
        assert!(manager.register(user_registration_with_class_and_weight(
            user_protocol,
            42,
            ConnectionClass::Protocol,
            1,
            protocol_tx,
        )));
        assert!(manager.register(user_registration_with_class_and_weight(
            user_bulk,
            42,
            ConnectionClass::Bulk,
            1,
            bulk_tx,
        )));
        assert!(manager.register(user_registration_with_class_and_weight(
            other_user,
            7,
            ConnectionClass::Protocol,
            1,
            other_tx,
        )));

        assert_eq!(
            manager.enqueue_frame(user_protocol, Arc::from(vec![b'p'; 120])),
            Ok(30)
        );
        assert_eq!(
            manager.enqueue_frame(user_bulk, Arc::from(vec![b'b'; 800])),
            Ok(200)
        );
        assert_eq!(
            manager.enqueue_frame(other_user, Arc::from(vec![b'o'; 920])),
            Ok(230)
        );

        let mut user_protocol_count = 0usize;
        let mut user_bulk_count = 0usize;
        let mut other_user_count = 0usize;
        let mut saw_user_bulk = false;
        for _ in 0..120 {
            let connection_id = expect_dispatch(&mut manager);
            if connection_id == user_protocol {
                assert!(!saw_user_bulk);
                user_protocol_count += 1;
                assert_eq!(protocol_rx.try_recv().unwrap().chunk.as_bytes(), b"pppp");
            } else if connection_id == user_bulk {
                saw_user_bulk = true;
                user_bulk_count += 1;
                assert_eq!(bulk_rx.try_recv().unwrap().chunk.as_bytes(), b"bbbb");
            } else {
                assert_eq!(connection_id, other_user);
                other_user_count += 1;
                assert_eq!(other_rx.try_recv().unwrap().chunk.as_bytes(), b"oooo");
            }
            assert!(manager.ack(connection_id));
        }

        let combined_user_count = user_protocol_count + user_bulk_count;
        assert_eq!(user_protocol_count, 30);
        assert!(user_bulk_count > 0);
        assert!(combined_user_count.abs_diff(other_user_count) <= 2);
    }

    #[test]
    fn shared_account_protocol_and_bulk_sessions_share_one_user_flow() {
        let mut manager = manager(4);
        let shared_protocol = conn(1);
        let shared_bulk_first = conn(2);
        let shared_bulk_second = conn(3);
        let other_user = conn(4);
        let (protocol_tx, mut protocol_rx) = channel();
        let (bulk_first_tx, mut bulk_first_rx) = channel();
        let (bulk_second_tx, mut bulk_second_rx) = channel();
        let (other_tx, mut other_rx) = channel();
        let shared_user_id = 99;
        assert!(manager.register(user_registration_with_class_and_weight(
            shared_protocol,
            shared_user_id,
            ConnectionClass::Protocol,
            1,
            protocol_tx,
        )));
        assert!(manager.register(user_registration_with_class_and_weight(
            shared_bulk_first,
            shared_user_id,
            ConnectionClass::Bulk,
            1,
            bulk_first_tx,
        )));
        assert!(manager.register(user_registration_with_class_and_weight(
            shared_bulk_second,
            shared_user_id,
            ConnectionClass::Bulk,
            1,
            bulk_second_tx,
        )));
        assert!(manager.register(user_registration_with_class_and_weight(
            other_user,
            7,
            ConnectionClass::Protocol,
            1,
            other_tx,
        )));

        assert_eq!(
            manager.enqueue_frame(shared_protocol, Arc::from(vec![b'p'; 40])),
            Ok(10)
        );
        assert_eq!(
            manager.enqueue_frame(shared_bulk_first, Arc::from(vec![b'a'; 400])),
            Ok(100)
        );
        assert_eq!(
            manager.enqueue_frame(shared_bulk_second, Arc::from(vec![b'b'; 400])),
            Ok(100)
        );
        assert_eq!(
            manager.enqueue_frame(other_user, Arc::from(vec![b'o'; 840])),
            Ok(210)
        );

        let mut shared_count = 0usize;
        let mut other_count = 0usize;
        let mut protocol_count = 0usize;
        let mut bulk_count = 0usize;
        for _ in 0..120 {
            let connection_id = expect_dispatch(&mut manager);
            if connection_id == shared_protocol {
                protocol_count += 1;
                shared_count += 1;
                assert_eq!(protocol_rx.try_recv().unwrap().chunk.as_bytes(), b"pppp");
            } else if connection_id == shared_bulk_first {
                bulk_count += 1;
                shared_count += 1;
                assert_eq!(bulk_first_rx.try_recv().unwrap().chunk.as_bytes(), b"aaaa");
            } else if connection_id == shared_bulk_second {
                bulk_count += 1;
                shared_count += 1;
                assert_eq!(bulk_second_rx.try_recv().unwrap().chunk.as_bytes(), b"bbbb");
            } else {
                assert_eq!(connection_id, other_user);
                other_count += 1;
                assert_eq!(other_rx.try_recv().unwrap().chunk.as_bytes(), b"oooo");
            }
            assert!(manager.ack(connection_id));
        }

        assert_eq!(protocol_count, 10);
        assert!(bulk_count > 0);
        assert!(shared_count.abs_diff(other_count) <= 2);
    }

    #[test]
    fn streaming_bulk_bbs_priority_and_peer_flow_compose_without_share_multiplication() {
        let mut manager = manager_with_frame_limit(4, 1);
        let transfer = conn(1);
        let bbs = conn(2);
        let other_user = conn(3);
        let (transfer_tx, mut transfer_rx) = channel();
        let (bbs_tx, mut bbs_rx) = channel();
        let (other_tx, mut other_rx) = channel();
        let user_id = 42;
        assert!(manager.register(user_registration_with_class_and_weight(
            transfer,
            user_id,
            ConnectionClass::Bulk,
            1,
            transfer_tx,
        )));
        assert!(manager.register(user_registration_with_class_and_weight(
            bbs,
            user_id,
            ConnectionClass::Protocol,
            1,
            bbs_tx,
        )));
        assert!(manager.register(user_registration_with_class_and_weight(
            other_user,
            7,
            ConnectionClass::Protocol,
            1,
            other_tx,
        )));

        let stream_header = frame(b"NX|8|FileData|000000000301|12|");
        let stream_payload = frame(b"abcdefghijkl");
        let stream_chunks = manager
            .begin_stream_frame(transfer, Arc::clone(&stream_header))
            .unwrap()
            + manager
                .stage_stream_chunk(transfer, Arc::clone(&stream_payload))
                .unwrap()
            + manager.finish_stream_frame(transfer).unwrap();
        assert_eq!(stream_chunks, 12);

        assert_eq!(manager.enqueue_priority_frame(bbs, frame(b"pong")), Ok(1));
        let user_chunks = stream_chunks + 1;
        assert_eq!(
            manager.enqueue_frame(other_user, Arc::from(vec![b'o'; user_chunks * 4])),
            Ok(user_chunks)
        );

        let mut saw_bbs_priority_before_bulk = false;
        let mut transfer_bytes = Vec::new();
        let mut user_count = 0usize;
        let mut other_count = 0usize;
        for _ in 0..(user_chunks * 2) {
            let connection_id = expect_dispatch(&mut manager);
            if connection_id == bbs {
                saw_bbs_priority_before_bulk = true;
                user_count += 1;
                assert_eq!(bbs_rx.try_recv().unwrap().chunk.as_bytes(), b"pong");
            } else if connection_id == transfer {
                assert!(
                    saw_bbs_priority_before_bulk,
                    "BBS priority traffic must drain before same-user bulk stream chunks"
                );
                user_count += 1;
                transfer_bytes.extend_from_slice(transfer_rx.try_recv().unwrap().chunk.as_bytes());
            } else {
                assert_eq!(connection_id, other_user);
                other_count += 1;
                assert_eq!(other_rx.try_recv().unwrap().chunk.as_bytes(), b"oooo");
            }
            assert!(manager.ack(connection_id));
            assert!(
                user_count.abs_diff(other_count) <= 2,
                "same-user BBS+bulk flow must not multiply share against a peer flow; running split was {user_count}:{other_count}"
            );
        }

        assert!(saw_bbs_priority_before_bulk);
        assert_eq!(user_count, user_chunks);
        assert_eq!(other_count, user_chunks);
        let mut expected_stream = Vec::new();
        expected_stream.extend_from_slice(stream_header.as_ref());
        expected_stream.extend_from_slice(stream_payload.as_ref());
        expected_stream.push(b'\n');
        assert_eq!(transfer_bytes, expected_stream);
        assert_eq!(manager.dispatch_next(), DispatchOutcome::Empty);

        assert_eq!(manager.enqueue_frame(transfer, frame(b"done")), Ok(1));
        assert_eq!(expect_dispatch(&mut manager), transfer);
        assert_eq!(transfer_rx.try_recv().unwrap().chunk.as_bytes(), b"done");
        assert!(manager.ack(transfer));
        assert_eq!(manager.dispatch_next(), DispatchOutcome::Empty);
    }

    #[test]
    fn bulk_connections_receive_weighted_share_across_users() {
        let mut manager = manager(4);
        let heavy = conn(1);
        let light = conn(2);
        let (heavy_tx, mut heavy_rx) = channel();
        let (light_tx, mut light_rx) = channel();
        assert!(manager.register(user_registration_with_class_and_weight(
            heavy,
            1,
            ConnectionClass::Bulk,
            3,
            heavy_tx,
        )));
        assert!(manager.register(user_registration_with_class_and_weight(
            light,
            2,
            ConnectionClass::Bulk,
            1,
            light_tx,
        )));
        assert_eq!(
            manager.enqueue_frame(heavy, Arc::from(vec![b'h'; 800])),
            Ok(200)
        );
        assert_eq!(
            manager.enqueue_frame(light, Arc::from(vec![b'l'; 800])),
            Ok(200)
        );

        let mut heavy_count = 0usize;
        let mut light_count = 0usize;
        for _ in 0..160 {
            let connection_id = expect_dispatch(&mut manager);
            if connection_id == heavy {
                heavy_count += 1;
                assert_eq!(heavy_rx.try_recv().unwrap().chunk.as_bytes(), b"hhhh");
            } else {
                assert_eq!(connection_id, light);
                light_count += 1;
                assert_eq!(light_rx.try_recv().unwrap().chunk.as_bytes(), b"llll");
            }
            assert!(manager.ack(connection_id));
        }

        assert!(
            (112..=128).contains(&heavy_count),
            "expected heavy bulk flow near 3:1 share, got {heavy_count}:{light_count}"
        );
        assert!(
            (32..=48).contains(&light_count),
            "expected light bulk flow near 3:1 share, got {heavy_count}:{light_count}"
        );
    }

    #[test]
    fn transition_to_user_preserves_queued_chunks() {
        let mut manager = manager(4);
        let connection = conn(1);
        let (tx, mut rx) = channel();
        assert!(manager.register_anon(connection, ConnectionClass::Protocol, tx));
        assert_eq!(manager.enqueue_frame(connection, frame(b"abcdef")), Ok(2));
        assert_eq!(manager.queued_packets(connection), Some(2));
        assert!(!manager.update_user_weight(42, 1));

        assert!(manager.transition_to_user(connection, 42, 1));
        assert_eq!(manager.queued_packets(connection), Some(2));
        assert!(manager.update_user_weight(42, 1));

        assert_eq!(expect_dispatch(&mut manager), connection);
        assert_eq!(rx.try_recv().unwrap().chunk.as_bytes(), b"abcd");
        assert!(manager.ack(connection));
    }

    #[test]
    fn transition_to_user_while_in_flight_resumes_after_ack() {
        let mut manager = manager(4);
        let connection = conn(1);
        let (tx, mut rx) = channel();
        assert!(manager.register_anon(connection, ConnectionClass::Protocol, tx));
        assert_eq!(manager.enqueue_frame(connection, frame(b"abcdef")), Ok(2));

        assert_eq!(expect_dispatch(&mut manager), connection);
        assert_eq!(rx.try_recv().unwrap().chunk.as_bytes(), b"abcd");
        assert!(manager.transition_to_user(connection, 42, 5));
        assert!(manager.update_user_weight(42, 5));
        assert_eq!(manager.dispatch_next(), DispatchOutcome::Empty);

        assert!(manager.ack(connection));
        assert_eq!(expect_dispatch(&mut manager), connection);
        assert_eq!(rx.try_recv().unwrap().chunk.as_bytes(), b"ef");
        assert!(manager.ack(connection));
        assert_eq!(manager.enqueue_frame(connection, frame(b"next")), Ok(1));
    }

    #[test]
    fn transition_to_user_while_final_chunk_in_flight_frees_capacity_on_ack() {
        let mut manager = manager_with_frame_limit(4, 1);
        let connection = conn(1);
        let (tx, mut rx) = channel();
        assert!(manager.register_anon(connection, ConnectionClass::Protocol, tx));
        assert_eq!(manager.enqueue_frame(connection, frame(b"abcd")), Ok(1));

        assert_eq!(expect_dispatch(&mut manager), connection);
        let dispatch = rx.try_recv().unwrap();
        assert!(dispatch.chunk.is_final_frame_chunk());
        assert!(manager.transition_to_user(connection, 42, 5));

        assert!(manager.ack(connection));
        assert_eq!(manager.enqueue_frame(connection, frame(b"next")), Ok(1));
    }

    #[test]
    fn blocked_connection_does_not_dispatch_until_unblocked() {
        let mut manager = manager(4);
        let connection = conn(1);
        let (tx, mut rx) = channel();
        assert!(manager.register(user_registration(connection, 1, tx)));
        assert_eq!(manager.enqueue_frame(connection, frame(b"abcd")), Ok(1));

        assert!(manager.set_blocked(connection, true));
        assert_eq!(manager.dispatch_next(), DispatchOutcome::Empty);

        assert!(manager.set_blocked(connection, false));
        assert_eq!(expect_dispatch(&mut manager), connection);
        assert_eq!(rx.try_recv().unwrap().chunk.as_bytes(), b"abcd");
    }

    #[test]
    fn blocking_in_flight_connection_allows_current_chunk_then_stops_next() {
        let mut manager = manager(4);
        let connection = conn(1);
        let (tx, mut rx) = channel();
        assert!(manager.register(user_registration(connection, 1, tx)));
        assert_eq!(manager.enqueue_frame(connection, frame(b"abcdef")), Ok(2));

        assert_eq!(expect_dispatch(&mut manager), connection);
        assert_eq!(rx.try_recv().unwrap().chunk.as_bytes(), b"abcd");
        assert!(manager.set_blocked(connection, true));
        assert!(manager.ack(connection));
        assert_eq!(manager.dispatch_next(), DispatchOutcome::Empty);

        assert!(manager.set_blocked(connection, false));
        assert_eq!(expect_dispatch(&mut manager), connection);
        assert_eq!(rx.try_recv().unwrap().chunk.as_bytes(), b"ef");
    }

    #[test]
    fn blocked_then_unblocked_connection_does_not_receive_catchup_burst() {
        let mut manager = manager(4);
        let blocked = conn(1);
        let active = conn(2);
        let (blocked_tx, mut blocked_rx) = channel();
        let (active_tx, mut active_rx) = channel();
        assert!(manager.register(user_registration(blocked, 1, blocked_tx)));
        assert!(manager.register(user_registration(active, 2, active_tx)));
        assert_eq!(
            manager.enqueue_frame(blocked, Arc::from(vec![b'b'; 80])),
            Ok(20)
        );
        assert_eq!(
            manager.enqueue_frame(active, Arc::from(vec![b'a'; 80])),
            Ok(20)
        );
        assert!(manager.set_blocked(blocked, true));

        for _ in 0..5 {
            assert_eq!(expect_dispatch(&mut manager), active);
            assert_eq!(active_rx.try_recv().unwrap().chunk.as_bytes(), b"aaaa");
            assert!(manager.ack(active));
        }

        assert!(manager.set_blocked(blocked, false));
        let mut order = Vec::new();
        for _ in 0..4 {
            let connection_id = expect_dispatch(&mut manager);
            order.push(connection_id);
            if connection_id == blocked {
                assert_eq!(blocked_rx.try_recv().unwrap().chunk.as_bytes(), b"bbbb");
            } else {
                assert_eq!(connection_id, active);
                assert_eq!(active_rx.try_recv().unwrap().chunk.as_bytes(), b"aaaa");
            }
            assert!(manager.ack(connection_id));
        }

        assert_eq!(order.iter().filter(|&&id| id == blocked).count(), 2);
        assert_eq!(order.iter().filter(|&&id| id == active).count(), 2);
        assert!(order.windows(2).all(|window| window != [blocked, blocked]));
    }

    #[test]
    fn blocked_connection_stages_to_limit_then_returns_queue_full() {
        let mut manager = manager_with_frame_limit(4, 2);
        let connection = conn(1);
        let (tx, _rx) = channel();
        assert!(manager.register(user_registration(connection, 1, tx)));
        assert!(manager.set_blocked(connection, true));

        assert_eq!(manager.enqueue_frame(connection, frame(b"one")), Ok(1));
        assert_eq!(manager.enqueue_frame(connection, frame(b"two")), Ok(1));
        assert_eq!(
            manager.enqueue_frame(connection, frame(b"three")),
            Err(EgressEnqueueError::QueueFull)
        );
        assert_eq!(manager.dispatch_next(), DispatchOutcome::Empty);
    }

    #[test]
    fn same_user_weight_update_shifts_manager_fairness() {
        let mut manager = manager(4);
        let first_user_first = conn(1);
        let first_user_second = conn(2);
        let other_user = conn(3);
        let (first_tx, mut first_rx) = channel();
        let (second_tx, mut second_rx) = channel();
        let (other_tx, mut other_rx) = channel();
        assert!(manager.register(user_registration(first_user_first, 1, first_tx)));
        assert!(manager.register(user_registration(first_user_second, 1, second_tx)));
        assert!(manager.register(user_registration(other_user, 2, other_tx)));
        assert_eq!(
            manager.enqueue_frame(first_user_first, Arc::from(vec![b'a'; 400])),
            Ok(100)
        );
        assert_eq!(
            manager.enqueue_frame(first_user_second, Arc::from(vec![b'b'; 400])),
            Ok(100)
        );
        assert_eq!(
            manager.enqueue_frame(other_user, Arc::from(vec![b'o'; 400])),
            Ok(100)
        );

        assert!(manager.update_user_weight(1, 10));

        let mut first_user_total: usize = 0;
        let mut other_user_total: usize = 0;
        for _ in 0..110 {
            let connection_id = expect_dispatch(&mut manager);
            if connection_id == first_user_first {
                first_user_total += 1;
                assert_eq!(first_rx.try_recv().unwrap().chunk.as_bytes(), b"aaaa");
            } else if connection_id == first_user_second {
                first_user_total += 1;
                assert_eq!(second_rx.try_recv().unwrap().chunk.as_bytes(), b"bbbb");
            } else {
                assert_eq!(connection_id, other_user);
                other_user_total += 1;
                assert_eq!(other_rx.try_recv().unwrap().chunk.as_bytes(), b"oooo");
            }
            assert!(manager.ack(connection_id));
        }

        assert!(first_user_total >= 90);
        assert!(other_user_total <= 20);
    }

    #[test]
    fn ack_clears_in_flight_and_allows_next_chunk() {
        let mut manager = manager(4);
        let connection = conn(1);
        let (tx, mut rx) = channel();
        assert!(manager.register(user_registration(connection, 1, tx)));
        assert_eq!(manager.enqueue_frame(connection, frame(b"abcdef")), Ok(2));

        assert_eq!(
            manager.dispatch_next(),
            DispatchOutcome::Dispatched {
                connection_id: connection,
                bytes: 4
            }
        );
        assert_eq!(rx.try_recv().unwrap().chunk.as_bytes(), b"abcd");
        assert_eq!(manager.dispatch_next(), DispatchOutcome::Empty);

        assert!(manager.ack(connection));
        assert_eq!(
            manager.dispatch_next(),
            DispatchOutcome::Dispatched {
                connection_id: connection,
                bytes: 2
            }
        );
        assert_eq!(rx.try_recv().unwrap().chunk.as_bytes(), b"ef");
    }

    #[test]
    fn unregister_while_in_flight_is_harmless() {
        let mut manager = manager(4);
        let connection = conn(1);
        let (tx, mut rx) = channel();
        assert!(manager.register(user_registration(connection, 1, tx)));
        assert_eq!(manager.enqueue_frame(connection, frame(b"abcdef")), Ok(2));

        assert_eq!(
            manager.dispatch_next(),
            DispatchOutcome::Dispatched {
                connection_id: connection,
                bytes: 4
            }
        );
        assert_eq!(rx.try_recv().unwrap().chunk.as_bytes(), b"abcd");

        assert!(manager.unregister(connection));
        assert!(!manager.ack(connection));
        assert_eq!(manager.dispatch_next(), DispatchOutcome::Empty);
        assert!(!manager.has_connection(connection));
    }

    #[test]
    fn write_failure_unregisters_connection() {
        let mut manager = manager(4);
        let connection = conn(1);
        let (tx, mut rx) = channel();
        assert!(manager.register(user_registration(connection, 1, tx)));
        assert_eq!(manager.enqueue_frame(connection, frame(b"abcdef")), Ok(2));

        assert_eq!(
            manager.dispatch_next(),
            DispatchOutcome::Dispatched {
                connection_id: connection,
                bytes: 4
            }
        );
        assert_eq!(rx.try_recv().unwrap().chunk.as_bytes(), b"abcd");

        assert!(manager.write_failed(connection));
        assert!(!manager.ack(connection));
        assert!(!manager.has_connection(connection));
        assert_eq!(manager.queued_packets(connection), None);
        assert_eq!(manager.dispatch_next(), DispatchOutcome::Empty);
    }

    #[test]
    fn failed_dispatch_unregisters_connection_instead_of_leaving_it_in_flight() {
        let mut manager = manager(4);
        let connection = conn(1);
        let (tx, rx) = channel();
        assert!(manager.register(user_registration(connection, 1, tx)));
        assert_eq!(manager.enqueue_frame(connection, frame(b"abcdef")), Ok(2));
        drop(rx);

        assert_eq!(
            manager.dispatch_next(),
            DispatchOutcome::Unregistered {
                connection_id: connection
            }
        );
        assert!(!manager.has_connection(connection));
        assert_eq!(manager.queued_packets(connection), None);
        assert!(!manager.ack(connection));

        let (new_tx, _new_rx) = channel();
        assert!(manager.register(user_registration(connection, 1, new_tx)));
        assert_eq!(manager.enqueue_frame(connection, frame(b"ok")), Ok(1));
    }
}
