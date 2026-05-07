//! Network module type aliases and internal types

use std::pin::Pin;
use std::task::{Context, Poll};

use tokio::io::{AsyncRead, AsyncWrite, BufReader, ReadBuf};
use tokio::net::TcpStream;
use tokio_rustls::rustls::ClientConnection;
use tokio_socks::tcp::Socks5Stream;

use nexus_common::framing::{FrameReader, FrameWriter};
use nexus_common::protocol::ChannelJoinInfo;
use nexus_common::tracker_protocol::ServerEntry;
use nexus_common::validators::PasswordStrength;

use crate::i18n::{t, t_args};

use super::CONNECTION_TIMEOUT;

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
/// Connect-flow error categories.
///
/// Distinguishes the two pre-login fingerprint failures (which need
/// dialog handling) from connect-phase failures (typed by which phase
/// — TCP / TLS / proxy / fingerprint extraction — so callers can
/// differentiate "tracker is down" from "tracker accepted TCP then
/// dropped" without parsing localized strings) from post-connect
/// errors (`Other`, used after `establish_connection` succeeds for
/// the handshake/login/framing flow on the BBS port).
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
    /// Address parses but the system resolver rejected it (IDN parse
    /// or `to_socket_addrs` failure). User likely typed a malformed
    /// hostname.
    InvalidAddress { address: String, error: String },
    /// Address resolved but to zero records. DNS knows nothing about
    /// this name.
    CouldNotResolve { address: String },
    /// TCP `connect()` timed out (peer never SYN-ACK'd in
    /// [`super::constants::CONNECTION_TIMEOUT`]).
    TcpTimeout,
    /// TCP `connect()` returned an error (refused, no route, etc.).
    TcpFailed { error: String },
    /// TLS handshake failed AFTER TCP succeeded. Reliable signal that
    /// the peer accepted the connection then dropped — typically
    /// rate-limiting, capacity, or active refusal at the application
    /// layer. Distinct from `TcpFailed` because the peer DID answer
    /// at the IP layer.
    TlsHandshake { error: String },
    /// Cert chain extraction returned empty. Should not happen in
    /// practice — TLS handshake succeeded but rustls produced no
    /// peer certs.
    NoCertificates,
    /// SOCKS5 proxy `connect()` timed out.
    ProxyTimeout,
    /// SOCKS5 proxy `connect()` returned an error.
    ProxyFailed { error: String },
    /// Post-`establish_connection` error from the BBS-port handshake /
    /// login / framing flow. The string is already localized — emitted
    /// by the protocol-layer code path that produced it.
    Other(String),
}

