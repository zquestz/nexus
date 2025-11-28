//! Integration tests for connection flows and multi-session scenarios

mod common;

use common::{add_test_user, create_test_db};
use nexus_common::protocol::{ServerMessage, UserInfo};
use nexus_server::db::{self, Permission, Permissions};
use nexus_server::users::UserManager;

// ============================================================================
// Multi-Session Tests
// ============================================================================

#[tokio::test]
async fn test_multi_session_partial_disconnect() {
    let db = create_test_db().await;
    let user_manager = UserManager::new();

    // Create a user in database with user_list permission
    let hashed_password = db::hash_password("password").unwrap();
    let mut perms = Permissions::new();
    perms.add(Permission::UserList);
    let alice = db
        .users
        .create_user("alice", &hashed_password, false, true, &perms)
        .await
        .unwrap();

    // Alice logs in from 3 devices (3 sessions)
    let (session_id1, mut rx1) = add_test_user(&user_manager, alice.id, "alice").await;
    let (session_id2, mut rx2) = add_test_user(&user_manager, alice.id, "alice").await;
    let (session_id3, mut rx3) = add_test_user(&user_manager, alice.id, "alice").await;

    // Verify all 3 sessions exist
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

    // Disconnect session 2 (middle device)
    let removed = user_manager.remove_user(session_id2).await;
    assert!(removed.is_some(), "Session 2 should be removed");

    // Broadcast disconnect to remaining users
    user_manager
        .broadcast_user_event(
            ServerMessage::UserDisconnected {
                session_id: session_id2,
                username: "alice".to_string(),
            },
            &db.users,
            Some(session_id2), // Exclude disconnected session
        )
        .await;

    // Sessions 1 and 3 should still exist
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

    // Sessions 1 and 3 should receive UserDisconnected message
    let msg1 = rx1.try_recv();
    assert!(msg1.is_ok(), "Session 1 should receive disconnect message");
    match msg1.unwrap() {
        ServerMessage::UserDisconnected {
            session_id,
            username,
        } => {
            assert_eq!(session_id, session_id2);
            assert_eq!(username, "alice");
        }
        _ => panic!("Expected UserDisconnected"),
    }

    let msg3 = rx3.try_recv();
    assert!(msg3.is_ok(), "Session 3 should receive disconnect message");

    // Session 2's channel should be closed (already removed)
    let msg2 = rx2.try_recv();
    assert!(
        msg2.is_err(),
        "Session 20 should not receive message (already disconnected)"
    );

    // Disconnect remaining sessions
    user_manager.remove_user(session_id1).await;
    user_manager.remove_user(session_id3).await;

    // No sessions should remain for alice
    let final_users = user_manager.get_all_users().await;
    let final_alice: Vec<_> = final_users
        .iter()
        .filter(|u| u.username == "alice")
        .collect();
    assert_eq!(final_alice.len(), 0, "Alice should have no sessions");
}

// ============================================================================
// Permission-Based Broadcasting Tests
// ============================================================================

