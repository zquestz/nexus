//! User session representation for logged-in users

use std::collections::HashSet;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicU16, AtomicU64};
use std::sync::{Arc, LazyLock};
use std::time::Instant;

use nexus_common::framing::MessageId;
#[cfg(test)]
use nexus_common::framing::{DELIMITER, MAGIC, MSG_ID_LENGTH, TERMINATOR};
#[cfg(test)]
use nexus_common::io::server_message_type;
use nexus_common::names::fold_name;
use nexus_common::protocol::ServerMessage;
use tokio::sync::mpsc;

use crate::constants::{
    ERR_SYSTEM_TIME_BEFORE_EPOCH_CHECK_CLOCK, SESSION_CONTROL_QUEUE_CAPACITY,
    SESSION_MESSAGE_QUEUE_CAPACITY,
};
use crate::db::Permission;

/// Event delivered from server-side producers to a connection task.
#[derive(Debug)]
pub enum SessionEvent {
    Message(Box<ServerMessage>, Option<MessageId>),
    Frame(Arc<[u8]>),
    Disconnect,
    SlowClientDisconnect,
}

impl SessionEvent {
    pub fn message(message: ServerMessage, message_id: Option<MessageId>) -> Self {
        Self::Message(Box::new(message), message_id)
    }

    pub fn frame(frame: Arc<[u8]>) -> Self {
        Self::Frame(frame)
    }
}

#[cfg(test)]
impl SessionEvent {
    pub fn expect_message(self) -> (ServerMessage, Option<MessageId>) {
        match self {
            Self::Message(message, message_id) => (*message, message_id),
            Self::Frame(frame) => expect_message_from_frame(&frame),
            Self::Disconnect => panic!("expected session message, got disconnect event"),
            Self::SlowClientDisconnect => {
                panic!("expected session message, got slow-client disconnect event")
            }
        }
    }
}

#[cfg(test)]
fn expect_message_from_frame(frame: &[u8]) -> (ServerMessage, Option<MessageId>) {
    assert!(
        frame.starts_with(MAGIC),
        "test frame is missing magic bytes"
    );
    assert_eq!(
        frame.last(),
        Some(&TERMINATOR),
        "test frame is missing terminator"
    );

    let mut offset = MAGIC.len();
    let type_len_end = find_frame_delimiter(frame, offset);
    let type_len = parse_ascii_usize(&frame[offset..type_len_end], "type length");
    offset = type_len_end + 1;

    let type_end = offset
        .checked_add(type_len)
        .expect("test frame type length overflow");
    let message_type =
        std::str::from_utf8(&frame[offset..type_end]).expect("test frame type must be UTF-8");
    offset = type_end;
    assert_eq!(
        frame.get(offset),
        Some(&DELIMITER),
        "test frame type delimiter missing"
    );
    offset += 1;

    let message_id_end = offset
        .checked_add(MSG_ID_LENGTH)
        .expect("test frame message id length overflow");
    MessageId::from_bytes(&frame[offset..message_id_end]).expect("test frame message id");
    offset = message_id_end;
    assert_eq!(
        frame.get(offset),
        Some(&DELIMITER),
        "test frame message id delimiter missing"
    );
    offset += 1;

    let payload_len_end = find_frame_delimiter(frame, offset);
    let payload_len = parse_ascii_usize(&frame[offset..payload_len_end], "payload length");
    offset = payload_len_end + 1;

    let payload_end = offset
        .checked_add(payload_len)
        .expect("test frame payload length overflow");
    assert_eq!(
        payload_end + 1,
        frame.len(),
        "test frame payload length does not reach terminator"
    );

    let message: ServerMessage =
        serde_json::from_slice(&frame[offset..payload_end]).expect("test frame payload JSON");
    assert_eq!(
        message_type,
        server_message_type(&message),
        "test frame type does not match payload"
    );

    (message, None)
}

#[cfg(test)]
fn find_frame_delimiter(frame: &[u8], offset: usize) -> usize {
    frame[offset..]
        .iter()
        .position(|byte| *byte == DELIMITER)
        .map(|index| offset + index)
        .expect("test frame delimiter missing")
}

#[cfg(test)]
fn parse_ascii_usize(bytes: &[u8], field: &str) -> usize {
    std::str::from_utf8(bytes)
        .unwrap_or_else(|_| panic!("test frame {field} must be ASCII"))
        .parse()
        .unwrap_or_else(|_| panic!("test frame {field} must be numeric"))
}

