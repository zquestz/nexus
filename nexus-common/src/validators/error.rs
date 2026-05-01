//! Error and response field validation constants
//!
//! Constants for field length limits used in protocol response messages.

/// Maximum length for error messages in protocol responses (bytes)
///
/// This limit applies to all error fields in response messages like
/// `ChatJoinResponse`, `ChatLeaveResponse`, `UserCreateResponse`, etc.
pub const MAX_ERROR_LENGTH: usize = 2048;

/// Maximum length for machine-readable error kind codes (bytes)
///
/// These are short, snake_case identifiers used for programmatic error handling.
/// Examples: "not_found", "permission", "hash_mismatch", "protocol_error",
/// "tracker_fingerprint_intercepted".
///
/// Current longest value is "tracker_fingerprint_intercepted" (31 chars), so
/// 32 sits exactly at that bound. Bumping if a longer kind is added is fine —
/// frame-size budgets recompute at compile time off this constant.
pub const MAX_ERROR_KIND_LENGTH: usize = 32;

/// Maximum length for command field in Error messages (bytes)
///
/// This field contains the message type name that caused the error.
/// Longest message type is "ChatTopicUpdateResponse" (24 chars), so 32 provides margin.
pub const MAX_COMMAND_LENGTH: usize = 32;

/// Maximum length for NewsAction enum variant names (bytes)
///
/// Variants: "Created", "Updated", "Deleted" - all 7 chars.
pub const MAX_NEWS_ACTION_LENGTH: usize = 7;

/// Length of transfer ID field (hex string)
///
/// Transfer IDs are 8 hex characters used for log correlation between
/// client and server during file transfers.
pub const TRANSFER_ID_LENGTH: usize = 8;

/// Maximum length for log level strings in ServerInfo (bytes)
///
/// Values: "none", "error", "warn", "info", "debug" - all 5 chars or less.
pub const MAX_LOG_LEVEL_LENGTH: usize = 5;