impl ConnectError {
    /// Translate a non-dialog `ConnectError` to a localized display
    /// string for forms / error banners. Each variant maps to the same
    /// locale key it was previously formatted with inside
    /// `establish_connection`, so user-visible messages are unchanged
    /// — the typed errors just move the formatting from the source
    /// to the renderer.
    ///
    /// `FingerprintMismatch` and `FingerprintInterception` should be
    /// matched explicitly upstream and routed to dialogs; their
    /// fall-through here renders a generic message as a fail-safe.
    pub(crate) fn to_localized_string(&self) -> String {
        match self {
            ConnectError::FingerprintMismatch(_) => t("err-fingerprint-mismatch"),
            ConnectError::FingerprintInterception(_) => t("err-fingerprint-interception"),
            ConnectError::InvalidAddress { address, error } => t_args(
                "err-invalid-address",
                &[("address", address), ("error", error)],
            ),
            ConnectError::CouldNotResolve { address } => {
                t_args("err-could-not-resolve", &[("address", address)])
            }
            ConnectError::TcpTimeout => t_args(
                "err-connection-timeout",
                &[("seconds", &CONNECTION_TIMEOUT.as_secs().to_string())],
            ),
            ConnectError::TcpFailed { error } => {
                t_args("err-connection-failed", &[("error", error)])
            }
            ConnectError::TlsHandshake { error } => {
                t_args("err-tls-handshake-failed", &[("error", error)])
            }
            ConnectError::NoCertificates => t("err-no-certificates-in-chain"),
            ConnectError::ProxyTimeout => t_args(
                "err-proxy-connection-timeout",
                &[("seconds", &CONNECTION_TIMEOUT.as_secs().to_string())],
            ),
            ConnectError::ProxyFailed { error } => {
                t_args("err-proxy-connection-failed", &[("error", error)])
            }
            ConnectError::Other(s) => s.clone(),
        }
    }
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

/// Caller-built parameters for a one-shot tracker query.
///
/// Mirrors [`ConnectionParams`] in spirit but targets the tracker port
/// instead of the BBS port. The query connects, performs the tracker
/// handshake, sends one `TrackerServerList` request, reads the
/// response, and disconnects (the tracker also closes its side per
/// spec). Built by the dispatching handler so the query function
/// itself stays pure. The dispatching handler captures the
/// `tracker_id` separately into the result-message closure, so the
/// id isn't part of this struct.
#[derive(Clone)]
pub struct TrackerQueryParams {
    /// Tracker hostname or IP literal (raw user input from the
    /// tracker config; resolved at connect time).
    pub address: String,
    /// Tracker TCP port.
    pub port: u16,
    /// Optional registration password (gated trackers).
    pub password: Option<String>,
    /// Stored TOFU pin. `Some` enforces stage 1 against this value;
    /// `None` is first-connect TOFU (any cert accepted at stage 1, pin
    /// committed by the result handler on success).
    pub expected_fingerprint: Option<String>,
    /// Locale tag passed to the tracker so it can localize any
    /// error responses it returns.
    pub locale: String,
    /// Client crate version, sent as the required `version` field on
    /// `TrackerServerList`. The tracker filters returned entries to
    /// servers semver-compatible with this value.
    pub version: String,
    /// Optional SOCKS5 proxy config. Subject to the same bypass rules
    /// as BBS connections (loopback, Yggdrasil, RFC1918).
    pub proxy: Option<ProxyConfig>,
}

/// Successful tracker query result.
#[derive(Debug, Clone)]
pub struct TrackerQueryOk {
    /// Server entries the tracker advertised, already filtered to
    /// semver-compatible versions per the spec.
    pub entries: Vec<ServerEntry>,
    /// Fingerprint observed from TLS during this query. Used by the
    /// result handler to commit the TOFU pin on first connect.
    pub observed_fingerprint: String,
}

/// Errors a one-shot tracker query can produce. Variants are
/// differentiated by what the panel cache should do on receipt:
/// `Stage1Mismatch` triggers the AcceptFingerprint flow (handler
/// stashes the observed value in `pending_fingerprint`); the others
/// just render localized error strings.
///
/// `establish_connection` flattens TCP and TLS errors into a single
/// `Connection` variant — the underlying helper returns
/// `Result<_, String>` without distinguishing the two.
#[derive(Debug, Clone)]
pub enum TrackerQueryError {
    /// Stage 1: TLS-observed fingerprint differs from the stored pin.
    /// `received` is the observed value; the handler stashes it in
    /// the cache's `pending_fingerprint` so the AcceptFingerprint
    /// dialog can promote it. The "expected" value is the row's
    /// stored pin and the dialog reads it from `Config` directly.
    Stage1Mismatch { received: String },
    /// Stage 2: server-self-reported fingerprint differs from
    /// TLS-observed. Strongly indicates active interception. No
    /// accept path — the dialog shown for this case is informational.
    Stage2Mismatch,
    /// Connect-phase failure from `establish_connection`. Carries the
    /// typed `ConnectError` so the panel renderer can dispatch on the
    /// phase: TCP-phase failures (refused, timeout, DNS) say "tracker
    /// is unreachable"; TLS-phase failures (EOF, handshake errors)
    /// say "tracker refused the connection" because TCP succeeding
    /// then TLS failing is the reliable signal that the peer
    /// rate-limited or actively dropped after accepting at the IP
    /// layer.
    Connection(ConnectError),
    /// Handshake request/response failure: rejected by tracker,
    /// timeout, malformed `HandshakeResponse`.
    Handshake(String),
    /// Tracker returned a typed `success: false` response.
    /// `kind` carries the canonical `error_kind` string when the
    /// tracker provided one (e.g. `unauthorized`, `rate_limited`,
    /// `capacity`). `None` means the tracker omitted the field.
    Rejected {
        kind: Option<String>,
        message: String,
    },
    /// Other protocol-level error: unexpected frame type, generic
    /// `ServerMessage::Error`, disconnect mid-flow.
    Protocol(String),
    /// Response shape was structurally invalid (failed to parse).
    MalformedResponse(String),
}
