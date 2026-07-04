//! Connection form state

use nexus_common::DEFAULT_PORT;

use crate::types::ui::ActivePanel;

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
    /// Origin panel to restore on Cancel. `None` = no return target
    /// (default-state, e.g. app boot or post-disconnect-of-last) and
    /// the Cancel button is hidden in this case. `Some(panel)` = the
    /// form was opened from that panel (tracker click, server-list
    /// "+" entrypoint, etc.) and Cancel returns there.
    pub connect_origin: Option<ActivePanel>,
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
            connect_origin: None,
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
            .field("password", &nexus_common::REDACTED)
            .field("nickname", &self.nickname)
            .field("fingerprint", &self.fingerprint)
            .field("error", &self.error)
            .field("is_connecting", &self.is_connecting)
            .field("add_bookmark", &self.add_bookmark)
            .field("connect_origin", &self.connect_origin)
            .finish()
    }
}

impl ConnectionFormState {
    /// Wipe all session state — input fields, the error banner, the
    /// `add_bookmark` checkbox, and the in-flight `is_connecting`
    /// flag — so the next summon starts fresh. The network layer
    /// still toggles `is_connecting` back on/off across an actual
    /// connect attempt; clearing it here covers the case where a new
    /// summon arrives mid-flight (e.g. user clicks a tracker row
    /// while a previous Connect is still running) so the Connect
    /// button isn't left stuck-disabled.
    ///
    /// `connect_origin` is preserved here since it's the layout's
    /// modal-like gate; Cancel / Connect success clear it explicitly.
    pub fn clear(&mut self) {
        self.server_name.clear();
        self.server_address.clear();
        self.port = DEFAULT_PORT;
        self.username.clear();
        self.password.clear();
        self.nickname.clear();
        self.fingerprint.clear();
        self.error = None;
        self.is_connecting = false;
        self.add_bookmark = false;
    }
}
