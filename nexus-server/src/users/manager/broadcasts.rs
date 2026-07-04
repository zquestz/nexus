//! Broadcast methods for UserManager

use std::sync::Arc;

use nexus_common::framing::MessageId;
use nexus_common::io::server_message_to_frame_bytes;
use nexus_common::names::fold_name;
use nexus_common::protocol::ServerMessage;
use tracing::error;

use crate::handlers::{ServerInfoOptions, ServerInfoValues, build_server_info};

use super::UserManager;
use crate::constants::{FEATURE_CHAT, LOG_BROADCAST_FRAME_SERIALIZE_FAILED};
use crate::db::Permission;
use crate::users::user::UserSession;

pub(super) fn shared_broadcast_frame(message: &ServerMessage) -> Option<Arc<[u8]>> {
    match server_message_to_frame_bytes(message, MessageId::new()) {
        Ok(frame) => Some(frame),
        Err(e) => {
            error!(err = ?e, "{}", LOG_BROADCAST_FRAME_SERIALIZE_FAILED);
            None
        }
    }
}

fn send_shared_frame(user: &UserSession, frame: &Arc<[u8]>, disconnected: &mut Vec<u32>) {
    if user.tx.send_frame(Arc::clone(frame)).is_err() {
        disconnected.push(user.session_id);
    }
}

#[derive(Default)]
enum CachedServerInfoFrame {
    #[default]
    Vacant,
    Failed,
    Ready(Arc<[u8]>),
}

impl CachedServerInfoFrame {
    fn get_or_build(
        &mut self,
        values: &ServerInfoValues,
        options: ServerInfoOptions,
    ) -> Option<&Arc<[u8]>> {
        if matches!(self, Self::Vacant) {
            *self = match server_info_updated_frame(values, &options) {
                Some(frame) => Self::Ready(frame),
                None => Self::Failed,
            };
        }

        match self {
            Self::Ready(frame) => Some(frame),
            Self::Vacant | Self::Failed => None,
        }
    }
}

struct ServerInfoUpdatedFrames<'a> {
    values: &'a ServerInfoValues,
    admin: CachedServerInfoFrame,
    non_admin: [CachedServerInfoFrame; 4],
}

impl<'a> ServerInfoUpdatedFrames<'a> {
    fn new(values: &'a ServerInfoValues) -> Self {
        Self {
            values,
            admin: CachedServerInfoFrame::default(),
            non_admin: std::array::from_fn(|_| CachedServerInfoFrame::default()),
        }
    }

    fn for_session(&mut self, user: &UserSession) -> Option<&Arc<[u8]>> {
        if user.is_admin {
            return self.admin.get_or_build(
                self.values,
                ServerInfoOptions {
                    is_admin: true,
                    has_file_reindex: false,
                    has_chat_join: false,
                    include_image: true,
                },
            );
        }

        let has_file_reindex = user.has_permission(Permission::FileReindex);
        let has_chat_join =
            user.has_feature(FEATURE_CHAT) && user.has_permission(Permission::ChatJoin);
        self.non_admin[server_info_variant_index(has_file_reindex, has_chat_join)].get_or_build(
            self.values,
            ServerInfoOptions {
                is_admin: false,
                has_file_reindex,
                has_chat_join,
                include_image: true,
            },
        )
    }
}

fn server_info_updated_frame(
    values: &ServerInfoValues,
    options: &ServerInfoOptions,
) -> Option<Arc<[u8]>> {
    let server_info = build_server_info(values, options);
    let message = ServerMessage::ServerInfoUpdated { server_info };
    shared_broadcast_frame(&message)
}

fn server_info_variant_index(has_file_reindex: bool, has_chat_join: bool) -> usize {
    usize::from(has_file_reindex) * 2 + usize::from(has_chat_join)
}

impl UserManager {
    /// Send a message to a specific session. Returns false if the session
    /// doesn't exist or the channel is closed.
    pub async fn send_to_session(&self, session_id: u32, message: ServerMessage) -> bool {
        let users = self.users.read().await;
        if let Some(user) = users.get(&session_id) {
            user.tx.send_message(message, None).is_ok()
        } else {
            false
        }
    }