/// Writer handle for events destined to a connection task.
#[derive(Clone, Debug)]
pub struct ConnectionWriter {
    tx: mpsc::Sender<SessionEvent>,
    control_tx: mpsc::Sender<SessionEvent>,
}

impl ConnectionWriter {
    pub fn channel() -> (Self, SessionRx) {
        let (tx, rx) = mpsc::channel(SESSION_MESSAGE_QUEUE_CAPACITY);
        let (control_tx, control_rx) = mpsc::channel(SESSION_CONTROL_QUEUE_CAPACITY);
        (Self { tx, control_tx }, SessionRx { rx, control_rx })
    }

    pub fn send_message(
        &self,
        message: ServerMessage,
        message_id: Option<MessageId>,
    ) -> Result<(), mpsc::error::SendError<SessionEvent>> {
        match self.tx.try_send(SessionEvent::message(message, message_id)) {
            Ok(()) => Ok(()),
            Err(mpsc::error::TrySendError::Full(_)) => {
                self.arm_slow_client_disconnect();
                Ok(())
            }
            Err(mpsc::error::TrySendError::Closed(event)) => Err(mpsc::error::SendError(event)),
        }
    }

    pub fn send_frame(&self, frame: Arc<[u8]>) -> Result<(), mpsc::error::SendError<SessionEvent>> {
        match self.tx.try_send(SessionEvent::frame(frame)) {
            Ok(()) => Ok(()),
            Err(mpsc::error::TrySendError::Full(_)) => {
                self.arm_slow_client_disconnect();
                Ok(())
            }
            Err(mpsc::error::TrySendError::Closed(event)) => Err(mpsc::error::SendError(event)),
        }
    }

    pub fn disconnect(&self) -> Result<(), mpsc::error::SendError<SessionEvent>> {
        match self.tx.try_send(SessionEvent::Disconnect) {
            Ok(()) => Ok(()),
            Err(mpsc::error::TrySendError::Full(_)) => {
                self.arm_slow_client_disconnect();
                Ok(())
            }
            Err(mpsc::error::TrySendError::Closed(event)) => Err(mpsc::error::SendError(event)),
        }
    }

    fn arm_slow_client_disconnect(&self) {
        let _ = self.control_tx.try_send(SessionEvent::SlowClientDisconnect);
    }

    #[cfg(test)]
    pub(crate) fn arm_slow_client_disconnect_for_test(&self) {
        self.arm_slow_client_disconnect();
    }
}

#[derive(Debug)]
pub struct SessionRx {
    rx: mpsc::Receiver<SessionEvent>,
    control_rx: mpsc::Receiver<SessionEvent>,
}

impl SessionRx {
    pub async fn recv(&mut self) -> Option<SessionEvent> {
        match self.try_recv() {
            Ok(event) => return Some(event),
            Err(mpsc::error::TryRecvError::Empty) => {}
            Err(mpsc::error::TryRecvError::Disconnected) => return None,
        }

        tokio::select! {
            biased;

            event = self.control_rx.recv() => {
                match event {
                    Some(event) => Some(event),
                    None => self.rx.recv().await,
                }
            }
            event = self.rx.recv() => event,
        }
    }

    pub fn try_recv(&mut self) -> Result<SessionEvent, mpsc::error::TryRecvError> {
        match self.control_rx.try_recv() {
            Ok(event) => Ok(event),
            Err(mpsc::error::TryRecvError::Empty)
            | Err(mpsc::error::TryRecvError::Disconnected) => self.rx.try_recv(),
        }
    }
}

#[cfg(test)]
mod connection_writer_tests {
    use super::*;

    #[tokio::test]
    async fn full_message_queue_arms_priority_slow_client_disconnect() {
        let (tx, mut rx) = ConnectionWriter::channel();

        for _ in 0..SESSION_MESSAGE_QUEUE_CAPACITY {
            tx.send_message(ServerMessage::Pong, None).unwrap();
        }

        tx.send_message(ServerMessage::Pong, None).unwrap();

        assert!(matches!(
            rx.recv().await,
            Some(SessionEvent::SlowClientDisconnect)
        ));
    }

    #[tokio::test]
    async fn slow_client_disconnect_control_queue_is_one_shot() {
        let (tx, mut rx) = ConnectionWriter::channel();

        for _ in 0..SESSION_MESSAGE_QUEUE_CAPACITY {
            tx.send_message(ServerMessage::Pong, None).unwrap();
        }

        tx.send_message(ServerMessage::Pong, None).unwrap();
        tx.send_message(ServerMessage::Pong, None).unwrap();

        assert!(matches!(
            rx.recv().await,
            Some(SessionEvent::SlowClientDisconnect)
        ));
        assert!(matches!(rx.recv().await, Some(SessionEvent::Message(_, _))));
    }

