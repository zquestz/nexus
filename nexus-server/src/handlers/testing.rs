//! Shared test utilities for handler tests
//!
//! Pitfall: handlers like `user_update` / `group_update` re-resolve a
//! target's effective permissions from the DB and overwrite the session
//! cache. A permission granted only in `NewSessionParams.permissions`
//! (never in the DB row) gets silently wiped on resync. To let a session
//! *receive* a handler's broadcast, grant in both `CreateUserParams`
//! (DB) and `NewSessionParams` (cache); the `login_*` helpers keep them
//! in sync automatically.

use std::collections::{HashMap, HashSet};
use std::fs;
use std::net::SocketAddr;
use std::path::Path;
use std::sync::{Arc, LazyLock, RwLock};
use std::time::Duration;

use tempfile::TempDir;

use tokio::io::{BufReader, Sink};
use tokio::net::{TcpListener, TcpStream};
use tokio::time::timeout;

use nexus_common::framing::{FrameReader, FrameWriter, MessageId};
use nexus_common::io::read_server_message as io_read_server_message;
use nexus_common::protocol::ServerMessage;
use nexus_common::validators::resolve_bandwidth_weight;

use super::{DirectWriter, HandlerContext};
use crate::channels::ChannelManager;
use crate::connection_tracker::ConnectionTracker;
use crate::db::{CreateUserParams, Database, Permissions};
use crate::egress::task::{
    DEFAULT_EGRESS_COMMAND_QUEUE_CAPACITY, EgressCommandRx, EgressHandle, EgressSettingsCommandRx,
};
use crate::files::{FileActivityMap, FileIndex};
use crate::ip_rule_cache::{IpRuleCache, IpRuleState};
use crate::scheduler::ConnectionId;
use crate::transfers::TransferRegistry;
use crate::users::UserManager;
use crate::users::user::{ConnectionWriter, NewSessionParams, SessionRx};
use crate::voice::VoiceRegistry;
use nexus_common::address::normalize_socket_addr;

pub const DEFAULT_TEST_LOCALE: &str = "en";

/// Fake server fingerprint (realistic 95-char shape; tests don't validate format).
pub const TEST_FINGERPRINT: &str = "AA:BB:CC:DD:EE:FF:00:11:22:33:44:55:66:77:88:99:AA:BB:CC:DD:EE:FF:00:11:22:33:44:55:66:77:88:99";

type TestWriteHalf = tokio::net::tcp::OwnedWriteHalf;
type TestReadHalf = tokio::net::tcp::OwnedReadHalf;

// Argon2 hashing is intentionally slow; handler tests log in hundreds of times.
// Caching hashes per unique password (most tests reuse "password") avoids the cost.
static PASSWORD_HASH_CACHE: LazyLock<RwLock<HashMap<String, String>>> =
    LazyLock::new(|| RwLock::new(HashMap::new()));

pub fn get_cached_password_hash(password: &str) -> String {
    {
        let cache = PASSWORD_HASH_CACHE.read().unwrap();
        if let Some(hash) = cache.get(password) {
            return hash.clone();
        }
    }

    let hash = crate::db::hash_password(
        password,
        nexus_common::validators::PasswordStrength::Weak,
        true,
    )
    .expect("Password hashing failed in test");

    {
        let mut cache = PASSWORD_HASH_CACHE.write().unwrap();
        cache.entry(password.to_string()).or_insert(hash).clone()
    }
}

/// Owns all resources needed for handler testing.
pub struct TestContext {
    pub frame_reader: FrameReader<BufReader<TestReadHalf>>,
    pub frame_writer: FrameWriter<TestWriteHalf>,
    pub user_manager: UserManager,
    pub db: Database,
    pub tx: ConnectionWriter,
    pub egress: EgressHandle,
    pub egress_connection_id: ConnectionId,
    pub peer_addr: SocketAddr,
    pub rx: SessionRx,
    pub _egress_command_rx: EgressCommandRx,
    pub egress_settings_rx: EgressSettingsCommandRx,
    pub message_id: MessageId,
    pub file_root: Option<&'static Path>,
    pub connection_tracker: Arc<ConnectionTracker>,
    pub ip_rule_cache: Arc<IpRuleState>,
    pub file_index: Arc<FileIndex>,
    pub file_activity: Arc<FileActivityMap>,
    pub channel_manager: ChannelManager,
    pub transfer_registry: Arc<TransferRegistry>,
    pub voice_registry: VoiceRegistry,
    pub tracker_manager: crate::tracker::TrackerManager,
    pub flood_config: Arc<crate::flood::FloodConfig>,
    /// Keep temp dir alive for tests that use file areas
    _temp_dir: TempDir,
}

