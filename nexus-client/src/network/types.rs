//! Network module type aliases and internal types

use std::pin::Pin;
use std::task::{Context, Poll};

use tokio::io::{AsyncRead, AsyncWrite, BufReader, ReadBuf};
use tokio::net::TcpStream;
use tokio_rustls::rustls::ClientConnection;
use tokio_socks::tcp::Socks5Stream;

use nexus_common::framing::{FrameReader, FrameWriter};
use nexus_common::protocol::ChannelJoinInfo;

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
    pub session_id: u32,
    pub is_admin: bool,
    pub permissions: Vec<String>,
    pub server_name: Option<String>,
    pub server_description: Option<String>,
    pub server_version: Option<String>,
    pub server_image: String,
    /// Channels the user was auto-joined to on login
    pub channels: Vec<ChannelJoinInfo>,
    pub max_connections_per_ip: Option<u32>,
    pub max_transfers_per_ip: Option<u32>,
    pub file_reindex_interval: Option<u32>,
    /// Persistent channels (space-separated, admin only)
    pub persistent_channels: Option<String>,
    /// Auto-join channels (space-separated, admin only)
    pub auto_join_channels: Option<String>,
    pub transfer_port: u16,
    pub locale: String,
}
