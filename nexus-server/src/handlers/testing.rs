//! Shared test utilities for handler tests

/// Default locale for tests
pub const DEFAULT_TEST_LOCALE: &str = "en";

use std::net::SocketAddr;

use tokio::io::BufReader;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc;

use nexus_common::framing::{FrameReader, FrameWriter, MessageId};
use nexus_common::io::read_server_message as io_read_server_message;
use nexus_common::protocol::ServerMessage;

use super::HandlerContext;
use crate::db::Database;
use crate::users::UserManager;
use crate::users::user::NewSessionParams;

/// Type alias for the write half used in tests
type TestWriteHalf = tokio::net::tcp::OwnedWriteHalf;

/// Test context that owns all resources needed for handler testing
pub struct TestContext {
    pub client: TcpStream,
    pub frame_writer: FrameWriter<TestWriteHalf>,
    pub user_manager: UserManager,
    pub db: Database,
    pub tx: mpsc::UnboundedSender<(ServerMessage, Option<MessageId>)>,
    pub peer_addr: SocketAddr,
    pub _rx: mpsc::UnboundedReceiver<(ServerMessage, Option<MessageId>)>, // Keep receiver alive to prevent channel closure
    pub message_id: MessageId,
}

impl TestContext {
    /// Create a HandlerContext from this TestContext
    pub fn handler_context(&mut self) -> HandlerContext<'_, TestWriteHalf> {
        HandlerContext {
            writer: &mut self.frame_writer,
            peer_addr: self.peer_addr,
            user_manager: &self.user_manager,
            db: &self.db,
            tx: &self.tx,
            debug: false, // Tests don't need debug logging
            locale: DEFAULT_TEST_LOCALE,
            message_id: self.message_id,
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
    let frame_writer = FrameWriter::new(write_half);

    let client = client_handle.await.unwrap();

    // Create message channel (keep receiver alive to prevent channel closure)
    let (tx, rx) = mpsc::unbounded_channel();

    // Create a default message ID for tests (must be valid hex characters)
    let message_id = MessageId::from_bytes(b"000000000000").unwrap();

    TestContext {
        client,
        frame_writer,
        user_manager,
        db,
        tx,
        peer_addr,
        _rx: rx,
        message_id,
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
        .add_user(NewSessionParams {
            session_id: 0, // Will be assigned by add_user
            db_user_id: user.id,
            username: username.to_string(),
            is_admin,
            permissions: perms.permissions.clone(),
            address: test_ctx.peer_addr,
            created_at: user.created_at,
            tx: test_ctx.tx.clone(),
            features,
            locale: DEFAULT_TEST_LOCALE.to_string(),
            avatar: None,
        })
        .await
}

/// Helper to read a ServerMessage from the client stream using the new framing format
pub async fn read_server_message(client: &mut TcpStream) -> ServerMessage {
    let (read_half, _write_half) = client.split();
    let buf_reader = BufReader::new(read_half);
    let mut frame_reader = FrameReader::new(buf_reader);

    io_read_server_message(&mut frame_reader)
        .await
        .expect("Failed to read message")
        .expect("Connection closed unexpectedly")
        .message
}
