//! Dark egress-manager skeleton for future BBS-port scheduling.
//!
//! This module is not wired to live connections yet. It owns the future
//! manager-side lifecycle around the WF2Q+ scheduler: registration, frame
//! chunking, dispatch, ack, and write-failure cleanup.

use std::{collections::HashMap, num::NonZeroUsize, sync::Arc};

use tokio::sync::mpsc;

use crate::scheduler::{
    ANON_FLOW_WEIGHT, ConnectionClass, ConnectionId, EnqueueError, FlowId, SchedulerPacket,
    Wf2qScheduler,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EgressChunk {
    frame: Arc<[u8]>,
    offset: usize,
    len: usize,
}

impl EgressChunk {
    pub fn new(frame: Arc<[u8]>, offset: usize, len: usize) -> Option<Self> {
        if len == 0 {
            return None;
        }

        let end = offset.checked_add(len)?;
        if end > frame.len() {
            return None;
        }

        Some(Self { frame, offset, len })
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
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DispatchOutcome {
    Dispatched { connection_id: ConnectionId },
    Unregistered { connection_id: ConnectionId },
    Empty,
}

pub struct EgressManager {
    scheduler: Wf2qScheduler<EgressChunk>,
    connections: HashMap<ConnectionId, EgressConnection>,
    chunk_size: NonZeroUsize,
}

impl EgressManager {
    pub fn new(chunk_size: NonZeroUsize) -> Self {
        Self {
            scheduler: Wf2qScheduler::new(),
            connections: HashMap::new(),
            chunk_size,
        }
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
        if frame.is_empty() {
            return Err(EgressEnqueueError::EmptyFrame);
        }

        if !self.connections.contains_key(&connection_id) {
            return Err(EgressEnqueueError::UnknownConnection);
        }

        let mut chunks = 0;
        let chunk_size = self.chunk_size.get();
        for offset in (0..frame.len()).step_by(chunk_size) {
            let len = chunk_size.min(frame.len() - offset);
            let Some(chunk) = EgressChunk::new(Arc::clone(&frame), offset, len) else {
                self.unregister(connection_id);
                return Err(EgressEnqueueError::EmptyFrame);
            };
            if let Err(err) = self
                .scheduler
                .enqueue(connection_id, SchedulerPacket::new(chunk.len(), chunk))
            {
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
        let Some(connection) = self.connections.get(&connection_id) else {
            self.scheduler.unregister_connection(connection_id);
            return DispatchOutcome::Unregistered { connection_id };
        };

        let dispatch = EgressDispatch {
            connection_id,
            chunk: packet.payload,
        };
        if connection.dispatch_tx.try_send(dispatch).is_err() {
            self.unregister(connection_id);
            return DispatchOutcome::Unregistered { connection_id };
        }

        DispatchOutcome::Dispatched { connection_id }
    }

    pub fn ack(&mut self, connection_id: ConnectionId) -> bool {
        self.scheduler
            .set_connection_in_flight(connection_id, false)
    }

    pub fn write_failed(&mut self, connection_id: ConnectionId) -> bool {
        self.unregister(connection_id)
    }

    pub fn queued_packets(&self, connection_id: ConnectionId) -> Option<usize> {
        self.scheduler.queued_packets(connection_id)
    }

    pub fn has_connection(&self, connection_id: ConnectionId) -> bool {
        self.connections.contains_key(&connection_id)
    }
}

struct EgressConnection {
    dispatch_tx: EgressDispatchTx,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn conn(id: u64) -> ConnectionId {
        ConnectionId::new(id)
    }

    fn manager(chunk_size: usize) -> EgressManager {
        EgressManager::new(NonZeroUsize::new(chunk_size).unwrap())
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
        EgressRegistration {
            connection_id,
            flow_id: FlowId::User(user_id),
            class: ConnectionClass::Protocol,
            weight: 1,
            dispatch_tx,
        }
    }

    fn expect_dispatch(manager: &mut EgressManager) -> ConnectionId {
        let DispatchOutcome::Dispatched { connection_id } = manager.dispatch_next() else {
            panic!("expected dispatch");
        };
        connection_id
    }

    #[test]
    fn chunk_borrows_shared_frame_storage() {
        let frame = frame(b"abcdef");
        let chunk = EgressChunk::new(Arc::clone(&frame), 2, 3).unwrap();

        assert_eq!(chunk.as_bytes(), b"cde");
        assert_eq!(chunk.len(), 3);
        assert_eq!(Arc::strong_count(&frame), 2);
        assert!(EgressChunk::new(Arc::clone(&frame), 6, 1).is_none());
        assert!(EgressChunk::new(frame, 0, 0).is_none());
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
                connection_id: second
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
            assert_eq!(
                manager.dispatch_next(),
                DispatchOutcome::Dispatched {
                    connection_id: connection
                }
            );
            reassembled.extend_from_slice(rx.try_recv().unwrap().chunk.as_bytes());
            assert!(manager.ack(connection));
        }

        assert_eq!(reassembled, original);
        assert_eq!(manager.dispatch_next(), DispatchOutcome::Empty);
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
            let DispatchOutcome::Dispatched { connection_id } = manager.dispatch_next() else {
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
                connection_id: connection
            }
        );
        assert_eq!(rx.try_recv().unwrap().chunk.as_bytes(), b"abcd");
        assert_eq!(manager.dispatch_next(), DispatchOutcome::Empty);

        assert!(manager.ack(connection));
        assert_eq!(
            manager.dispatch_next(),
            DispatchOutcome::Dispatched {
                connection_id: connection
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
                connection_id: connection
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
                connection_id: connection
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
    }
}
