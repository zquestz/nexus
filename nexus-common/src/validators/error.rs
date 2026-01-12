//! Error message validation constants
//!
//! Constants for error message length limits used in protocol messages.

/// Maximum length for error messages in protocol responses (bytes)
///
/// This limit applies to all error fields in response messages like
/// `ChatJoinResponse`, `ChatLeaveResponse`, `UserCreateResponse`, etc.
pub const MAX_ERROR_LENGTH: usize = 2048;