impl TestContext {
    pub fn handler_context(&mut self) -> HandlerContext<'_, TestWriteHalf> {
        HandlerContext {
            writer: DirectWriter::new(&mut self.frame_writer),
            peer_addr: self.peer_addr,
            user_manager: &self.user_manager,
            db: &self.db,
            tx: &self.tx,
            egress: &self.egress,
            egress_connection_id: self.egress_connection_id,
            locale: DEFAULT_TEST_LOCALE,
            message_id: self.message_id,
            file_root: self.file_root,
            transfer_port: nexus_common::DEFAULT_TRANSFER_PORT,
            transfer_websocket_port: Some(nexus_common::DEFAULT_TRANSFER_WEBSOCKET_PORT),
            connection_tracker: self.connection_tracker.clone(),
            ip_rule_cache: self.ip_rule_cache.clone(),
            file_index: self.file_index.clone(),
            file_activity: self.file_activity.clone(),
            channel_manager: &self.channel_manager,
            transfer_registry: self.transfer_registry.clone(),
            voice_registry: &self.voice_registry,
            tracker_manager: &self.tracker_manager,
            fingerprint: TEST_FINGERPRINT,
            flood_config: self.flood_config.clone(),
        }
    }
}

/// Build a `TestContext` over a real loopback TCP socket pair.
pub async fn create_test_context() -> TestContext {
    let pool = sqlx::SqlitePool::connect(":memory:")
        .await
        .expect("Failed to create test database");
    sqlx::migrate!("./migrations")
        .run(&pool)
        .await
        .expect("Failed to run migrations");

    let db = Database::new(pool);

    // Empty auto_join_channels keeps channels out of LoginResponse during login tests.
    db.config
        .set_auto_join_channels("")
        .await
        .expect("Failed to clear auto_join_channels");

    // Weak strength so tests can use simple passwords.
    db.config
        .set_min_password_strength(nexus_common::validators::PasswordStrength::Weak)
        .await
        .expect("Failed to set min_password_strength");
    let user_manager = UserManager::new();

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    let client_handle = tokio::spawn(async move { TcpStream::connect(addr).await.unwrap() });

    // Fold IPv4-mapped IPv6 like the production accept loops, so test sessions
    // land in states production can actually produce.
    let (server_stream, peer_addr) = listener.accept().await.unwrap();
    let peer_addr = normalize_socket_addr(peer_addr);
    let (_read_half, write_half) = server_stream.into_split();
    let frame_writer = FrameWriter::new(write_half);

    let client = client_handle.await.unwrap();
    let (client_read_half, _client_write_half) = client.into_split();
    let buf_reader = BufReader::new(client_read_half);
    let frame_reader = FrameReader::new(buf_reader);

    // Keep rx alive to prevent channel closure.
    let (tx, rx) = ConnectionWriter::channel();
    let (egress_tx, egress_command_rx) =
        tokio::sync::mpsc::channel(DEFAULT_EGRESS_COMMAND_QUEUE_CAPACITY);
    let (egress_settings_tx, egress_settings_rx) = tokio::sync::mpsc::unbounded_channel();
    let egress = EgressHandle::new(egress_tx, egress_settings_tx);
    let egress_connection_id = ConnectionId::new(1);

    let message_id = MessageId::from_bytes(b"000000000000").expect("valid hex test message ID");

    // Unlimited connections by default.
    let connection_tracker = Arc::new(ConnectionTracker::new(0, 0));

    let ip_rule_cache = Arc::new(IpRuleState::new(IpRuleCache::new()));

    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let file_index = Arc::new(FileIndex::new(temp_dir.path(), temp_dir.path()));
    let file_activity = Arc::new(FileActivityMap::new());

    let channel_manager = ChannelManager::new(db.channels.clone(), user_manager.clone());

    let transfer_registry = Arc::new(TransferRegistry::new());

    let voice_registry = VoiceRegistry::new();

    // No rows seeded, so tracker tasks don't run; manager-exercising tests
    // populate it via spawn() / replace().
    let tracker_context = Arc::new(crate::tracker::TrackerContext {
        db: db.clone(),
        user_manager: user_manager.clone(),
        server_fingerprint: TEST_FINGERPRINT,
        server_port: nexus_common::DEFAULT_PORT,
        server_websocket_port: None,
    });
    let tracker_manager = crate::tracker::TrackerManager::new(tracker_context);

    let flood_config = Arc::new(crate::flood::FloodConfig::new(5, 20));

    TestContext {
        frame_reader,
        frame_writer,
        user_manager,
        db,
        tx,
        egress,
        egress_connection_id,
        peer_addr,
        rx,
        _egress_command_rx: egress_command_rx,
        egress_settings_rx,
        message_id,
        file_root: None,
        connection_tracker,
        ip_rule_cache,
        file_index,
        file_activity,
        channel_manager,
        transfer_registry,
        voice_registry,
        tracker_manager,
        flood_config,
        _temp_dir: temp_dir,
    }
}

