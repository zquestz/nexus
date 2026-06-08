//! Integration tests for connection flows and multi-session scenarios

mod common;

use std::collections::HashSet;

use common::{add_test_user, create_test_db};
use nexus_common::framing::MessageId;
use nexus_common::protocol::{ChatAction, NewsAction, ServerMessage, UserInfo};
use nexus_common::validators::DEFAULT_CHANNEL;
use nexus_server::constants::FEATURE_NEWS;
use nexus_server::db::{self, CreateUserParams, Permission, Permissions};
use nexus_server::users::UserManager;
use nexus_server::users::user::SessionEvent;

trait SessionEventExt {
    fn expect_message(self) -> (ServerMessage, Option<MessageId>);
}

impl SessionEventExt for SessionEvent {
    fn expect_message(self) -> (ServerMessage, Option<MessageId>) {
        match self {
            SessionEvent::Message(message, message_id) => (*message, message_id),
            SessionEvent::Disconnect => panic!("expected session message, got disconnect event"),
            SessionEvent::SlowClientDisconnect => {
                panic!("expected session message, got slow-client disconnect event")
            }
        }
    }
}

#[tokio::test]
async fn test_multi_session_partial_disconnect() {
    let db = create_test_db().await;
    let user_manager = UserManager::new();

    let hashed_password = db::hash_password(
        "password",
        nexus_common::validators::PasswordStrength::Weak,
        true,
    )
    .unwrap();
    let mut perms = Permissions::new();
    perms.add(Permission::UserList);
    let alice = db
        .users
        .create_user(CreateUserParams {
            username: "alice",
            hashed_password: &hashed_password,
            is_admin: false,
            is_shared: false,
            enabled: true,
            permissions: &perms,
            group_id: None,
            revokes: &[],
            bandwidth_weight: None,
        })
        .await
        .unwrap();

    // Alice logs in from 3 devices (3 sessions).
    let cached_perms: HashSet<Permission> = [Permission::UserList].into_iter().collect();
    let (session_id1, mut rx1) = add_test_user(
        &user_manager,
        alice.id,
        "alice",
        false,
        cached_perms.clone(),
    )
    .await;
    let (session_id2, mut rx2) = add_test_user(
        &user_manager,
        alice.id,
        "alice",
        false,
        cached_perms.clone(),
    )
    .await;
    let (session_id3, mut rx3) =
        add_test_user(&user_manager, alice.id, "alice", false, cached_perms).await;

    let all_users = user_manager.get_all_users().await;
    let alice_sessions: Vec<u32> = all_users
        .iter()
        .filter(|u| u.username == "alice")
        .map(|u| u.session_id)
        .collect();
    assert_eq!(alice_sessions.len(), 3, "Should have 3 sessions");
    assert!(alice_sessions.contains(&session_id1));
    assert!(alice_sessions.contains(&session_id2));
    assert!(alice_sessions.contains(&session_id3));

    // Disconnect session 2 (middle device) and broadcast to the rest.
    let removed = user_manager.remove_users(&[session_id2]).await;
    assert!(!removed.is_empty(), "Session 2 should be removed");

    user_manager
        .broadcast_user_event(
            ServerMessage::UserDisconnected {
                session_id: session_id2,
                nickname: "alice".to_string(),
            },
            Some(session_id2), // Exclude disconnected session
        )
        .await;

    let remaining = user_manager.get_all_users().await;
    let remaining_sessions: Vec<u32> = remaining
        .iter()
        .filter(|u| u.username == "alice")
        .map(|u| u.session_id)
        .collect();
    assert_eq!(
        remaining_sessions.len(),
        2,
        "Should have 2 sessions remaining"
    );
    assert!(remaining_sessions.contains(&session_id1));
    assert!(remaining_sessions.contains(&session_id3));
    assert!(!remaining_sessions.contains(&session_id2));

    // Sessions 1 and 3 receive UserDisconnected; session 2's channel is closed.
    let msg1 = rx1.try_recv();
    assert!(msg1.is_ok(), "Session 1 should receive disconnect message");
    match msg1.unwrap().expect_message().0 {
        ServerMessage::UserDisconnected {
            session_id,
            nickname,
        } => {
            assert_eq!(session_id, session_id2);
            assert_eq!(nickname, "alice");
        }
        _ => panic!("Expected UserDisconnected"),
    }

    let msg3 = rx3.try_recv();
    assert!(msg3.is_ok(), "Session 3 should receive disconnect message");

    let msg2 = rx2.try_recv();
    assert!(
        msg2.is_err(),
        "Session 20 should not receive message (already disconnected)"
    );

    user_manager.remove_users(&[session_id1]).await;
    user_manager.remove_users(&[session_id3]).await;

    let final_users = user_manager.get_all_users().await;
    let final_alice: Vec<_> = final_users
        .iter()
        .filter(|u| u.username == "alice")
        .collect();
    assert_eq!(final_alice.len(), 0, "Alice should have no sessions");
}