    /// Broadcast to all connected users. Removes users whose channels have
    /// closed and notifies user_list clients of the disconnect.
    pub async fn broadcast(&self, message: ServerMessage) {
        let Some(frame) = shared_broadcast_frame(&message) else {
            return;
        };
        let mut disconnected = Vec::new();

        {
            let users = self.users.read().await;
            for user in users.values() {
                send_shared_frame(user, &frame, &mut disconnected);
            }
        }

        self.enqueue_dead_sessions(&disconnected);
    }

    /// Broadcast to users who both requested `feature` (client preference)
    /// and hold `required_permission` (server enforcement). Optionally excludes
    /// one session (e.g. the originator who already got a typed response).
    pub async fn broadcast_to_feature(
        &self,
        feature: &str,
        message: ServerMessage,
        required_permission: Permission,
        exclude_session_id: Option<u32>,
    ) {
        let Some(frame) = shared_broadcast_frame(&message) else {
            return;
        };
        let mut disconnected = Vec::new();

        {
            let users = self.users.read().await;
            for user in users.values() {
                if let Some(excluded) = exclude_session_id
                    && user.session_id == excluded
                {
                    continue;
                }

                if !user.has_feature(feature) {
                    continue;
                }

                // Cached permissions, admin bypass.
                if !user.has_permission(required_permission) {
                    continue;
                }

                send_shared_frame(user, &frame, &mut disconnected);
            }
        }

        self.enqueue_dead_sessions(&disconnected);
    }

    /// Broadcast a per-session message to every session of `user_id`. Use this
    /// when a shared payload contains feature-gated fields, because features are
    /// negotiated per connection.
    pub async fn broadcast_to_user_id_per_session<F>(&self, user_id: i64, mut build_message: F)
    where
        F: FnMut(&UserSession) -> ServerMessage,
    {
        let mut disconnected = Vec::new();

        {
            let users = self.users.read().await;
            for user in users.values() {
                if user.user_id == user_id {
                    let message = build_message(user);
                    if user.tx.send_message(message, None).is_err() {
                        disconnected.push(user.session_id);
                    }
                }
            }
        }

        self.enqueue_dead_sessions(&disconnected);
    }

    /// Broadcast to all sessions with `nickname` that activated `feature`.
    pub async fn broadcast_to_nickname_with_feature(
        &self,
        nickname: &str,
        feature: &str,
        message: &ServerMessage,
    ) {
        let Some(frame) = shared_broadcast_frame(message) else {
            return;
        };
        let mut disconnected = Vec::new();

        let nickname_lower = fold_name(nickname);

        {
            let users = self.users.read().await;
            for user in users.values() {
                if user.nickname_folded == nickname_lower && user.has_feature(feature) {
                    send_shared_frame(user, &frame, &mut disconnected);
                }
            }
        }

        self.enqueue_dead_sessions(&disconnected);
    }

    /// Broadcast a chat message to channel members who enabled chat and can
    /// receive chat. The caller supplies membership from ChannelManager so the
    /// user map is read once for permission/feature filtering.
    pub async fn broadcast_chat_message_to_members(
        &self,
        member_session_ids: &[u32],
        message: &ServerMessage,
    ) {
        let Some(frame) = shared_broadcast_frame(message) else {
            return;
        };
        let mut disconnected = Vec::new();

        {
            let users = self.users.read().await;
            for &member_session_id in member_session_ids {
                if let Some(member) = users.get(&member_session_id)
                    && member.has_feature(FEATURE_CHAT)
                    && member.has_permission(Permission::ChatReceive)
                {
                    send_shared_frame(member, &frame, &mut disconnected);
                }
            }
        }

        self.enqueue_dead_sessions(&disconnected);
    }

