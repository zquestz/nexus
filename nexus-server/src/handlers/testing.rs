//! Shared test utilities for handler tests

/// Default locale for tests
pub const DEFAULT_TEST_LOCALE: &str = "en";

use std::collections::HashMap;
use std::net::SocketAddr;
use std::path::Path;
use std::sync::{Arc, LazyLock, RwLock};

use tempfile::TempDir;

use tokio::io::BufReader;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc;

use nexus_common::framing::{FrameReader, FrameWriter, MessageId};
use nexus_common::io::read_server_message as io_read_server_message;
use nexus_common::protocol::ServerMessage;

use super::HandlerContext;
use crate::connection_tracker::ConnectionTracker;
use crate::db::Database;
use crate::files::FileIndex;
use crate::ip_rule_cache::IpRuleCache;
use crate::users::UserManager;
use crate::users::user::NewSessionParams;

/// Type alias for the write half used in tests
type TestWriteHalf = tokio::net::tcp::OwnedWriteHalf;

// ========================================================================
// Cached Password Hashes for Test Performance
// ========================================================================
//
// Argon2 password hashing is intentionally slow (~1 second per hash) for security.
// However, this makes tests extremely slow since handler tests call login_user()
// ~460 times. Pre-computing and caching hashes for common test passwords provides
// a ~10x speedup for the test suite.
//
// The cache is populated lazily - the first test to use a password pays the
// hashing cost, but all subsequent tests reuse the cached hash.

/// Global cache for pre-computed password hashes.
///
/// This dramatically speeds up tests by avoiding repeated Argon2 computations.
/// The cache is thread-safe and lazily populated.
static PASSWORD_HASH_CACHE: LazyLock<RwLock<HashMap<String, String>>> =
    LazyLock::new(|| RwLock::new(HashMap::new()));

/// Get a cached password hash, computing and caching it if not already present.
///
/// This function provides a ~1000x speedup for repeated password hashing in tests
/// by caching the Argon2 hash for each unique password. Since most tests use the
/// same password ("password"), this effectively eliminates password hashing
/// overhead after the first test.
///
/// # Thread Safety
///
/// Uses a read-write lock for concurrent access. Multiple tests can read cached
/// hashes simultaneously; writes only occur for new passwords.
pub fn get_cached_password_hash(password: &str) -> String {
    // Try to read from cache first (fast path)
    {
        let cache = PASSWORD_HASH_CACHE.read().unwrap();
        if let Some(hash) = cache.get(password) {
            return hash.clone();
        }
    }

    // Cache miss - compute hash and store it
    let hash = crate::db::hash_password(password).expect("Password hashing failed in test");

    {
        let mut cache = PASSWORD_HASH_CACHE.write().unwrap();
        // Double-check in case another thread computed it while we were hashing
        cache.entry(password.to_string()).or_insert(hash).clone()
    }
}

/// Test context that owns all resources needed for handler testing
pub struct TestContext {
    pub client: TcpStream,
    pub frame_writer: FrameWriter<TestWriteHalf>,
    pub user_manager: UserManager,
    pub db: Database,
    pub tx: mpsc::UnboundedSender<(ServerMessage, Option<MessageId>)>,
    pub peer_addr: SocketAddr,
    pub rx: mpsc::UnboundedReceiver<(ServerMessage, Option<MessageId>)>,
    pub message_id: MessageId,
    pub file_root: Option<&'static Path>,
    pub connection_tracker: Arc<ConnectionTracker>,
    pub ip_rule_cache: Arc<RwLock<IpRuleCache>>,
    pub file_index: Arc<FileIndex>,
    /// Keep temp dir alive for tests that use file areas
    #[allow(dead_code)]
    temp_dir: TempDir,
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
            file_root: self.file_root,
            transfer_port: nexus_common::DEFAULT_TRANSFER_PORT,
            connection_tracker: self.connection_tracker.clone(),
            ip_rule_cache: self.ip_rule_cache.clone(),
            file_index: self.file_index.clone(),
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
    let message_id = MessageId::from_bytes(b"000000000000").expect("valid hex test message ID");

    // Create connection tracker for tests (unlimited by default)
    let connection_tracker = Arc::new(ConnectionTracker::new(0, 0));

    // Create empty IP rule cache for tests
    let ip_rule_cache = Arc::new(RwLock::new(IpRuleCache::new()));

    // Create temp directory for file index
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let file_index = Arc::new(FileIndex::new(temp_dir.path(), temp_dir.path()));