/// Create a user + session, returning the session_id.
pub async fn login_user(
    test_ctx: &mut TestContext,
    username: &str,
    password: &str,
    permissions: &[crate::db::Permission],
    is_admin: bool,
) -> u32 {
    login_user_with_features(test_ctx, username, password, permissions, is_admin, vec![]).await
}

/// `login_user` variant placing the session on a specific IP (ban-scenario tests).
pub async fn login_user_from_ip(
    test_ctx: &mut TestContext,
    username: &str,
    password: &str,
    permissions: &[crate::db::Permission],
    is_admin: bool,
    ip: &str,
) -> u32 {
    let hashed = get_cached_password_hash(password);

    let mut perms = Permissions::new();
    for perm in permissions {
        perms.permissions.insert(*perm);
    }

    let user = test_ctx
        .db
        .users
        .create_user(CreateUserParams {
            username,
            hashed_password: &hashed,
            is_admin,
            is_shared: false,
            enabled: true,
            permissions: &perms,
            group_id: None,
            revokes: &[],
            bandwidth_weight: None,
        })
        .await
        .unwrap();

    // Fold IPv4-mapped IPv6 like production's accept loops.
    let addr: SocketAddr = format!("{}:12345", ip).parse().expect("valid IP address");
    let addr = normalize_socket_addr(addr);

    test_ctx
        .user_manager
        .add_user(NewSessionParams {
            session_id: 0, // Will be assigned by add_user
            user_id: user.id,
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
            group_id: None,
            group_name: None,
            // No override, no group: resolve via the shared resolver so
            // fixtures stay in sync with production precedence.
            bandwidth_weight: resolve_bandwidth_weight(None, None, is_admin),
            bandwidth_weight_override: None,
            last_activity: std::time::Instant::now(),
        })
        .await
        .expect("Failed to add user to UserManager")
}

/// `login_user` variant that sets session feature flags.
pub async fn login_user_with_features(
    test_ctx: &mut TestContext,
    username: &str,
    password: &str,
    permissions: &[crate::db::Permission],
    is_admin: bool,
    features: Vec<String>,
) -> u32 {
    let hashed = get_cached_password_hash(password);

    let mut perms = Permissions::new();
    for perm in permissions {
        perms.permissions.insert(*perm);
    }

    let user = test_ctx
        .db
        .users
        .create_user(CreateUserParams {
            username,
            hashed_password: &hashed,
            is_admin,
            is_shared: false,
            enabled: true,
            permissions: &perms,
            group_id: None,
            revokes: &[],
            bandwidth_weight: None,
        })
        .await
        .unwrap();

    test_ctx
        .user_manager
        .add_user(NewSessionParams {
            session_id: 0, // Will be assigned by add_user
            user_id: user.id,
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
            group_id: None,
            group_name: None,
            // See note in login_user_from_ip.
            bandwidth_weight: resolve_bandwidth_weight(None, None, is_admin),
            bandwidth_weight_override: None,
            last_activity: std::time::Instant::now(),
        })
        .await
        .expect("Failed to add user to UserManager")
}

