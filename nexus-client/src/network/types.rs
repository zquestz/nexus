//! Network module type aliases and internal types

use tokio::io::BufReader;
use tokio::net::TcpStream;

use nexus_common::framing::{FrameReader, FrameWriter};

/// Parameters for connecting to a server
pub struct ConnectionParams {
    /// Server address (IPv4 or IPv6)
    pub server_address: String,
    /// Server port
    pub port: u16,
    /// Username for authentication
    pub username: String,
    /// Password for authentication
    pub password: String,
    /// Nickname for shared account logins
    pub nickname: Option<String>,
    /// Locale for server messages
    pub locale: String,
    /// Avatar data URI
    pub avatar: Option<String>,
    /// Unique connection identifier
    pub connection_id: usize,
}

/// Type alias for TLS stream
pub type TlsStream = tokio_rustls::client::TlsStream<TcpStream>;

/// Type alias for TLS stream read half with buffering and framing
pub type Reader = FrameReader<BufReader<tokio::io::ReadHalf<TlsStream>>>;

/// Type alias for TLS stream write half with framing
pub type Writer = FrameWriter<tokio::io::WriteHalf<TlsStream>>;

/// Login information returned from the server
pub struct LoginInfo {
    pub session_id: u32,
    pub is_admin: bool,
    pub permissions: Vec<String>,
    pub server_name: Option<String>,
    pub server_description: Option<String>,
    pub server_version: Option<String>,
    pub server_image: String,
    pub chat_topic: Option<String>,
    pub chat_topic_set_by: Option<String>,
    pub max_connections_per_ip: Option<u32>,
    pub locale: String,
}
