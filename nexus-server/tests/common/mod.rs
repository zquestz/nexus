//! Common test helpers for integration tests

use std::collections::HashSet;
use std::net::SocketAddr;

use nexus_server::db::{Database, Permission};
use nexus_server::users::UserManager;
use nexus_server::users::user::{ConnectionWriter, NewSessionParams, SessionRx};

#[allow(unused)] // Not all test files use this
pub const DEFAULT_TEST_LOCALE: &str = "en";

/// In-memory test database with migrations applied.
pub async fn create_test_db() -> Database {
    let pool = sqlx::SqlitePool::connect(":memory:")
        .await
        .expect("Failed to create pool");

    sqlx::migrate!("./migrations")
        .run(&pool)
        .await
        .expect("Failed to run migrations");

    Database::new(pool)
}

/// Simulate a login: add a user (mock TCP connection) and return their
/// session_id and message receiver.
#[allow(unused)] // Not all test files use this
pub async fn add_test_user(
    user_manager: &UserManager,
    user_id: i64,
    username: &str,
    is_admin: bool,
    permissions: HashSet<Permission>,
) -> (u32, SessionRx) {
    let (tx, rx) = ConnectionWriter::channel();
    let addr: SocketAddr = "127.0.0.1:8000".parse().unwrap();
    let created_at = chrono::Utc::now().timestamp();

    let session_id = user_manager
        .add_user(NewSessionParams {
            session_id: 0, // Will be assigned by add_user
            user_id,
            username: username.to_string(),
            is_admin,
            is_shared: false,
            permissions,
            address: addr,
            created_at,
            tx,
            features: vec!["chat".to_string(), "news".to_string()],
            locale: DEFAULT_TEST_LOCALE.to_string(),
            avatar: None,
            nickname: username.to_string(),
            is_away: false,
            status: None,
            group_id: None,
            group_name: None,
            bandwidth_weight: nexus_common::validators::DEFAULT_BANDWIDTH_WEIGHT,
            bandwidth_weight_override: None,
            last_activity: std::time::Instant::now(),
        })
        .await
        .expect("Failed to add user to UserManager");

    (session_id, rx)
}