#[tokio::test]
async fn test_broadcast_respects_user_list_permission() {
    let db = create_test_db().await;
    let user_manager = UserManager::new();

    let hashed = db::hash_password(
        "password",
        nexus_common::validators::PasswordStrength::Weak,
        true,
    )
    .unwrap();
    let admin = db
        .users
        .create_user(CreateUserParams {
            username: "admin",
            hashed_password: &hashed,
            is_admin: true,
            is_shared: false,
            enabled: true,
            permissions: &Permissions::new(),
            group_id: None,
            revokes: &[],
            bandwidth_weight: None,
        })
        .await
        .unwrap();

    // User WITH user_list permission.
    let mut perms_with = Permissions::new();
    perms_with.add(Permission::UserList);
    let user_with = db
        .users
        .create_user(CreateUserParams {
            username: "user_with",
            hashed_password: &hashed,
            is_admin: false,
            is_shared: false,
            enabled: true,
            permissions: &perms_with,
            group_id: None,
            revokes: &[],
            bandwidth_weight: None,
        })
        .await
        .unwrap();

    // User WITHOUT user_list permission.
    let user_without = db
        .users
        .create_user(CreateUserParams {
            username: "user_without",
            hashed_password: &hashed,
            is_admin: false,
            is_shared: false,
            enabled: true,
            permissions: &Permissions::new(),
            group_id: None,
            revokes: &[],
            bandwidth_weight: None,
        })
        .await
        .unwrap();

    let (_sid_admin, mut rx_admin) =
        add_test_user(&user_manager, admin.id, "admin", true, HashSet::new()).await;
    let cached_user_list: HashSet<Permission> = [Permission::UserList].into_iter().collect();
    let (_sid_with, mut rx_with) = add_test_user(
        &user_manager,
        user_with.id,
        "user_with",
        false,
        cached_user_list,
    )
    .await;
    let (_sid_without, mut rx_without) = add_test_user(
        &user_manager,
        user_without.id,
        "user_without",
        false,
        HashSet::new(),
    )
    .await;

    user_manager
        .broadcast_user_event(
            ServerMessage::UserConnected {
                user: UserInfo {
                    id: 0,
                    username: "newuser".to_string(),
                    nickname: "newuser".to_string(),
                    is_admin: false,
                    is_shared: false,
                    login_time: chrono::Utc::now().timestamp(),
                    session_ids: vec![99],
                    locale: "en".to_string(),
                    avatar: None,
                    is_away: false,
                    status: None,
                    group_id: None,
                    group_name: None,
                    bandwidth_weight: nexus_common::validators::DEFAULT_BANDWIDTH_WEIGHT,
                },
            },
            None, // Don't exclude anyone
        )
        .await;

    let msg_admin = rx_admin.try_recv();
    assert!(
        msg_admin.is_ok(),
        "Admin should receive UserConnected message"
    );
    assert!(matches!(
        msg_admin.unwrap().expect_message().0,
        ServerMessage::UserConnected { .. }
    ));

    let msg_with = rx_with.try_recv();
    assert!(
        msg_with.is_ok(),
        "User with user_list permission should receive message"
    );
    assert!(matches!(
        msg_with.unwrap().expect_message().0,
        ServerMessage::UserConnected { .. }
    ));

    let msg_without = rx_without.try_recv();
    assert!(
        msg_without.is_err(),
        "User without user_list permission should NOT receive message"
    );
}