#[tokio::test]
async fn test_broadcast_respects_user_list_permission() {
    let db = create_test_db().await;
    let user_manager = UserManager::new();

    // Create admin (has all permissions)
    let hashed = db::hash_password("password").unwrap();
    let admin = db
        .users
        .create_user("admin", &hashed, true, true, &Permissions::new())
        .await
        .unwrap();

    // Create user WITH user_list permission
    let mut perms_with = Permissions::new();
    perms_with.add(Permission::UserList);
    let user_with = db
        .users
        .create_user("user_with", &hashed, false, true, &perms_with)
        .await
        .unwrap();

    // Create user WITHOUT user_list permission
    let user_without = db
        .users
        .create_user("user_without", &hashed, false, true, &Permissions::new())
        .await
        .unwrap();

    // Add all to UserManager
    let (_sid_admin, mut rx_admin) = add_test_user(&user_manager, admin.id, "admin").await;
    let (_sid_with, mut rx_with) = add_test_user(&user_manager, user_with.id, "user_with").await;
    let (_sid_without, mut rx_without) =
        add_test_user(&user_manager, user_without.id, "user_without").await;

    // Broadcast UserConnected event
    user_manager
        .broadcast_user_event(
            ServerMessage::UserConnected {
                user: UserInfo {
                    username: "newuser".to_string(),
                    is_admin: false,
                    login_time: chrono::Utc::now().timestamp() as u64,
                    session_ids: vec![99],
                    locale: "en".to_string(),
                },
            },
            &db.users,
            None, // Don't exclude anyone
        )
        .await;

    // Admin should receive (has all permissions)
    let msg_admin = rx_admin.try_recv();
    assert!(
        msg_admin.is_ok(),
        "Admin should receive UserConnected message"
    );

    // User with permission should receive
    let msg_with = rx_with.try_recv();
    assert!(
        msg_with.is_ok(),
        "User with user_list permission should receive message"
    );

    // User without permission should NOT receive
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

    // Create users with user_list permission
    let hashed = db::hash_password("password").unwrap();
    let mut perms = Permissions::new();
    perms.add(Permission::UserList);

    let user1 = db
        .users
        .create_user("user1", &hashed, false, true, &perms)
        .await
        .unwrap();
    let user2 = db
        .users
        .create_user("user2", &hashed, false, true, &perms)
        .await
        .unwrap();

    let (session_id1, mut rx1) = add_test_user(&user_manager, user1.id, "user1").await;
    let (_session_id2, mut rx2) = add_test_user(&user_manager, user2.id, "user2").await;

    // Broadcast with exclusion of session 1
    user_manager
        .broadcast_user_event(
            ServerMessage::UserConnected {
                user: UserInfo {
                    username: "newcomer".to_string(),
                    is_admin: false,
                    login_time: chrono::Utc::now().timestamp() as u64,
                    session_ids: vec![30],
                    locale: "en".to_string(),
                },
            },
            &db.users,
            Some(session_id1), // Exclude session 1
        )
        .await;

    // Session 1 should NOT receive (excluded)
    let msg1 = rx1.try_recv();
    assert!(
        msg1.is_err(),
        "Session 1 should not receive message (excluded)"
    );

    // Session 2 should receive
    let msg2 = rx2.try_recv();
    assert!(msg2.is_ok(), "Session 2 should receive message");
    match msg2.unwrap() {
        ServerMessage::UserConnected { .. } => {}
        _ => panic!("Expected UserConnected"),
    }
}

// ============================================================================
// Disconnect Detection Tests
// ============================================================================

#[tokio::test]
async fn test_broadcast_detects_closed_channels() {
    let db = create_test_db().await;
    let user_manager = UserManager::new();

    // Create users with permission
    let hashed = db::hash_password("password").unwrap();
    let mut perms = Permissions::new();
    perms.add(Permission::ChatReceive);

    let user1 = db
        .users
        .create_user("user1", &hashed, false, true, &perms)
        .await
        .unwrap();
    let user2 = db
        .users
        .create_user("user2", &hashed, false, true, &perms)
        .await
        .unwrap();

    // Add users to manager
    let (session_id1, rx1) = add_test_user(&user_manager, user1.id, "user1").await;
    let (session_id2, rx2) = add_test_user(&user_manager, user2.id, "user2").await;

    // Drop rx1 to close the channel (simulates dead connection)
    drop(rx1);

    // Verify both users exist before broadcast
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

    // Broadcast a message (should detect closed channel)
    user_manager
        .broadcast_to_feature(
            "chat",
            ServerMessage::ChatMessage {
                session_id: 999,
                username: "system".to_string(),
                message: "test".to_string(),
            },
            &db.users,
            Permission::ChatReceive,
        )
        .await;

    // User 1 should be removed (channel was closed)
    assert!(
        user_manager
            .get_user_by_session_id(session_id1)
            .await
            .is_none(),
        "User 1 should be removed after broadcast detected closed channel"
    );

    // User 2 should still exist
    assert!(
        user_manager
            .get_user_by_session_id(session_id2)
            .await
            .is_some(),
        "User 2 should still exist"
    );

    // Clean up
    drop(rx2);
}