    TestContext {
        client,
        frame_writer,
        user_manager,
        db,
        tx,
        peer_addr,
        rx,
        message_id,
        file_root: None,
        connection_tracker,
        ip_rule_cache,
        file_index,
        temp_dir,
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

/// Helper to create a user from a specific IP address, returning their session_id
///
/// This is useful for testing ban scenarios where users need to be on different IPs.
pub async fn login_user_from_ip(
    test_ctx: &mut TestContext,
    username: &str,
    password: &str,
    permissions: &[crate::db::Permission],
    is_admin: bool,
    ip: &str,
) -> u32 {
    use crate::db::Permissions;

    // Get cached password hash (fast path for repeated passwords)
    let hashed = get_cached_password_hash(password);

    // Build permissions
    let mut perms = Permissions::new();
    for perm in permissions {
        perms.permissions.insert(*perm);
    }

    // Create user in database
    let user = test_ctx
        .db
        .users
        .create_user(username, &hashed, is_admin, false, true, &perms)
        .await
        .unwrap();

    // Parse the IP into a SocketAddr
    let addr: SocketAddr = format!("{}:12345", ip).parse().expect("valid IP address");

    // Add user to UserManager with custom address
    test_ctx
        .user_manager
        .add_user(NewSessionParams {
            session_id: 0, // Will be assigned by add_user
            db_user_id: user.id,
            username: username.to_string(),
            is_admin,
            is_shared: false,
            permissions: perms.permissions.clone(),
            address: addr,
            created_at: user.created_at,
            tx: test_ctx.tx.clone(),
            features: vec![],
            locale: DEFAULT_TEST_LOCALE.to_string(),
            avatar: None,
            nickname: username.to_string(), // Regular account: nickname == username
            is_away: false,
            status: None,
        })
        .await
        .expect("Failed to add user to UserManager")
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
    use crate::db::Permissions;

    // Get cached password hash (fast path for repeated passwords)
    let hashed = get_cached_password_hash(password);

    // Build permissions
    let mut perms = Permissions::new();
    for perm in permissions {
        perms.permissions.insert(*perm);
    }

    // Create user in database
    let user = test_ctx
        .db
        .users
        .create_user(username, &hashed, is_admin, false, true, &perms)
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
            is_shared: false,
            permissions: perms.permissions.clone(),
            address: test_ctx.peer_addr,
            created_at: user.created_at,
            tx: test_ctx.tx.clone(),
            features,
            locale: DEFAULT_TEST_LOCALE.to_string(),
            avatar: None,
            nickname: username.to_string(), // Regular account: nickname == username
            is_away: false,
            status: None,
        })
        .await
        .expect("Failed to add user to UserManager")
}

/// Helper to create a shared account user with a nickname and add them to UserManager
pub async fn login_shared_user(
    test_ctx: &mut TestContext,
    account_username: &str,
    password: &str,
    nickname: &str,
    permissions: &[crate::db::Permission],
) -> u32 {
    use crate::db::Permissions;

    // Get cached password hash (fast path for repeated passwords)
    let hashed = get_cached_password_hash(password);

    // Build permissions
    let mut perms = Permissions::new();
    for perm in permissions {
        perms.permissions.insert(*perm);
    }

    // Create shared account in database (is_shared = true)
    let user = test_ctx
        .db
        .users
        .create_user(account_username, &hashed, false, true, true, &perms)
        .await
        .unwrap();

    // Add user to UserManager with nickname
    test_ctx
        .user_manager
        .add_user(NewSessionParams {
            session_id: 0, // Will be assigned by add_user
            db_user_id: user.id,
            username: account_username.to_string(),
            is_admin: false,
            is_shared: true,
            permissions: perms.permissions.clone(),
            address: test_ctx.peer_addr,
            created_at: user.created_at,
            tx: test_ctx.tx.clone(),
            features: vec![],
            locale: DEFAULT_TEST_LOCALE.to_string(),
            avatar: None,
            nickname: nickname.to_string(), // Shared account: custom nickname
            is_away: false,
            status: None,
        })
        .await
        .expect("Failed to add shared user to UserManager")
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

/// Helper to drain broadcast messages from the channel until a response is found
///
/// In tests, all users share the same `tx` channel, so broadcast messages (like
/// `UserMessage`) arrive in the channel before the response (like `UserMessageResponse`).
/// This helper drains all broadcast messages and returns the first response message.
///
/// The `is_response` predicate should return true for the response message type you expect.
pub fn read_channel_response<F>(test_ctx: &mut TestContext, is_response: F) -> ServerMessage
where
    F: Fn(&ServerMessage) -> bool,
{
    loop {
        let msg = test_ctx
            .rx
            .try_recv()
            .expect("No response message found in channel")
            .0;
        if is_response(&msg) {
            return msg;
        }
        // Otherwise, it's a broadcast - skip it and keep looking
    }
}