#[tokio::test]
async fn test_broadcast_excludes_specified_session() {
    let db = create_test_db().await;
    let user_manager = UserManager::new();

    let hashed = db::hash_password(
        "password",
        nexus_common::validators::PasswordStrength::Weak,
        true,
    )
    .unwrap();
    let mut perms = Permissions::new();
    perms.add(Permission::UserList);

    let user1 = db
        .users
        .create_user(CreateUserParams {
            username: "user1",
            hashed_password: &hashed,
            is_admin: false,
            is_shared: false,
            enabled: true,
            permissions: &perms,
            group_id: None,
            revokes: &[],
            bandwidth_weight: None,
        })
        .await
        .unwrap();
    let user2 = db
        .users
        .create_user(CreateUserParams {
            username: "user2",
            hashed_password: &hashed,
            is_admin: false,
            is_shared: false,
            enabled: true,
            permissions: &perms,
            group_id: None,
            revokes: &[],
            bandwidth_weight: None,
        })
        .await
        .unwrap();

    let cached_perms: HashSet<Permission> = [Permission::UserList].into_iter().collect();
    let (session_id1, mut rx1) = add_test_user(
        &user_manager,
        user1.id,
        "user1",
        false,
        cached_perms.clone(),
    )
    .await;
    let (_session_id2, mut rx2) =
        add_test_user(&user_manager, user2.id, "user2", false, cached_perms).await;

    // Broadcast excluding session 1.
    user_manager
        .broadcast_user_event(
            ServerMessage::UserConnected {
                user: UserInfo {
                    id: 0,
                    username: "newcomer".to_string(),
                    nickname: "newcomer".to_string(),
                    is_admin: false,
                    is_shared: false,
                    login_time: chrono::Utc::now().timestamp(),
                    session_ids: vec![30],
                    locale: "en".to_string(),
                    avatar: None,
                    is_away: false,
                    status: None,
                    group_id: None,
                    group_name: None,
                    bandwidth_weight: nexus_common::validators::DEFAULT_BANDWIDTH_WEIGHT,
                },
            },
            Some(session_id1), // Exclude session 1
        )
        .await;

    let msg1 = rx1.try_recv();
    assert!(
        msg1.is_err(),
        "Session 1 should not receive message (excluded)"
    );

    let msg2 = rx2.try_recv();
    assert!(msg2.is_ok(), "Session 2 should receive message");
    match msg2.unwrap().expect_message().0 {
        ServerMessage::UserConnected { .. } => {}
        _ => panic!("Expected UserConnected"),
    }
}

