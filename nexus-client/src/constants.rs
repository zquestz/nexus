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

/// Panic message: `rfind(char::is_whitespace)` returned an index that should
/// point at a valid whitespace character in the original string.
pub const ERR_WHITESPACE_CHAR_AT_RFIND_INDEX: &str =
    "rfind(char::is_whitespace) returns a valid character index";

/// Panic message: the startup URI lock is poisoned. A poisoned lock means
/// a previous holder panicked while updating the one-shot startup URI.
pub const ERR_STARTUP_URI_LOCK_POISONED: &str = "startup URI lock poisoned";

/// Panic message: the sound-state lock is poisoned. A poisoned lock means
/// the audio-thread sender state may be unknown-shape.
pub const ERR_SOUND_STATE_LOCK_POISONED: &str = "sound state lock poisoned";

/// Panic message: `str::split` unexpectedly returned no segments while parsing
/// a hotkey string. The standard-library iterator always yields at least one.
pub const ERR_HOTKEY_SPLIT_EMPTY: &str = "str::split returns at least one segment";

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

/// Panic message: the global `TRAY_RX` was accessed before the tray
/// service initialized it. Programmer-error in the startup sequence.
/// Linux-only — the Windows tray uses crossbeam channels, no static.
#[cfg(target_os = "linux")]
pub const ERR_TRAY_RX_UNINITIALIZED: &str = "TRAY_RX not initialized";

/// Panic message: `DEFAULT_LOCALE` failed to parse as a Fluent
/// `LanguageIdentifier`. Programmer-error: the constant is hand-edited
/// to be valid.
pub const ERR_DEFAULT_LOCALE_INVALID: &str = "DEFAULT_LOCALE is a valid locale";

/// Panic message: HKDF-SHA256 expansion to a 32-byte output failed.
/// Programmer-error: 32 bytes is well under the per-spec maximum
/// output length for HKDF-SHA256.
pub const ERR_HKDF_OUTPUT_LENGTH: &str = "32 bytes is a valid output length for HKDF-SHA256";

/// Panic message: an Opus decoder slot was not present immediately
/// after `insert`. Programmer-error in the codec map.
pub const ERR_DECODER_MISSING_AFTER_INSERT: &str = "Decoder should exist after insert";

/// Panic message: a mixer user buffer was not present immediately
/// after `insert`. Programmer-error in the mixer map.
pub const ERR_MIXER_BUFFER_MISSING_AFTER_INSERT: &str = "Mixer buffer should exist after insert";

/// Panic message: the jitter buffer's `next_sequence` was `None` after
/// the initialization branch above set it. Programmer-error.
pub const ERR_NEXT_SEQUENCE_NONE: &str = "next_sequence should be Some after initialization above";

/// Panic message: the voice thread's tokio runtime construction
/// failed. The voice subsystem can't function without it; treated as
/// fatal for the voice path.
pub const ERR_VOICE_THREAD_TOKIO_RUNTIME: &str = "Failed to create tokio runtime for voice thread";

/// Error prefix: WebRTC audio processor construction failed. Surfaced
/// through the voice-processor-disabled warning path.
pub const ERR_VOICE_PROCESSOR_CREATE: &str = "Failed to create processor";

/// Error prefix: WebRTC capture-frame processing failed.
pub const ERR_VOICE_CAPTURE_PROCESSING: &str = "Capture processing error";

/// Error prefix: WebRTC render-frame analysis failed.
pub const ERR_VOICE_RENDER_ANALYSIS: &str = "Render analysis error";

/// Error prefix: enumerating input device configurations failed.
pub const ERR_AUDIO_INPUT_CONFIGS: &str = "Failed to get supported input configs";

/// Error: the input device reported zero supported configurations.
pub const ERR_AUDIO_INPUT_NO_CONFIGS: &str = "Input device has no supported configurations";

/// Error: the input device has configurations, but none match our
/// supported sample formats / channel layouts.
pub const ERR_AUDIO_INPUT_NO_MATCH: &str = "Input device has no supported audio configuration";

/// Error: the configured input device was not found on the host.
pub const ERR_AUDIO_INPUT_DEVICE_NOT_FOUND: &str = "Input device not found";

