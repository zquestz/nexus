//! Shared test utilities for handler tests

use super::HandlerContext;
use crate::db::UserDb;
use crate::users::UserManager;
use nexus_common::protocol::ServerMessage;
use std::net::SocketAddr;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc;

/// Test context that owns all resources needed for handler testing
pub struct TestContext {
    pub client: TcpStream,
    pub write_half: tokio::net::tcp::OwnedWriteHalf,
    pub user_manager: UserManager,
    pub user_db: UserDb,
    pub tx: mpsc::UnboundedSender<ServerMessage>,
    pub peer_addr: SocketAddr,
}

impl TestContext {
    /// Create a HandlerContext from this TestContext
    pub fn handler_context(&mut self) -> HandlerContext {
        HandlerContext {
            writer: &mut self.write_half,
            peer_addr: self.peer_addr,
            user_manager: &self.user_manager,
            user_db: &self.user_db,
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

    let user_db = UserDb::new(pool);
    let user_manager = UserManager::new();

    // Create TCP listener on localhost
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    // Connect client
    let client_handle = tokio::spawn(async move { TcpStream::connect(addr).await.unwrap() });

    // Accept connection
    let (server_stream, peer_addr) = listener.accept().await.unwrap();
    let (_read_half, write_half) = server_stream.into_split();

    let client = client_handle.await.unwrap();

    // Create message channel
    let (tx, _rx) = mpsc::unbounded_channel();

    TestContext {
        client,
        write_half,
        user_manager,
        user_db,
        tx,
        peer_addr,
    }
}