/// Like `login_user`, but gives the session its own fresh channel (not the
/// shared `test_ctx.tx`) and returns the `rx`. Lets a test assert what this
/// specific session received — e.g. verifying sender-exclusion in broadcasts.
pub async fn login_observer_user(
    test_ctx: &mut TestContext,
    username: &str,
    password: &str,
    permissions: &[crate::db::Permission],
    features: Vec<String>,
) -> (u32, SessionRx) {
    let hashed = get_cached_password_hash(password);

    let mut perms = Permissions::new();
    for perm in permissions {
        perms.permissions.insert(*perm);
    }

    let user = test_ctx
        .db
        .users
        .create_user(CreateUserParams {
            username,
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

    let (tx, rx) = ConnectionWriter::channel();

    let session_id = test_ctx
        .user_manager
        .add_user(NewSessionParams {
            session_id: 0, // Will be assigned by add_user
            user_id: user.id,
            username: username.to_string(),
            is_admin: false,
            is_shared: false,
            permissions: perms.permissions.clone(),
            address: test_ctx.peer_addr,
            created_at: user.created_at,
            tx,
            features,
            locale: DEFAULT_TEST_LOCALE.to_string(),
            avatar: None,
            nickname: username.to_string(),
            is_away: false,
            status: None,
            group_id: None,
            group_name: None,
            bandwidth_weight: resolve_bandwidth_weight(None, None, false),
            bandwidth_weight_override: None,
            last_activity: std::time::Instant::now(),
        })
        .await
        .expect("Failed to add observer user to UserManager");

    (session_id, rx)
}

/// Create a shared account + session with a distinct `nickname`.
pub async fn login_shared_user(
    test_ctx: &mut TestContext,
    account_username: &str,
    password: &str,
    nickname: &str,
    permissions: &[crate::db::Permission],
) -> u32 {
    let hashed = get_cached_password_hash(password);

    let mut perms = Permissions::new();
    for perm in permissions {
        perms.permissions.insert(*perm);
    }

    let user = test_ctx
        .db
        .users
        .create_user(CreateUserParams {
            username: account_username,
            hashed_password: &hashed,
            is_admin: false,
            is_shared: true,
            enabled: true,
            permissions: &perms,
            group_id: None,
            revokes: &[],
            bandwidth_weight: None,
        })
        .await
        .unwrap();

    test_ctx
        .user_manager
        .add_user(NewSessionParams {
            session_id: 0, // Will be assigned by add_user
            user_id: user.id,
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
            nickname: nickname.to_string(),
            is_away: false,
            status: None,
            group_id: None,
            group_name: None,
            bandwidth_weight: resolve_bandwidth_weight(None, None, false),
            bandwidth_weight_override: None,
            last_activity: std::time::Instant::now(),
        })
        .await
        .expect("Failed to add shared user to UserManager")
}

/// Iteration count for concurrent-handler regression loops: defensive coverage
/// so a removed `lock_lifecycle()` call fails reliably across interleavings.
pub const CONCURRENT_LIFECYCLE_ITERATIONS: usize = 25;

/// Like `handler_context`, but with a caller-owned writer (typically a `Sink`)
/// so multiple `HandlerContext`s can coexist for concurrent-handler tests
/// (`tokio::join!`). Responses are discarded; tests assert on side effects.
pub fn concurrent_handler_context<'a>(
    test_ctx: &'a TestContext,
    writer: &'a mut nexus_common::framing::FrameWriter<Sink>,
) -> HandlerContext<'a, Sink> {
    HandlerContext {
        writer: DirectWriter::new(writer),
        peer_addr: test_ctx.peer_addr,
        user_manager: &test_ctx.user_manager,
        db: &test_ctx.db,
        tx: &test_ctx.tx,
        egress: &test_ctx.egress,
        egress_connection_id: test_ctx.egress_connection_id,
        locale: DEFAULT_TEST_LOCALE,
        message_id: test_ctx.message_id,
        file_root: test_ctx.file_root,
        transfer_port: nexus_common::DEFAULT_TRANSFER_PORT,
        transfer_websocket_port: Some(nexus_common::DEFAULT_TRANSFER_WEBSOCKET_PORT),
        connection_tracker: test_ctx.connection_tracker.clone(),
        ip_rule_cache: test_ctx.ip_rule_cache.clone(),
        file_index: test_ctx.file_index.clone(),
        file_activity: test_ctx.file_activity.clone(),
        channel_manager: &test_ctx.channel_manager,
        transfer_registry: test_ctx.transfer_registry.clone(),
        voice_registry: &test_ctx.voice_registry,
        tracker_manager: &test_ctx.tracker_manager,
        fingerprint: TEST_FINGERPRINT,
        flood_config: test_ctx.flood_config.clone(),
    }
}