#[tokio::test]
async fn test_broadcast_to_feature_excludes_specified_session() {
    // Mirrors news_create/update/delete: broadcast NewsUpdated to the "news"
    // feature gated by NewsList, excluding the originator so they don't get a
    // redundant refresh on top of their typed *Response.
    let db = create_test_db().await;
    let user_manager = UserManager::new();

    let hashed = db::hash_password(
        "password",
        nexus_common::validators::PasswordStrength::Weak,
        true,
    )
    .unwrap();
    let mut perms = Permissions::new();
    perms.add(Permission::NewsList);

    let user1 = db
        .users
        .create_user(CreateUserParams {
            username: "user1",
            hashed_password: &hashed,
            is_admin: false,
            is_shared: false,
            enabled: true,
            permissions: &perms,
            group_id: None,
            revokes: &[],
            bandwidth_weight: None,
        })
        .await
        .unwrap();
    let user2 = db
        .users
        .create_user(CreateUserParams {
            username: "user2",
            hashed_password: &hashed,
            is_admin: false,
            is_shared: false,
            enabled: true,
            permissions: &perms,
            group_id: None,
            revokes: &[],
            bandwidth_weight: None,
        })
        .await
        .unwrap();

    // add_test_user grants the "news" feature by default.
    let cached_perms: HashSet<Permission> = [Permission::NewsList].into_iter().collect();
    let (session_id1, mut rx1) = add_test_user(
        &user_manager,
        user1.id,
        "user1",
        false,
        cached_perms.clone(),
    )
    .await;
    let (_session_id2, mut rx2) =
        add_test_user(&user_manager, user2.id, "user2", false, cached_perms).await;

    let post_id: i64 = 42;
    user_manager
        .broadcast_to_feature(
            FEATURE_NEWS,
            ServerMessage::NewsUpdated {
                action: NewsAction::Created,
                id: post_id,
            },
            Permission::NewsList,
            Some(session_id1),
        )
        .await;

    assert!(
        rx1.try_recv().is_err(),
        "Originator should not receive NewsUpdated for their own action"
    );

    let msg2 = rx2.try_recv();
    assert!(msg2.is_ok(), "Observer should receive NewsUpdated");
    match msg2.unwrap().expect_message().0 {
        ServerMessage::NewsUpdated { action, id } => {
            assert_eq!(action, NewsAction::Created);
            assert_eq!(id, post_id);
        }
        other => panic!("Expected NewsUpdated, got {other:?}"),
    }
}

#[tokio::test]
async fn test_broadcast_detects_closed_channels() {
    let db = create_test_db().await;
    let user_manager = UserManager::new();

    let hashed = db::hash_password(
        "password",
        nexus_common::validators::PasswordStrength::Weak,
        true,
    )
    .unwrap();
    let mut perms = Permissions::new();
    perms.add(Permission::ChatReceive);

    let user1 = db
        .users
        .create_user(CreateUserParams {
            username: "user1",
            hashed_password: &hashed,
            is_admin: false,
            is_shared: false,
            enabled: true,
            permissions: &perms,
            group_id: None,
            revokes: &[],
            bandwidth_weight: None,
        })
        .await
        .unwrap();
    let user2 = db
        .users
        .create_user(CreateUserParams {
            username: "user2",
            hashed_password: &hashed,
            is_admin: false,
            is_shared: false,
            enabled: true,
            permissions: &perms,
            group_id: None,
            revokes: &[],
            bandwidth_weight: None,
        })
        .await
        .unwrap();

    let cached_perms: HashSet<Permission> = [Permission::ChatReceive].into_iter().collect();
    let (session_id1, rx1) = add_test_user(
        &user_manager,
        user1.id,
        "user1",
        false,
        cached_perms.clone(),
    )
    .await;
    let (session_id2, rx2) =
        add_test_user(&user_manager, user2.id, "user2", false, cached_perms).await;

    // Drop rx1 to simulate a dead connection (closed channel).
    drop(rx1);

    assert!(
        user_manager
            .get_user_by_session_id(session_id1)
            .await
            .is_some()
    );
    assert!(
        user_manager
            .get_user_by_session_id(session_id2)
            .await
            .is_some()
    );

    // Broadcast should detect and prune the closed channel.
    user_manager
        .broadcast_to_feature(
            "chat",
            ServerMessage::ChatMessage {
                session_id: 999,
                nickname: "system".to_string(),
                is_admin: false,
                is_shared: false,
                message: "test".to_string(),
                action: ChatAction::Normal,
                channel: DEFAULT_CHANNEL.to_string(),
                timestamp: 0,
            },
            Permission::ChatReceive,
            None,
        )
        .await;

    assert!(
        user_manager
            .get_user_by_session_id(session_id1)
            .await
            .is_none(),
        "User 1 should be removed after broadcast detected closed channel"
    );

    assert!(
        user_manager
            .get_user_by_session_id(session_id2)
            .await
            .is_some(),
        "User 2 should still exist"
    );

    drop(rx2);
}