    /// Broadcast to all sessions with `nickname` that activated `feature` and
    /// hold `required_permission`.
    pub async fn broadcast_to_nickname_with_feature_and_permission(
        &self,
        nickname: &str,
        feature: &str,
        required_permission: Permission,
        message: &ServerMessage,
    ) {
        let Some(frame) = shared_broadcast_frame(message) else {
            return;
        };
        let mut disconnected = Vec::new();

        let nickname_lower = fold_name(nickname);

        {
            let users = self.users.read().await;
            for user in users.values() {
                if user.nickname_folded == nickname_lower
                    && user.has_feature(feature)
                    && user.has_permission(required_permission)
                {
                    send_shared_frame(user, &frame, &mut disconnected);
                }
            }
        }

        self.enqueue_dead_sessions(&disconnected);
    }

    /// Broadcast to all users holding `required_permission` (e.g. topic updates
    /// to those allowed to see them).
    pub async fn broadcast_to_permission(
        &self,
        message: ServerMessage,
        required_permission: Permission,
    ) {
        let Some(frame) = shared_broadcast_frame(&message) else {
            return;
        };
        let mut disconnected = Vec::new();

        {
            let users = self.users.read().await;
            for user in users.values() {
                // Cached permissions, admin bypass.
                if !user.has_permission(required_permission) {
                    continue;
                }

                send_shared_frame(user, &frame, &mut disconnected);
            }
        }

        self.enqueue_dead_sessions(&disconnected);
    }

    /// Broadcast a UserConnected/UserDisconnected event to user_list holders.
    /// Optionally excludes one session (e.g. the connecting user).
    pub async fn broadcast_user_event(
        &self,
        message: ServerMessage,
        exclude_session_id: Option<u32>,
    ) {
        let Some(frame) = shared_broadcast_frame(&message) else {
            return;
        };
        let mut disconnected = Vec::new();

        {
            let users = self.users.read().await;
            for user in users.values() {
                if let Some(excluded) = exclude_session_id
                    && user.session_id == excluded
                {
                    continue;
                }

                // Cached permissions, admin bypass.
                if !user.has_permission(Permission::UserList) {
                    continue;
                }

                send_shared_frame(user, &frame, &mut disconnected);
            }
        }

        self.enqueue_dead_sessions(&disconnected);
    }

