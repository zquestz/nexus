//! Application-wide constants
//!
//! Shared constants used across multiple modules.

/// Application display name (used in window title, notifications, etc.)
pub const APP_NAME: &str = "Nexus BBS";

/// Application directory name (used in config directory path)
pub const APP_DIR_NAME: &str = "nexus";

/// Config file name
pub const CONFIG_FILE_NAME: &str = "config.json";

/// Transfers file name
pub const TRANSFERS_FILE_NAME: &str = "transfers.json";

// =============================================================================
// Panic messages (programmer-error invariants and unrecoverable conditions)
// =============================================================================

/// Panic message: the transfer-registry mutex is poisoned. A poisoned
/// mutex means a previous holder panicked while updating the active
/// transfer map; the in-memory state is unknown-shape.
pub const ERR_TRANSFER_REGISTRY_POISONED: &str = "transfer registry poisoned";

/// Panic message: `SystemTime::now().duration_since(UNIX_EPOCH)` failed
/// (system clock is set before 1970-01-01). Used by user-info /
/// uptime timestamps.
pub const ERR_SYSTEM_TIME_AFTER_EPOCH: &str = "system time should be after Unix epoch";

/// Panic message: a code path expected the user's connection to still
/// be present in `self.connections` — the lookup just upstream
/// confirmed it. Used by `commands/list` and the user-info handler.
pub const ERR_CONNECTION_EXISTS: &str = "connection exists";

/// Panic message: identicon PNG generation from a string seed failed.
/// Programmer-error: the underlying generator is documented to
/// succeed for any string input.
pub const ERR_IDENTICON_GENERATION: &str =
    "Identicon PNG generation from string seed should not fail";

/// Panic message: macOS `NSProcessInfo` class lookup failed. Should
/// never happen on a healthy macOS system; if it does, the
/// Objective-C runtime is broken.
#[cfg(target_os = "macos")]
pub const ERR_NSPROCESSINFO_CLASS_NOT_FOUND: &str = "NSProcessInfo class not found";

/// Panic message: macOS `NSAppleEventManager` class lookup failed.
/// Same reasoning as [`ERR_NSPROCESSINFO_CLASS_NOT_FOUND`].
#[cfg(target_os = "macos")]
pub const ERR_NSAPPLEEVENTMANAGER_CLASS_NOT_FOUND: &str = "NSAppleEventManager class not found";

/// Panic message: rustls crypto-provider installation failed. Should
/// never fire in practice; if it does, the rustls library itself is
/// broken.
pub const ERR_RUSTLS_PROVIDER: &str = "failed to install rustls crypto provider";

/// Panic message: parsing a hardcoded `SNI_SERVER_NAME` constant as a
/// rustls server name failed. Programmer-error: the constant is
/// hand-edited to be valid.
pub const ERR_SNI_SERVER_NAME_INVALID: &str = "SNI_SERVER_NAME is valid";

/// Panic message: the global `TRAY_RX` was accessed before the tray
/// service initialized it. Programmer-error in the startup sequence.
pub const ERR_TRAY_RX_UNINITIALIZED: &str = "TRAY_RX not initialized";

/// Panic message: `DEFAULT_LOCALE` failed to parse as a Fluent
/// `LanguageIdentifier`. Programmer-error: the constant is hand-edited
/// to be valid.
pub const ERR_DEFAULT_LOCALE_INVALID: &str = "DEFAULT_LOCALE is a valid locale";

/// Panic message: parsing the literal `"localhost"` as a rustls DNS
/// name failed. Programmer-error: `"localhost"` is RFC-valid.
pub const ERR_LOCALHOST_INVALID_DNS: &str = "'localhost' is a valid DNS name";

/// Panic message: HKDF-SHA256 expansion to a 32-byte output failed.
/// Programmer-error: 32 bytes is well under the per-spec maximum
/// output length for HKDF-SHA256.
pub const ERR_HKDF_OUTPUT_LENGTH: &str = "32 bytes is a valid output length for HKDF-SHA256";

/// Panic message: an Opus decoder slot was not present immediately
/// after `insert`. Programmer-error in the codec map.
pub const ERR_DECODER_MISSING_AFTER_INSERT: &str = "Decoder should exist after insert";

/// Panic message: the jitter buffer's `next_sequence` was `None` after
/// the initialization branch above set it. Programmer-error.
pub const ERR_NEXT_SEQUENCE_NONE: &str = "next_sequence should be Some after initialization above";

/// Panic message: the voice thread's tokio runtime construction
/// failed. The voice subsystem can't function without it; treated as
/// fatal for the voice path.
pub const ERR_VOICE_THREAD_TOKIO_RUNTIME: &str = "Failed to create tokio runtime for voice thread";

/// Panic message: a file-transfer code path expected a non-empty path
/// after construction. Programmer-error.
pub const ERR_PATH_EMPTY: &str = "non-empty path";

// =============================================================================
// Operator-visible runtime warnings (eprintln! sites)
// =============================================================================
//
// Capitalization follows the existing operator-log convention: messages
// read as sentences with leading capitals, distinct from the lowercased
// panic-message convention above.

/// macOS only: the App-Nap suppression call returned nil instead of a
/// valid activity token. App Nap may not be suppressed on this run.
#[cfg(target_os = "macos")]
pub const ERR_NSPROCESSINFO_BEGIN_ACTIVITY_NIL: &str =
    "macos_nap: beginActivityWithOptions returned nil";

/// macOS only: the URL-handler installer was called from a non-main
/// thread; AppKit requires the main thread for `NSAppleEventManager`
/// registration, so the install is skipped.
#[cfg(target_os = "macos")]
pub const ERR_NSAPPLEEVENTMANAGER_NOT_MAIN_THREAD: &str =
    "macos_url: not on main thread, skipping URL handler install";

/// Format prefix for IPC errors logged to stderr. Composed as
/// `format!("{}{}", ERR_IPC_PREFIX, e)` (trailing ": " included).
pub const ERR_IPC_PREFIX: &str = "IPC error: ";

/// Format prefix for sound-playback errors logged to stderr.
pub const ERR_SOUND_PLAYBACK_PREFIX: &str = "Sound playback error: ";

/// Format prefix for audio-stream errors logged to stderr (cpal
/// stream callback `StreamError`).
pub const ERR_AUDIO_STREAM_PREFIX: &str = "Audio stream error: ";

/// Format prefix for transfer-hashing keepalive send failures (sender
/// dropped before keepalive could be delivered).
pub const ERR_HASH_KEEPALIVE_PREFIX: &str = "[HASH] Failed to send keepalive: ";
