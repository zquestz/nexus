//! Network module type aliases and internal types

use tokio::io::BufReader;
use tokio::net::TcpStream;

/// Type alias for TLS stream
pub type TlsStream = tokio_rustls::client::TlsStream<TcpStream>;

/// Type alias for TLS stream read half with buffering
pub type Reader = BufReader<tokio::io::ReadHalf<TlsStream>>;

/// Type alias for TLS stream write half
pub type Writer = tokio::io::WriteHalf<TlsStream>;

/// Login information returned from the server
pub struct LoginInfo {
    pub session_id: u32,
    pub is_admin: bool,
    pub permissions: Vec<String>,
    pub chat_topic: Option<String>,
    pub chat_topic_set_by: Option<String>,
    pub locale: String,
}