    #[tokio::test]
    async fn full_message_queue_arms_priority_slow_client_disconnect_for_frame() {
        let (tx, mut rx) = ConnectionWriter::channel();

        for _ in 0..SESSION_MESSAGE_QUEUE_CAPACITY {
            tx.send_frame(Arc::from(&b"frame"[..])).unwrap();
        }

        tx.send_frame(Arc::from(&b"overflow"[..])).unwrap();

        assert!(matches!(
            rx.recv().await,
            Some(SessionEvent::SlowClientDisconnect)
        ));
    }

    #[tokio::test]
    async fn test_hook_arms_slow_client_disconnect() {
        let (tx, mut rx) = ConnectionWriter::channel();

        tx.arm_slow_client_disconnect_for_test();

        assert!(matches!(
            rx.recv().await,
            Some(SessionEvent::SlowClientDisconnect)
        ));
    }

    #[test]
    fn closed_message_queue_returns_send_error() {
        let (tx, rx) = ConnectionWriter::channel();
        drop(rx);

        assert!(tx.send_message(ServerMessage::Pong, None).is_err());
    }
}

/// Parameters for creating a new user session
pub struct NewSessionParams {
    pub session_id: u32,
    pub user_id: i64,
    pub username: String,
    pub is_admin: bool,
    pub is_shared: bool,
    pub permissions: HashSet<Permission>,
    pub address: SocketAddr,
    pub created_at: i64,
    pub tx: ConnectionWriter,
    pub features: Vec<String>,
    pub locale: String,
    /// User's avatar as a data URI (ephemeral, not stored in DB)
    pub avatar: Option<String>,
    /// Display name (always populated; equals username for regular accounts)
    pub nickname: String,
    pub is_away: bool,
    /// Status message (used for both away messages and general status)
    pub status: Option<String>,
    pub group_id: Option<i64>,
    pub group_name: Option<String>,
    /// Resolved effective bandwidth weight at session creation.
    pub bandwidth_weight: u16,
    /// Per-user override column value (`None` if NULL). Group cascades skip
    /// sessions where this is `Some(_)` even if the value matches the group's.
    pub bandwidth_weight_override: Option<u16>,
    pub last_activity: std::time::Instant,
}

/// Reference instant for [`UserSession::last_activity`] stamps.
static PROCESS_START: LazyLock<Instant> = LazyLock::new(Instant::now);

/// Nanoseconds since process start, for the advisory activity clock.
/// Nanosecond scale preserves the ordering resolution the field had as an
/// `Instant` — "most recently active" picks between events in the same
/// millisecond must not tie. `u64` nanos covers ~584 years of uptime.
pub(crate) fn activity_now_nanos() -> u64 {
    PROCESS_START.elapsed().as_nanos() as u64
}

/// Convert an `Instant` (captured just before session construction) to the
/// nanos-since-process-start scale. Saturates to 0 for instants at or
/// before the reference — "active since startup", harmless for advisory
/// idle tracking.
fn instant_to_activity_nanos(instant: Instant) -> u64 {
    instant.saturating_duration_since(*PROCESS_START).as_nanos() as u64
}

