//! Password change form state

use super::super::ActivePanel;

// =============================================================================
// Password Change State
// =============================================================================

/// Password change form state (for User Info panel)
///
/// Tracks the form fields when a user is changing their own password.
#[derive(Clone)]
pub struct PasswordChangeState {
    /// Current password (required for verification)
    pub current_password: String,
    /// New password
    pub new_password: String,
    /// Confirm new password (must match new_password)
    pub confirm_password: String,
    /// Whether the current password field shows plain text instead of dots
    pub current_password_revealed: bool,
    /// Whether the new password field shows plain text instead of dots
    pub new_password_revealed: bool,
    /// Whether the confirm password field shows plain text instead of dots
    pub confirm_password_revealed: bool,
    /// Error message to display
    pub error: Option<String>,
    /// Panel to return to after cancel/success (e.g., UserInfo)
    pub return_to_panel: Option<ActivePanel>,
    /// Whether a password change request is in flight
    pub is_submitting: bool,
}

impl std::fmt::Debug for PasswordChangeState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PasswordChangeState")
            .field("current_password", &nexus_common::REDACTED)
            .field("new_password", &nexus_common::REDACTED)
            .field("confirm_password", &nexus_common::REDACTED)
            .field("current_password_revealed", &self.current_password_revealed)
            .field("new_password_revealed", &self.new_password_revealed)
            .field("confirm_password_revealed", &self.confirm_password_revealed)
            .field("error", &self.error)
            .field("return_to_panel", &self.return_to_panel)
            .field("is_submitting", &self.is_submitting)
            .finish()
    }
}

impl PasswordChangeState {
    /// Create a new empty password change state with a return panel
    pub fn new(return_to_panel: Option<ActivePanel>) -> Self {
        Self {
            current_password: String::new(),
            new_password: String::new(),
            confirm_password: String::new(),
            current_password_revealed: false,
            new_password_revealed: false,
            confirm_password_revealed: false,
            error: None,
            return_to_panel,
            is_submitting: false,
        }
    }
}