/// Assert the lifecycle invariant `TrackerManager::lock_lifecycle` preserves:
/// enabled DB rows ⇔ manager handles (disabled rows have none).
pub async fn assert_tracker_db_and_manager_consistent(test_ctx: &TestContext) {
    let rows = test_ctx
        .db
        .trackers
        .list_all()
        .await
        .expect("list_all should not fail in test");
    let statuses = test_ctx.tracker_manager.status_all();

    let enabled_ids: HashSet<i64> = rows.iter().filter(|r| r.enabled).map(|r| r.id).collect();
    let manager_ids: HashSet<i64> = statuses.keys().copied().collect();

    assert_eq!(
        enabled_ids, manager_ids,
        "DB-enabled tracker ids must match manager handle ids; \
         enabled in DB: {enabled_ids:?}, in manager: {manager_ids:?}",
    );
}

/// Read one `ServerMessage` from the frame reader (keeps the reader's buffer).
pub async fn read_server_message(test_ctx: &mut TestContext) -> ServerMessage {
    io_read_server_message(&mut test_ctx.frame_reader)
        .await
        .expect("Failed to read message")
        .expect("Connection closed unexpectedly")
        .message
}

/// Read until the first `LoginResponse` (5s timeout panic).
pub async fn read_login_response(test_ctx: &mut TestContext) -> ServerMessage {
    read_server_message_matching(test_ctx, |msg| {
        matches!(msg, ServerMessage::LoginResponse { .. })
    })
    .await
}

/// Read until `predicate` matches, discarding earlier messages (5s timeout panic).
pub async fn read_server_message_matching<F>(
    test_ctx: &mut TestContext,
    predicate: F,
) -> ServerMessage
where
    F: Fn(&ServerMessage) -> bool,
{
    let result = timeout(Duration::from_secs(5), async {
        loop {
            let msg = read_server_message(test_ctx).await;
            if predicate(&msg) {
                return msg;
            }
        }
    })
    .await;

    result.expect("Timed out waiting for matching server message")
}

/// Drain broadcasts from the shared `tx` until `is_response` matches.
///
/// In tests every session shares one `tx`, so broadcasts (e.g. `UserMessage`)
/// queue ahead of the response (e.g. `UserMessageResponse`).
pub fn read_channel_response<F>(test_ctx: &mut TestContext, is_response: F) -> ServerMessage
where
    F: Fn(&ServerMessage) -> bool,
{
    loop {
        let (msg, _) = test_ctx
            .rx
            .try_recv()
            .expect("No response message found in channel")
            .expect_message();
        if is_response(&msg) {
            return msg;
        }
    }
}

// File-area helpers: handle the `Box::leak` boilerplate and set
// `test_ctx.file_root`. basic ⊂ with_uploads ⊂ full.

/// `shared/` + `users/` only. Returns the `TempDir` to keep alive for the test.
pub fn setup_file_area_basic(test_ctx: &mut TestContext) -> TempDir {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let file_root: &'static Path = Box::leak(temp_dir.path().to_path_buf().into_boxed_path());
    test_ctx.file_root = Some(file_root);

    fs::create_dir_all(temp_dir.path().join("shared")).expect("Failed to create shared");
    fs::create_dir_all(temp_dir.path().join("users")).expect("Failed to create users");

    temp_dir
}

/// Basic + `shared/Uploads [NEXUS-UL]/`.
pub fn setup_file_area_with_uploads(test_ctx: &mut TestContext) -> TempDir {
    let temp_dir = setup_file_area_basic(test_ctx);

    fs::create_dir_all(temp_dir.path().join("shared/Uploads [NEXUS-UL]"))
        .expect("Failed to create upload folder");

    temp_dir
}

/// Uploads + a `[NEXUS-DB]` dropbox + sample files (`Documents/file.txt`, `readme.txt`).
pub fn setup_file_area_full(test_ctx: &mut TestContext) -> TempDir {
    let temp_dir = setup_file_area_with_uploads(test_ctx);
    let root = temp_dir.path();

    fs::create_dir_all(root.join("shared/Submissions [NEXUS-DB]"))
        .expect("Failed to create dropbox");

    fs::create_dir_all(root.join("shared/Documents")).expect("Failed to create Documents");
    fs::write(root.join("shared/Documents/file.txt"), "doc content")
        .expect("Failed to create file");

    fs::write(root.join("shared/readme.txt"), "test content").expect("Failed to create readme");

    temp_dir
}
