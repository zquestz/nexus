//! Network module type aliases and internal types

use std::pin::Pin;
use std::task::{Context, Poll};

use tokio::io::{AsyncRead, AsyncWrite, BufReader, ReadBuf};
use tokio::net::TcpStream;
use tokio_rustls::rustls::ClientConnection;
use tokio_socks::tcp::Socks5Stream;

use nexus_common::framing::{FrameReader, FrameWriter};
use nexus_common::protocol::ChannelJoinInfo;
use nexus_common::validators::PasswordStrength;

/// SOCKS5 proxy configuration for connections
#[derive(Clone)]
pub struct ProxyConfig {
    /// Proxy server address (hostname or IP)
    pub address: String,

    /// Proxy server port
    pub port: u16,

    /// Optional username for authentication
    pub username: Option<String>,

    /// Optional password for authentication
    pub password: Option<String>,
}

impl std::fmt::Debug for ProxyConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ProxyConfig")
            .field("address", &self.address)
            .field("port", &self.port)
            .field("username", &self.username)
            .field("password", &self.password.as_ref().map(|_| "[REDACTED]"))
            .finish()
    }
}

impl ProxyConfig {
    /// Create from app proxy settings if enabled
    pub fn from_settings(settings: &crate::config::settings::ProxySettings) -> Option<Self> {
        if settings.enabled {
            Some(ProxyConfig {
                address: settings.address.clone(),
                port: settings.port,
                username: settings.username.clone(),
                password: settings.password.clone(),
            })
        } else {
            None
        }
    }
}

/// Parameters for connecting to a server
#[derive(Clone)]
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
    /// Optional SOCKS5 proxy configuration
    pub proxy: Option<ProxyConfig>,
    /// Bookmark's stored TLS fingerprint, if any. Used for the stage-1
    /// (pre-handshake) TOFU check. `None` means no stored fingerprint —
    /// either no bookmark, or a brand-new bookmark that will commit its
    /// fingerprint after stage 2 passes.
    pub expected_fingerprint: Option<String>,
}

impl std::fmt::Debug for ConnectionParams {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ConnectionParams")
            .field("server_address", &self.server_address)
            .field("port", &self.port)
            .field("username", &self.username)
            .field("password", &"[REDACTED]")
            .field("nickname", &self.nickname)
            .field("locale", &self.locale)
            .field("avatar", &self.avatar.as_ref().map(|_| "[data]"))
            .field("connection_id", &self.connection_id)
            .field("proxy", &self.proxy)
            .field("expected_fingerprint", &self.expected_fingerprint)
            .finish()
    }
}

/// Certificate fingerprint mismatch details emitted by `connect_to_server`.
///
/// `connect_to_server` only knows the wire-level inputs (the server address
/// it was asked to connect to, the expected fingerprint it was given, and
/// the fingerprint it actually observed). The handler layer decorates this
/// with bookmark identity and a `ReconnectAction` when queueing a
/// `FingerprintMismatch`.
#[derive(Debug, Clone)]
pub struct FingerprintMismatchDetails {
    /// Expected fingerprint (the value caller passed in `expected_fingerprint`)
    pub expected: String,
    /// Received fingerprint (TLS-observed)
    pub received: String,
    /// Server address (IP or hostname)
    pub server_address: String,
    /// Server port
    pub server_port: String,
}

/// Certificate fingerprint interception detected (TLS-observed vs server-reported mismatch)
#[derive(Debug, Clone)]
pub struct FingerprintInterception {
    /// Server name for display
    pub server_name: String,
    /// Server address for display
    pub server_address: String,
    /// Server port for display
    pub server_port: String,
    /// Fingerprint observed during TLS handshake
    pub tls_fingerprint: String,
    /// Fingerprint reported by server in ServerInfo
    pub server_fingerprint: String,
}

/// Errors returned by `connect_to_server`.
///
/// Distinguishes the two pre-login fingerprint failures (which need dialog
/// handling) from generic connection errors. `Other` carries any preexisting
/// string-based error path (TLS failure, login refused, etc.).
#[derive(Debug, Clone)]
pub enum ConnectError {
    /// Stage-1 mismatch: the TLS-observed fingerprint doesn't match the
    /// bookmark's stored fingerprint. User can accept (cert rotation) or
    /// reject (likely MITM).
    FingerprintMismatch(Box<FingerprintMismatchDetails>),
    /// Stage-2 mismatch: the server's self-reported fingerprint doesn't
    /// match the TLS-observed fingerprint. Active interception in progress.
    /// No accept path; informational only.
    FingerprintInterception(Box<FingerprintInterception>),
    /// Any other connection error (TLS, framing, login refused, etc.).
    Other(String),
}