/// Error prefix: constructing the capture-side rubato resampler failed.
pub const ERR_AUDIO_INPUT_RESAMPLER: &str = "Failed to create input resampler";

/// Error prefix: starting the capture stream failed.
pub const ERR_AUDIO_CAPTURE_START: &str = "Failed to start capture";

/// Error prefix: the capture stream's runtime error callback fired.
pub const ERR_AUDIO_CAPTURE: &str = "Audio capture error";

/// Error prefix: building the mono input stream failed.
pub const ERR_AUDIO_INPUT_STREAM_BUILD: &str = "Failed to build input stream";

/// Error prefix: building the stereo (downmixed) input stream failed.
pub const ERR_AUDIO_STEREO_INPUT_STREAM_BUILD: &str = "Failed to build stereo input stream";

/// Error prefix: enumerating output device configurations failed.
pub const ERR_AUDIO_OUTPUT_CONFIGS: &str = "Failed to get supported output configs";

/// Error: the output device reported zero supported configurations.
pub const ERR_AUDIO_OUTPUT_NO_CONFIGS: &str = "Output device has no supported configurations";

/// Error: the output device has configurations, but none match our
/// supported sample formats / channel layouts.
pub const ERR_AUDIO_OUTPUT_NO_MATCH: &str = "Output device has no supported audio configuration";

/// Error: the configured output device was not found on the host.
pub const ERR_AUDIO_OUTPUT_DEVICE_NOT_FOUND: &str = "Output device not found";

/// Error prefix: constructing the render-side rubato resampler failed.
pub const ERR_AUDIO_OUTPUT_RESAMPLER: &str = "Failed to create output resampler";

/// Error prefix: starting the mixer output stream failed.
pub const ERR_AUDIO_MIXER_START: &str = "Failed to start mixer";

/// Error prefix: the mixer stream's runtime error callback fired.
pub const ERR_AUDIO_MIXER: &str = "Mixer error";

/// Error prefix: building the mono mixer output stream failed.
pub const ERR_AUDIO_MIXER_STREAM_BUILD: &str = "Failed to build mixer stream";

/// Error prefix: building the stereo (upmixed) mixer output stream failed.
pub const ERR_AUDIO_STEREO_MIXER_STREAM_BUILD: &str = "Failed to build stereo mixer stream";

/// Panic message: a file-transfer code path expected a non-empty path
/// after construction. Programmer-error.
pub const ERR_PATH_EMPTY: &str = "non-empty path";

/// Panic message: our own `PROTOCOL_VERSION` constant failed semver parsing.
/// Programmer-error caught at the first handshake in any run.
pub const ERR_PROTOCOL_VERSION_UNPARSEABLE: &str =
    "PROTOCOL_VERSION is a canonical semver constant";

/// Panic message: our own `TRACKER_PROTOCOL_VERSION` constant failed semver
/// parsing. Programmer-error caught at the first tracker query in any run.
pub const ERR_TRACKER_PROTOCOL_VERSION_UNPARSEABLE: &str =
    "TRACKER_PROTOCOL_VERSION is a canonical semver constant";

// =============================================================================
// Runtime diagnostic details (wrapped by localized UI messages where shown)
// =============================================================================

/// Sound playback diagnostic: OGG container could not be opened.
pub const ERR_SOUND_DECODE_OGG: &str = "Failed to decode OGG";
/// Sound playback diagnostic: OGG packet decode failed.
pub const ERR_SOUND_DECODE_PACKET: &str = "Decode error";
/// Sound playback diagnostic: device default output config query failed.
pub const ERR_SOUND_OUTPUT_CONFIG: &str = "Failed to query default output config";
/// Sound playback diagnostic: stream playback failed to start.
pub const ERR_SOUND_START_PLAYBACK: &str = "Failed to start playback";
/// Sound playback diagnostic: CPAL reported a sample format we do not support.
pub const ERR_SOUND_UNSUPPORTED_SAMPLE_FORMAT: &str = "Unsupported output sample format";
/// Sound playback diagnostic: output stream construction failed.
pub const ERR_SOUND_BUILD_OUTPUT_STREAM: &str = "Failed to build output stream";
/// Sound playback diagnostic: source or output sample rate was invalid.
pub const ERR_SOUND_INVALID_SAMPLE_RATE: &str = "Invalid sound sample rate";
/// Sound playback diagnostic: source or output channel count was invalid.
pub const ERR_SOUND_INVALID_CHANNEL_COUNT: &str = "Invalid sound channel count";
/// Sound playback diagnostic: no usable output device was available.
pub const ERR_SOUND_NO_DEFAULT_OUTPUT_DEVICE: &str = "No default output device available";
/// Sound playback diagnostic: output device enumeration failed.
pub const ERR_SOUND_ENUMERATE_DEVICES: &str = "Failed to enumerate devices";

