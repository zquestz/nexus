//! Connection form state

use nexus_common::DEFAULT_PORT;

// =============================================================================
// Connection Form State
// =============================================================================

/// Connection form state (not persisted)
#[derive(Clone)]
pub struct ConnectionFormState {
    /// Optional display name for connection
    pub server_name: String,
    /// Server address (IPv4 or IPv6)
    pub server_address: String,
    /// Server port number
    pub port: u16,
    /// Username for authentication
    pub username: String,
    /// Password for authentication
    pub password: String,
    /// Nickname for shared account authentication
    pub nickname: String,
    /// Optional TLS certificate fingerprint pin (canonical 95-char form).
    /// Empty = unpinned (TOFU on first connect). Non-empty = Stage 1 pin
    /// enforced before login.
    pub fingerprint: String,
    /// Connection error message
    pub error: Option<String>,
    /// Whether a connection attempt is currently in progress
    pub is_connecting: bool,
    /// Whether to save this connection as a bookmark on successful connect
    pub add_bookmark: bool,
}

impl Default for ConnectionFormState {
    fn default() -> Self {
        Self {
            server_name: String::new(),
            server_address: String::new(),
            port: DEFAULT_PORT,
            username: String::new(),
            password: String::new(),
            nickname: String::new(),
            fingerprint: String::new(),
            error: None,
            is_connecting: false,
            add_bookmark: false,
        }
    }
}

impl std::fmt::Debug for ConnectionFormState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ConnectionFormState")
            .field("server_name", &self.server_name)
            .field("server_address", &self.server_address)
            .field("port", &self.port)
            .field("username", &self.username)
            .field("password", &"[REDACTED]")
            .field("nickname", &self.nickname)
            .field("fingerprint", &self.fingerprint)
            .field("error", &self.error)
            .field("is_connecting", &self.is_connecting)
            .field("add_bookmark", &self.add_bookmark)
            .finish()
    }
}

impl ConnectionFormState {
    /// Clear all form fields
    pub fn clear(&mut self) {
        self.server_name.clear();
        self.server_address.clear();
        self.port = DEFAULT_PORT;
        self.username.clear();
        self.password.clear();
        self.nickname.clear();
        self.fingerprint.clear();
    }
}