    /// Broadcast ServerInfoUpdated to all users. Everyone gets full server info;
    /// gated fields are filtered per receiving session.
    pub async fn broadcast_server_info_updated(&self, values: ServerInfoValues) {
        let mut frames = ServerInfoUpdatedFrames::new(&values);
        let mut disconnected = Vec::new();

        {
            let users = self.users.read().await;
            for user in users.values() {
                let Some(frame) = frames.for_session(user) else {
                    continue;
                };

                send_shared_frame(user, frame, &mut disconnected);
            }
        }

        self.enqueue_dead_sessions(&disconnected);
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;
    use std::net::SocketAddr;

    use super::*;
    use crate::users::user::{ConnectionWriter, NewSessionParams, SessionEvent};
    use nexus_common::protocol::ChatAction;

    fn session_params(user_id: i64, username: &str, tx: ConnectionWriter) -> NewSessionParams {
        NewSessionParams {
            session_id: 0,
            user_id,
            username: username.to_string(),
            is_admin: false,
            is_shared: false,
            permissions: HashSet::new(),
            address: "127.0.0.1:12345".parse::<SocketAddr>().unwrap(),
            created_at: 0,
            tx,
            features: vec![],
            locale: "en".to_string(),
            avatar: None,
            nickname: username.to_string(),
            is_away: false,
            status: None,
            group_id: None,
            group_name: None,
            bandwidth_weight_override: None,
            last_activity: std::time::Instant::now(),
            bandwidth_weight: 1,
        }
    }

    fn server_info_values() -> ServerInfoValues {
        ServerInfoValues {
            name: "Nexus".to_string(),
            description: "Test server".to_string(),
            public_address: String::new(),
            version: "0.9.3".to_string(),
            image: String::new(),
            max_connections_per_ip: 0,
            max_transfers_per_ip: 0,
            transfer_port: 7501,
            transfer_websocket_port: None,
            file_reindex_interval: 0,
            persistent_channels: "#general".to_string(),
            auto_join_channels: "#general".to_string(),
            min_password_strength: 1,
            chat_burst_limit: 0,
            chat_rate_limit: 0,
            max_outbound_rate: 0,
            scheduler_chunk_size: 65_536,
        }
    }

    #[test]
    fn server_info_updated_frames_are_built_lazily() {
        let values = server_info_values();
        let mut frames = ServerInfoUpdatedFrames::new(&values);

        assert!(matches!(&frames.admin, CachedServerInfoFrame::Vacant));
        assert!(
            frames
                .non_admin
                .iter()
                .all(|slot| matches!(slot, CachedServerInfoFrame::Vacant))
        );

        let (tx_basic, _rx_basic) = ConnectionWriter::channel();
        let basic = UserSession::new(session_params(1, "basic", tx_basic));
        let basic_frame = Arc::clone(
            frames
                .for_session(&basic)
                .expect("basic visibility frame should build"),
        );

        assert!(matches!(&frames.admin, CachedServerInfoFrame::Vacant));
        assert!(matches!(
            &frames.non_admin[0],
            CachedServerInfoFrame::Ready(frame) if Arc::ptr_eq(frame, &basic_frame)
        ));
        assert!(
            frames.non_admin[1..]
                .iter()
                .all(|slot| matches!(slot, CachedServerInfoFrame::Vacant))
        );

        let (tx_same, _rx_same) = ConnectionWriter::channel();
        let same_visibility = UserSession::new(session_params(2, "same", tx_same));
        let same_frame = Arc::clone(
            frames
                .for_session(&same_visibility)
                .expect("cached basic visibility frame should be reused"),
        );
        assert!(Arc::ptr_eq(&basic_frame, &same_frame));

        let (tx_admin, _rx_admin) = ConnectionWriter::channel();
        let mut admin_params = session_params(3, "admin", tx_admin);
        admin_params.is_admin = true;
        let admin = UserSession::new(admin_params);
        let admin_frame = Arc::clone(
            frames
                .for_session(&admin)
                .expect("admin visibility frame should build"),
        );

        assert!(matches!(
            &frames.admin,
            CachedServerInfoFrame::Ready(frame) if Arc::ptr_eq(frame, &admin_frame)
        ));
        assert!(!Arc::ptr_eq(&basic_frame, &admin_frame));
    }

    #[tokio::test]
    async fn test_broadcast_sends_shared_preframed_event_to_each_session() {
        let manager = UserManager::new();

        let (tx_a, mut rx_a) = ConnectionWriter::channel();
        let (tx_b, mut rx_b) = ConnectionWriter::channel();
        manager
            .add_user(session_params(1, "alice", tx_a))
            .await
            .unwrap();
        manager
            .add_user(session_params(2, "bob", tx_b))
            .await
            .unwrap();

        manager.broadcast(ServerMessage::Pong).await;

        let frame_a = match rx_a.try_recv().expect("alice should receive broadcast") {
            SessionEvent::Frame(frame) => frame,
            other => panic!("expected preframed broadcast event, got {other:?}"),
        };
        let frame_b = match rx_b.try_recv().expect("bob should receive broadcast") {
            SessionEvent::Frame(frame) => frame,
            other => panic!("expected preframed broadcast event, got {other:?}"),
        };

        assert!(
            Arc::ptr_eq(&frame_a, &frame_b),
            "fixed-payload broadcasts should share one serialized frame"
        );
        assert!(rx_a.try_recv().is_err());
        assert!(rx_b.try_recv().is_err());
    }

    /// Pins the PK-keyed dispatch contract the rename-race fix depends on:
    /// `broadcast_to_user_id_per_session` hits every session of the target
    /// user_id only.
    #[tokio::test]
    async fn test_broadcast_to_user_id_per_session_hits_all_sessions_of_one_user_only() {
        let manager = UserManager::new();

        // user_id=1 has two sessions (fixture-only multi-session fan-out, not a
        // production state for a regular account); user_id=2 has one.
        let (tx_a1, mut rx_a1) = ConnectionWriter::channel();
        let (tx_a2, mut rx_a2) = ConnectionWriter::channel();
        let (tx_b, mut rx_b) = ConnectionWriter::channel();
        manager
            .add_user(session_params(1, "alice_one", tx_a1))
            .await
            .unwrap();
        manager
            .add_user(session_params(1, "alice_two", tx_a2))
            .await
            .unwrap();
        manager
            .add_user(session_params(2, "bob", tx_b))
            .await
            .unwrap();

        manager
            .broadcast_to_user_id_per_session(1, |_| ServerMessage::Pong)
            .await;

        assert!(rx_a1.try_recv().is_ok(), "alice's first session received");
        assert!(rx_a2.try_recv().is_ok(), "alice's second session received");
        assert!(
            rx_b.try_recv().is_err(),
            "bob (different user_id) must not receive"
        );
    }

    #[tokio::test]
    async fn test_chat_member_broadcast_filters_and_reuses_preframed_event() {
        let manager = UserManager::new();

        let (tx_alice, mut rx_alice) = ConnectionWriter::channel();
        let (tx_bob, mut rx_bob) = ConnectionWriter::channel();
        let (tx_no_feature, mut rx_no_feature) = ConnectionWriter::channel();
        let (tx_no_permission, mut rx_no_permission) = ConnectionWriter::channel();

        let mut alice = session_params(1, "alice", tx_alice);
        alice.features.push(FEATURE_CHAT.to_string());
        alice.permissions.insert(Permission::ChatReceive);
        let alice_id = manager.add_user(alice).await.unwrap();

        let mut bob = session_params(2, "bob", tx_bob);
        bob.features.push(FEATURE_CHAT.to_string());
        bob.permissions.insert(Permission::ChatReceive);
        let bob_id = manager.add_user(bob).await.unwrap();

        let mut no_feature = session_params(3, "no_feature", tx_no_feature);
        no_feature.permissions.insert(Permission::ChatReceive);
        let no_feature_id = manager.add_user(no_feature).await.unwrap();

        let mut no_permission = session_params(4, "no_permission", tx_no_permission);
        no_permission.features.push(FEATURE_CHAT.to_string());
        let no_permission_id = manager.add_user(no_permission).await.unwrap();

        let message = ServerMessage::ChatMessage {
            session_id: alice_id,
            nickname: "alice".to_string(),
            is_admin: false,
            is_shared: false,
            message: "hello".to_string(),
            action: ChatAction::Normal,
            channel: "#general".to_string(),
            timestamp: 123,
        };

        manager
            .broadcast_chat_message_to_members(
                &[alice_id, bob_id, no_feature_id, no_permission_id, u32::MAX],
                &message,
            )
            .await;

        let alice_frame = match rx_alice.try_recv().expect("alice should receive chat") {
            SessionEvent::Frame(frame) => frame,
            other => panic!("expected preframed chat event, got {other:?}"),
        };
        let bob_frame = match rx_bob.try_recv().expect("bob should receive chat") {
            SessionEvent::Frame(frame) => frame,
            other => panic!("expected preframed chat event, got {other:?}"),
        };

        assert!(
            Arc::ptr_eq(&alice_frame, &bob_frame),
            "chat recipients should share one serialized frame"
        );
        assert!(rx_no_feature.try_recv().is_err());
        assert!(rx_no_permission.try_recv().is_err());

        match SessionEvent::Frame(alice_frame).expect_message().0 {
            ServerMessage::ChatMessage {
                session_id,
                nickname,
                message,
                channel,
                timestamp,
                ..
            } => {
                assert_eq!(session_id, alice_id);
                assert_eq!(nickname, "alice");
                assert_eq!(message, "hello");
                assert_eq!(channel, "#general");
                assert_eq!(timestamp, 123);
            }
            other => panic!("expected chat message, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn test_server_info_updated_auto_join_requires_chat_feature() {
        let manager = UserManager::new();

        let (tx_chatless, mut rx_chatless) = ConnectionWriter::channel();
        let (tx_chat, mut rx_chat) = ConnectionWriter::channel();

        let mut chatless = session_params(1, "chatless", tx_chatless);
        chatless.permissions.insert(Permission::ChatJoin);

        let mut chat = session_params(2, "chat", tx_chat);
        chat.permissions.insert(Permission::ChatJoin);
        chat.features.push(FEATURE_CHAT.to_string());

        manager.add_user(chatless).await.unwrap();
        manager.add_user(chat).await.unwrap();

        manager
            .broadcast_server_info_updated(server_info_values())
            .await;

        let (chatless_msg, _) = rx_chatless
            .try_recv()
            .expect("chatless session should receive server info")
            .expect_message();
        let (chat_msg, _) = rx_chat
            .try_recv()
            .expect("chat session should receive server info")
            .expect_message();

        match chatless_msg {
            ServerMessage::ServerInfoUpdated { server_info } => {
                assert_eq!(server_info.auto_join_channels, None);
            }
            other => panic!("Expected ServerInfoUpdated, got {other:?}"),
        }

        match chat_msg {
            ServerMessage::ServerInfoUpdated { server_info } => {
                assert_eq!(server_info.auto_join_channels, Some("#general".to_string()));
            }
            other => panic!("Expected ServerInfoUpdated, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn test_server_info_updated_reuses_preframed_visibility_variants() {
        let manager = UserManager::new();

        let (tx_alice, mut rx_alice) = ConnectionWriter::channel();
        let (tx_bob, mut rx_bob) = ConnectionWriter::channel();
        let (tx_reindex, mut rx_reindex) = ConnectionWriter::channel();

        manager
            .add_user(session_params(1, "alice", tx_alice))
            .await
            .unwrap();
        manager
            .add_user(session_params(2, "bob", tx_bob))
            .await
            .unwrap();

        let mut reindex = session_params(3, "reindex", tx_reindex);
        reindex.permissions.insert(Permission::FileReindex);
        manager.add_user(reindex).await.unwrap();

        manager
            .broadcast_server_info_updated(server_info_values())
            .await;

        let alice_frame = match rx_alice
            .try_recv()
            .expect("alice should receive server info")
        {
            SessionEvent::Frame(frame) => frame,
            other => panic!("expected preframed server info, got {other:?}"),
        };
        let bob_frame = match rx_bob.try_recv().expect("bob should receive server info") {
            SessionEvent::Frame(frame) => frame,
            other => panic!("expected preframed server info, got {other:?}"),
        };
        let reindex_frame = match rx_reindex
            .try_recv()
            .expect("file-reindex holder should receive server info")
        {
            SessionEvent::Frame(frame) => frame,
            other => panic!("expected preframed server info, got {other:?}"),
        };

        assert!(
            Arc::ptr_eq(&alice_frame, &bob_frame),
            "sessions with identical visibility should share one frame"
        );
        assert!(
            !Arc::ptr_eq(&alice_frame, &reindex_frame),
            "different visibility should use a distinct preframed payload"
        );

        let (alice_msg, _) = SessionEvent::Frame(alice_frame).expect_message();
        let (reindex_msg, _) = SessionEvent::Frame(reindex_frame).expect_message();

        match alice_msg {
            ServerMessage::ServerInfoUpdated { server_info } => {
                assert_eq!(server_info.file_reindex_interval, None);
            }
            other => panic!("Expected ServerInfoUpdated, got {other:?}"),
        }
        match reindex_msg {
            ServerMessage::ServerInfoUpdated { server_info } => {
                assert_eq!(server_info.file_reindex_interval, Some(0));
            }
            other => panic!("Expected ServerInfoUpdated, got {other:?}"),
        }
    }
}