/// Voice DTLS diagnostic: UDP socket bind failed.
pub const ERR_VOICE_DTLS_BIND_UDP_SOCKET: &str = "Failed to bind UDP socket";
/// Voice DTLS diagnostic: UDP socket connect failed.
pub const ERR_VOICE_DTLS_CONNECT_UDP_SOCKET: &str = "Failed to connect UDP socket";
/// Voice DTLS diagnostic: handshake timed out.
pub const ERR_VOICE_DTLS_HANDSHAKE_TIMEOUT: &str = "DTLS handshake timeout";
/// Voice DTLS diagnostic: handshake failed.
pub const ERR_VOICE_DTLS_HANDSHAKE_FAILED: &str = "DTLS handshake failed";
/// Voice DTLS diagnostic: packet send failed.
pub const ERR_VOICE_DTLS_SEND_PACKET: &str = "Failed to send voice packet";
/// Voice DTLS diagnostic: packet receive failed.
pub const ERR_VOICE_DTLS_RECEIVE_PACKET: &str = "Failed to receive";
/// Voice DTLS diagnostic: relayed packet failed validation.
pub const ERR_VOICE_DTLS_INVALID_RELAYED_PACKET: &str = "Invalid relayed packet";
/// Voice DTLS diagnostic: connection close failed.
pub const ERR_VOICE_DTLS_CLOSE_CONNECTION: &str = "Failed to close connection";
/// Voice DTLS diagnostic: peer certificate did not match the TLS-verified fingerprint.
pub const ERR_VOICE_CERT_FINGERPRINT_MISMATCH: &str =
    "voice server certificate does not match the verified TLS certificate";
/// Voice connection diagnostic: DTLS closed before setup completed.
pub const ERR_VOICE_CONNECTION_CLOSED: &str = "Connection closed";
/// Voice audio diagnostic: output device setup failed.
pub const ERR_VOICE_OUTPUT_DEVICE: &str = "Output device error";
/// Voice audio diagnostic: playback stream failed to start.
pub const ERR_VOICE_PLAYBACK_START: &str = "Failed to start playback";
/// Voice audio diagnostic: microphone capture failed.
pub const ERR_VOICE_CAPTURE: &str = "Capture error";

/// Voice resampler diagnostic: input resampler construction failed.
pub const ERR_INPUT_RESAMPLER_CREATE: &str = "Failed to create input resampler";
/// Voice resampler diagnostic: output resampler construction failed.
pub const ERR_OUTPUT_RESAMPLER_CREATE: &str = "Failed to create output resampler";
/// Voice resampler diagnostic: rubato input adapter construction failed.
pub const ERR_INPUT_ADAPTER: &str = "Input adapter error";
/// Voice resampler diagnostic: rubato output adapter construction failed.
pub const ERR_OUTPUT_ADAPTER: &str = "Output adapter error";
/// Voice resampler diagnostic: rubato processing failed.
pub const ERR_RESAMPLER_PROCESS: &str = "Resampler error";

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

/// macOS bundle identifier. Must match the `[package.metadata.bundle]`
/// `identifier` in `nexus-client/Cargo.toml`.
#[cfg(target_os = "macos")]
pub const MACOS_BUNDLE_ID: &str = "at.greyh.nexus";

/// Format prefix for macOS notification-backend registration failures
/// logged to stderr (trailing ": " included). Expected on unbundled dev
/// runs, where the bundle identifier resolves to no installed app.
#[cfg(target_os = "macos")]
pub const ERR_MACOS_NOTIFY_APP_PREFIX: &str = "Failed to set notification application: ";
