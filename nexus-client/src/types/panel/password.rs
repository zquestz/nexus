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
            .field("current_password", &"[REDACTED]")
            .field("new_password", &"[REDACTED]")
            .field("confirm_password", &"[REDACTED]")
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
            error: None,
            return_to_panel,
            is_submitting: false,
        }
    }
}
