//! Shared test utilities for handler tests

/// Default locale for tests
pub const DEFAULT_TEST_LOCALE: &str = "en";

use super::{HandlerContext, Writer};
use crate::db::Database;
use crate::users::UserManager;
use nexus_common::protocol::ServerMessage;
use std::net::SocketAddr;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc;

/// Test context that owns all resources needed for handler testing
pub struct TestContext {
    pub client: TcpStream,
    pub write_half: Writer,
    pub user_manager: UserManager,
    pub db: Database,
    pub tx: mpsc::UnboundedSender<ServerMessage>,
    pub peer_addr: SocketAddr,
    pub _rx: mpsc::UnboundedReceiver<ServerMessage>, // Keep receiver alive to prevent channel closure
}

impl TestContext {
    /// Create a HandlerContext from this TestContext
    pub fn handler_context(&mut self) -> HandlerContext<'_> {
        HandlerContext {
            writer: &mut self.write_half,
            peer_addr: self.peer_addr,
            user_manager: &self.user_manager,
            db: &self.db,
            tx: &self.tx,
            debug: false, // Tests don't need debug logging
        }
    }
}

/// Helper to create test context using real TCP sockets
///
/// Returns a TestContext that owns all resources and can create HandlerContext instances
pub async fn create_test_context() -> TestContext {
    // Create in-memory database
    let pool = sqlx::SqlitePool::connect(":memory:")
        .await
        .expect("Failed to create test database");
    sqlx::migrate!("./migrations")
        .run(&pool)
        .await
        .expect("Failed to run migrations");

    let db = Database::new(pool);
    let user_manager = UserManager::new();

    // Create TCP listener on localhost
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    // Connect client
    let client_handle = tokio::spawn(async move { TcpStream::connect(addr).await.unwrap() });

    // Accept connection
    let (server_stream, peer_addr) = listener.accept().await.unwrap();
    let (_read_half, write_half) = server_stream.into_split();
    let write_half = Box::pin(write_half);

    let client = client_handle.await.unwrap();

    // Create message channel (keep receiver alive to prevent channel closure)
    let (tx, rx) = mpsc::unbounded_channel();

    TestContext {
        client,
        write_half,
        user_manager,
        db,
        tx,
        peer_addr,
        _rx: rx,
    }
}

/// Helper to create a user and add them to UserManager, returning their session_id
pub async fn login_user(
    test_ctx: &mut TestContext,
    username: &str,
    password: &str,
    permissions: &[crate::db::Permission],
    is_admin: bool,
) -> u32 {
    login_user_with_features(test_ctx, username, password, permissions, is_admin, vec![]).await
}

/// Helper to create a user with features and add them to UserManager, returning their session_id
pub async fn login_user_with_features(
    test_ctx: &mut TestContext,
    username: &str,
    password: &str,
    permissions: &[crate::db::Permission],
    is_admin: bool,
    features: Vec<String>,
) -> u32 {
    use crate::db::{Permissions, hash_password};

    // Hash password
    let hashed = hash_password(password).unwrap();

    // Build permissions
    let mut perms = Permissions::new();
    for perm in permissions {
        perms.permissions.insert(*perm);
    }

    // Create user in database
    let user = test_ctx
        .db
        .users
        .create_user(username, &hashed, is_admin, true, &perms)
        .await
        .unwrap();

    // Add user to UserManager
    test_ctx
        .user_manager
        .add_user(
            user.id,
            username.to_string(),
            test_ctx.peer_addr,
            user.created_at,
            test_ctx.tx.clone(),
            features,
            DEFAULT_TEST_LOCALE.to_string(),
        )
        .await
}

/// Helper to read a ServerMessage from the client stream
pub async fn read_server_message(client: &mut TcpStream) -> ServerMessage {
    use tokio::io::AsyncBufReadExt;

    let mut reader = tokio::io::BufReader::new(client);
    let mut line = String::new();
    reader.read_line(&mut line).await.unwrap();
    serde_json::from_str(line.trim()).unwrap()
}
