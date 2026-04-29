//! Translated error helpers for tracker handlers
//!
//! Each helper returns a localized string for a single `err-tracker-*`
//! key. Keep these helpers thin so call sites read uniformly:
//!
//! ```ignore
//! return Err(err_tracker_unauthorized(locale));
//! ```
//!
//! Helpers used only by handlers that haven't been written yet are
//! annotated `#[allow(dead_code)]` until those handlers wire them up.

use crate::i18n::{t, t_args};

// =============================================================================
// Authentication
// =============================================================================

/// "Wrong or missing password" — sent when registration or listing is
/// gated and the peer's password is missing or doesn't verify.
#[allow(dead_code)] // used by TrackerRegister/TrackerList handlers (later step)
pub fn err_tracker_unauthorized(locale: &str) -> String {
    t(locale, "err-tracker-unauthorized")
}

// =============================================================================
// Field validation (error_kind: invalid)
// =============================================================================

/// Cert fingerprint did not match the canonical 95-byte uppercase form.
#[allow(dead_code)] // used by TrackerRegister handler (later step)
pub fn err_tracker_fingerprint_invalid(locale: &str) -> String {
    t(locale, "err-tracker-fingerprint-invalid")
}

/// `name` exceeded `MAX_SERVER_NAME_LENGTH`.
#[allow(dead_code)] // used by TrackerRegister handler (later step)
pub fn err_tracker_name_too_long(locale: &str, max_length: usize) -> String {
    t_args(
        locale,
        "err-tracker-name-too-long",
        &[("max_length", &max_length.to_string())],
    )
}

/// `description` exceeded `MAX_SERVER_DESCRIPTION_LENGTH`.
#[allow(dead_code)] // used by TrackerRegister handler (later step)
pub fn err_tracker_description_too_long(locale: &str, max_length: usize) -> String {
    t_args(
        locale,
        "err-tracker-description-too-long",
        &[("max_length", &max_length.to_string())],
    )
}

/// `password` exceeded `MAX_PASSWORD_LENGTH`.
#[allow(dead_code)] // used by TrackerRegister/TrackerList handlers (later step)
pub fn err_tracker_password_too_long(locale: &str, max_length: usize) -> String {
    t_args(
        locale,
        "err-tracker-password-too-long",
        &[("max_length", &max_length.to_string())],
    )
}

/// `address` exceeded `MAX_PUBLIC_ADDRESS_LENGTH`.
#[allow(dead_code)] // used by TrackerRegister handler (later step)
pub fn err_tracker_address_too_long(locale: &str, max_length: usize) -> String {
    t_args(
        locale,
        "err-tracker-address-too-long",
        &[("max_length", &max_length.to_string())],
    )
}

/// `address` failed semantic validation (not a valid hostname / IP).
#[allow(dead_code)] // used by TrackerRegister handler (later step)
pub fn err_tracker_address_invalid(locale: &str) -> String {
    t(locale, "err-tracker-address-invalid")
}

/// `version` exceeded `MAX_VERSION_LENGTH`.
#[allow(dead_code)] // used by TrackerRegister handler (later step)
pub fn err_tracker_version_too_long(locale: &str, max_length: usize) -> String {
    t_args(
        locale,
        "err-tracker-version-too-long",
        &[("max_length", &max_length.to_string())],
    )
}

/// `locale` exceeded `MAX_LOCALE_LENGTH`.
#[allow(dead_code)] // used by TrackerRegister handler (later step)
pub fn err_tracker_locale_too_long(locale: &str, max_length: usize) -> String {
    t_args(
        locale,
        "err-tracker-locale-too-long",
        &[("max_length", &max_length.to_string())],
    )
}

// =============================================================================
// Rate / capacity
// =============================================================================

/// Per-IP rate limit hit (token bucket exhausted).
#[allow(dead_code)] // used by rate limiter (later step)
pub fn err_tracker_rate_limited(locale: &str) -> String {
    t(locale, "err-tracker-rate-limited")
}