/// Represents a logged-in user session
#[derive(Debug)]
pub struct UserSession {
    pub session_id: u32,
    pub user_id: i64,
    pub username: String,
    pub is_admin: bool,
    pub is_shared: bool,
    /// Cached effective permissions; admins bypass this set entirely.
    /// `Arc`: only ever replaced wholesale (login, permission cascades),
    /// so session clones share one set instead of deep-copying it on
    /// every handler entry.
    pub permissions: Arc<HashSet<Permission>>,
    pub address: SocketAddr,
    /// Account creation timestamp; reserved for future account-age/audit use.
    pub created_at: i64,
    pub login_time: i64,
    pub tx: ConnectionWriter,
    /// Negotiated client features; immutable after login (`Arc`: shared
    /// by session clones).
    pub features: Arc<[String]>,
    pub locale: String,
    /// User's avatar as a data URI (ephemeral, not stored in DB).
    /// `Arc<str>`: up to ~176 KB and set only at session construction —
    /// cloning a session must not copy it.
    pub avatar: Option<Arc<str>>,
    /// Display name (always populated; equals username for regular accounts)
    pub nickname: String,
    /// `fold_name(&nickname)`, cached so per-message identity comparisons
    /// (broadcast targeting, nickname lookups, flood keys) don't re-fold.
    /// Maintained wherever `nickname` is written: session construction and
    /// `UserManager::update_username`.
    pub nickname_folded: String,
    pub is_away: bool,
    /// Status message (used for both away messages and general status)
    pub status: Option<String>,
    pub group_id: Option<i64>,
    pub group_name: Option<String>,
    /// Live cache of this user's resolved effective bandwidth weight; read by
    /// the scheduler each dispatch (advisory fairness, not correctness, so
    /// `Relaxed` is fine). Multi-session invariant: every session of a `user_id`
    /// holds the same value, kept in lockstep by
    /// `UserManager::update_bandwidth_state`. The atomic is per-session, not
    /// shared — convergence is by value, so aggregating readers can use any one.
    pub bandwidth_weight: AtomicU16,
    /// Per-user override column value (`None` if NULL). Group cascades skip
    /// sessions where this is `Some(_)` even if the value matches the group's.
    pub bandwidth_weight_override: Option<u16>,
    /// Last meaningful activity in nanoseconds since process start (idle
    /// tracking; excludes passive messages like Ping/UserAway). Atomic so
    /// the per-message stamp runs under the users *read* lock — advisory
    /// accuracy, not correctness, so `Relaxed` is fine (same pattern as
    /// `bandwidth_weight`).
    pub last_activity: AtomicU64,
}

// Manual `Clone`: `AtomicU16` isn't `Clone`. The clone snapshots the current
// weight into a fresh, independent atomic — later `update_bandwidth_state`
// writes don't reach it. Matches caller intent (queries return snapshots; live
// updates flow through the manager lock).
impl Clone for UserSession {
    fn clone(&self) -> Self {
        Self {
            session_id: self.session_id,
            user_id: self.user_id,
            username: self.username.clone(),
            is_admin: self.is_admin,
            is_shared: self.is_shared,
            permissions: self.permissions.clone(),
            address: self.address,
            created_at: self.created_at,
            login_time: self.login_time,
            tx: self.tx.clone(),
            features: self.features.clone(),
            locale: self.locale.clone(),
            avatar: self.avatar.clone(),
            nickname: self.nickname.clone(),
            nickname_folded: self.nickname_folded.clone(),
            is_away: self.is_away,
            status: self.status.clone(),
            group_id: self.group_id,
            group_name: self.group_name.clone(),
            bandwidth_weight: AtomicU16::new(
                self.bandwidth_weight
                    .load(std::sync::atomic::Ordering::Relaxed),
            ),
            bandwidth_weight_override: self.bandwidth_weight_override,
            last_activity: AtomicU64::new(
                self.last_activity
                    .load(std::sync::atomic::Ordering::Relaxed),
            ),
        }
    }
}

impl UserSession {
    pub fn new(params: NewSessionParams) -> Self {
        let nickname_folded = fold_name(&params.nickname);
        Self {
            session_id: params.session_id,
            user_id: params.user_id,
            username: params.username,
            is_admin: params.is_admin,
            is_shared: params.is_shared,
            permissions: Arc::new(params.permissions),
            address: params.address,
            created_at: params.created_at,
            login_time: current_timestamp(),
            tx: params.tx,
            features: Arc::from(params.features),
            locale: params.locale,
            avatar: params.avatar.map(Arc::from),
            nickname: params.nickname,
            nickname_folded,
            is_away: params.is_away,
            status: params.status,
            group_id: params.group_id,
            group_name: params.group_name,
            bandwidth_weight: AtomicU16::new(params.bandwidth_weight),
            bandwidth_weight_override: params.bandwidth_weight_override,
            last_activity: AtomicU64::new(instant_to_activity_nanos(params.last_activity)),
        }
    }

    pub fn has_feature(&self, feature: &str) -> bool {
        self.features.iter().any(|f| f == feature)
    }

    /// Admins implicitly have every permission.
    pub fn has_permission(&self, permission: Permission) -> bool {
        if self.is_admin {
            true
        } else {
            self.permissions.contains(&permission)
        }
    }
}

/// Current Unix timestamp in seconds. Panics only if the system clock is set
/// before the Unix epoch.
fn current_timestamp() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect(ERR_SYSTEM_TIME_BEFORE_EPOCH_CHECK_CLOCK)
        .as_secs() as i64
}