/// Type alias for TLS stream over direct TCP connection
pub type DirectTlsStream = tokio_rustls::client::TlsStream<TcpStream>;

/// Type alias for TLS stream over SOCKS5 proxy connection
pub type ProxiedTlsStream = tokio_rustls::client::TlsStream<Socks5Stream<TcpStream>>;

/// Unified TLS stream that can be either direct or proxied
pub enum TlsStream {
    /// Direct TLS connection (no proxy)
    Direct(DirectTlsStream),
    /// TLS connection through SOCKS5 proxy
    Proxied(ProxiedTlsStream),
}

impl TlsStream {
    /// Get a reference to the TLS session (for certificate inspection)
    pub fn get_ref(&self) -> (&dyn std::any::Any, &ClientConnection) {
        match self {
            TlsStream::Direct(stream) => {
                let (io, session) = stream.get_ref();
                (io, session)
            }
            TlsStream::Proxied(stream) => {
                let (io, session) = stream.get_ref();
                (io, session)
            }
        }
    }
}

impl AsyncRead for TlsStream {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        match self.get_mut() {
            TlsStream::Direct(stream) => Pin::new(stream).poll_read(cx, buf),
            TlsStream::Proxied(stream) => Pin::new(stream).poll_read(cx, buf),
        }
    }
}

impl AsyncWrite for TlsStream {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<std::io::Result<usize>> {
        match self.get_mut() {
            TlsStream::Direct(stream) => Pin::new(stream).poll_write(cx, buf),
            TlsStream::Proxied(stream) => Pin::new(stream).poll_write(cx, buf),
        }
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        match self.get_mut() {
            TlsStream::Direct(stream) => Pin::new(stream).poll_flush(cx),
            TlsStream::Proxied(stream) => Pin::new(stream).poll_flush(cx),
        }
    }

    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        match self.get_mut() {
            TlsStream::Direct(stream) => Pin::new(stream).poll_shutdown(cx),
            TlsStream::Proxied(stream) => Pin::new(stream).poll_shutdown(cx),
        }
    }
}

/// Type alias for TLS stream read half with buffering and framing
pub type Reader = FrameReader<BufReader<tokio::io::ReadHalf<TlsStream>>>;

/// Type alias for TLS stream write half with framing
pub type Writer = FrameWriter<tokio::io::WriteHalf<TlsStream>>;

/// Login information returned from the server
pub struct LoginInfo {
    pub is_admin: bool,
    /// Database user ID (for ID-based protocol messages)
    pub user_id: Option<i64>,
    /// Server-confirmed nickname (equals username for regular accounts)
    pub nickname: String,
    pub permissions: Vec<String>,
    pub server_name: Option<String>,
    pub server_description: Option<String>,
    /// Public address advertised for `nexus://` URI sharing (from ServerInfo)
    pub public_address: Option<String>,
    pub server_version: Option<String>,
    pub server_image: String,
    /// Channels the user was auto-joined to on login
    pub channels: Vec<ChannelJoinInfo>,
    /// Chat burst limit (max messages in a burst, from ServerInfo)
    pub chat_burst_limit: Option<u32>,
    /// Chat rate limit (messages per minute, from ServerInfo)
    pub chat_rate_limit: Option<u32>,
    pub max_connections_per_ip: Option<u32>,
    pub max_transfers_per_ip: Option<u32>,
    pub file_reindex_interval: Option<u32>,
    /// Persistent channels (space-separated, admin only)
    pub persistent_channels: Option<String>,
    /// Auto-join channels (space-separated, admin only)
    pub auto_join_channels: Option<String>,
    /// Minimum password strength requirement from the server
    pub min_password_strength: PasswordStrength,
    /// Server log level (read-only, from ServerInfo)
    pub log_level: Option<String>,
    pub transfer_port: u16,
    pub locale: String,
}