/// Tracker is at `--max-entries`; new registrations are refused until
/// existing entries expire or disconnect.
#[allow(dead_code)] // used by TrackerRegister handler (later step)
pub fn err_tracker_capacity(locale: &str) -> String {
    t(locale, "err-tracker-capacity")
}

/// Source IP has reached `--max-entries-per-ip` and cannot register
/// another entry until one of its existing entries unregisters or
/// stale-evicts.
#[allow(dead_code)] // used by TrackerRegister handler (later step)
pub fn err_tracker_per_ip_capacity(locale: &str) -> String {
    t(locale, "err-tracker-per-ip-capacity")
}

// =============================================================================
// Protocol-level
// =============================================================================

/// JSON parse failed for an otherwise-known message envelope. The
/// connection task currently lumps all `FrameError` cases under
/// `err_tracker_frame_error`; this helper goes live once we differentiate
/// JSON-parse failures from frame-format violations (later step).
#[allow(dead_code)] // used once we differentiate FrameError variants
pub fn err_tracker_malformed_message(locale: &str) -> String {
    t(locale, "err-tracker-malformed-message")
}

/// Peer sent a non-`Handshake` message before completing the handshake.
pub fn err_tracker_handshake_required(locale: &str) -> String {
    t(locale, "err-tracker-handshake-required")
}

/// `TrackerList` on a server connection, or `TrackerRegister` on a
/// client connection.
#[allow(dead_code)] // used by connection task (later step)
pub fn err_tracker_role_violation(locale: &str) -> String {
    t(locale, "err-tracker-role-violation")
}

/// Handshake version string failed length / semver validation.
pub fn err_tracker_handshake_version_invalid(locale: &str) -> String {
    t(locale, "err-tracker-handshake-version-invalid")
}

/// Peer's tracker protocol version is incompatible with this tracker's.
/// `server` and `client` are the canonical semver strings for each side.
pub fn err_tracker_protocol_version_mismatch(locale: &str, server: &str, client: &str) -> String {
    t_args(
        locale,
        "err-tracker-protocol-version-mismatch",
        &[("server", server), ("client", client)],
    )
}

/// Peer sent a message whose `type` field isn't recognized.
#[allow(dead_code)] // used by connection task (later step)
pub fn err_tracker_unknown_message_type(locale: &str) -> String {
    t(locale, "err-tracker-unknown-message-type")
}

// =============================================================================
// Frame / transport
// =============================================================================

/// Magic bytes / framing structure violated.
pub fn err_tracker_frame_error(locale: &str) -> String {
    t(locale, "err-tracker-frame-error")
}

/// Frame payload exceeded the per-message-type maximum.
#[allow(dead_code)] // used by connection task (later step, when reading typed messages)
pub fn err_tracker_payload_too_large(locale: &str) -> String {
    t(locale, "err-tracker-payload-too-large")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Smoke test: every English key bound to an `err_tracker_*` helper
    /// resolves to a non-empty string. This catches typos that would
    /// otherwise only surface at handler runtime as a missing-key panic.
    #[test]
    fn test_all_helpers_resolve_in_english() {
        let cases: Vec<String> = vec![
            err_tracker_unauthorized("en"),
            err_tracker_fingerprint_invalid("en"),
            err_tracker_name_too_long("en", 64),
            err_tracker_description_too_long("en", 512),
            err_tracker_password_too_long("en", 256),
            err_tracker_address_too_long("en", 253),
            err_tracker_address_invalid("en"),
            err_tracker_version_too_long("en", 32),
            err_tracker_locale_too_long("en", 16),
            err_tracker_rate_limited("en"),
            err_tracker_capacity("en"),
            err_tracker_per_ip_capacity("en"),
            err_tracker_malformed_message("en"),
            err_tracker_handshake_required("en"),
            err_tracker_role_violation("en"),
            err_tracker_handshake_version_invalid("en"),
            err_tracker_protocol_version_mismatch("en", "0.1.0", "1.0.0"),
            err_tracker_unknown_message_type("en"),
            err_tracker_frame_error("en"),
            err_tracker_payload_too_large("en"),
        ];
        for s in &cases {
            assert!(!s.is_empty(), "translation produced empty string");
        }
    }
}
