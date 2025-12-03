//! Common test helpers for integration tests

use std::collections::HashSet;
use std::net::SocketAddr;

use nexus_common::protocol::ServerMessage;
use nexus_server::db::{Database, Permission};
use nexus_server::users::UserManager;
use nexus_server::users::user::NewSessionParams;
use tokio::sync::mpsc;

/// Default locale for integration tests
pub const DEFAULT_TEST_LOCALE: &str = "en";

/// Create an in-memory test database with migrations applied
pub async fn create_test_db() -> Database {
    let pool = sqlx::SqlitePool::connect(":memory:")
        .await
        .expect("Failed to create pool");

    // Run migrations
    sqlx::migrate!("./migrations")
        .run(&pool)
        .await
        .expect("Failed to run migrations");

    Database::new(pool)
}

/// Add a test user to UserManager and return their session_id and message receiver
///
/// This helper simulates a user logging in by adding them to the UserManager
/// with a mock TCP connection.
///
/// # Arguments
/// * `user_manager` - The UserManager to add the user to
/// * `db_user_id` - The database ID of the user
/// * `username` - The username
/// * `is_admin` - Whether the user is an admin (admins bypass permission checks)
/// * `permissions` - Cached permissions for non-admin users
pub async fn add_test_user(
    user_manager: &UserManager,
    db_user_id: i64,
    username: &str,
    is_admin: bool,
    permissions: HashSet<Permission>,
) -> (u32, mpsc::UnboundedReceiver<ServerMessage>) {
    let (tx, rx) = mpsc::unbounded_channel();
    let addr: SocketAddr = "127.0.0.1:8000".parse().unwrap();
    let created_at = chrono::Utc::now().timestamp();

    // Use the public add_user API
    let session_id = user_manager
        .add_user(NewSessionParams {
            session_id: 0, // Will be assigned by add_user
            db_user_id,
            username: username.to_string(),
            is_admin,
            permissions,
            address: addr,
            created_at,
            tx,
            features: vec!["chat".to_string()],
            locale: DEFAULT_TEST_LOCALE.to_string(),
        })
        .await;

    (session_id, rx)
}
