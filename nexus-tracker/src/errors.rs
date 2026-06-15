//! Thin translated-string helpers, one per `err-tracker-*` key.

use crate::i18n::{t, t_args};

/// Sent when a gated flow's password is missing or doesn't verify.
pub fn err_tracker_unauthorized(locale: &str) -> String {
    t(locale, "err-tracker-unauthorized")
}

// Field-validation errors below all map to `error_kind: invalid`.

/// Cert fingerprint did not match the canonical 95-byte uppercase form.
pub fn err_tracker_fingerprint_invalid(locale: &str) -> String {
    t(locale, "err-tracker-fingerprint-invalid")
}

/// `name` exceeded `MAX_SERVER_NAME_LENGTH`.
pub fn err_tracker_name_too_long(locale: &str, max_length: usize) -> String {
    t_args(
        locale,
        "err-tracker-name-too-long",
        &[("max_length", &max_length.to_string())],
    )
}

/// `name` was empty or whitespace-only.
pub fn err_tracker_name_empty(locale: &str) -> String {
    t(locale, "err-tracker-name-empty")
}

/// `name` contained newline characters.
pub fn err_tracker_name_contains_newlines(locale: &str) -> String {
    t(locale, "err-tracker-name-contains-newlines")
}

/// `name` contained non-newline control characters.
pub fn err_tracker_name_invalid_characters(locale: &str) -> String {
    t(locale, "err-tracker-name-invalid-characters")
}

/// `description` exceeded `MAX_SERVER_DESCRIPTION_LENGTH`.
pub fn err_tracker_description_too_long(locale: &str, max_length: usize) -> String {
    t_args(
        locale,
        "err-tracker-description-too-long",
        &[("max_length", &max_length.to_string())],
    )
}

/// `description` contained newline characters.
pub fn err_tracker_description_contains_newlines(locale: &str) -> String {
    t(locale, "err-tracker-description-contains-newlines")
}

/// `description` contained non-newline control characters.
pub fn err_tracker_description_invalid_characters(locale: &str) -> String {
    t(locale, "err-tracker-description-invalid-characters")
}

/// `password` exceeded `MAX_PASSWORD_LENGTH`.
pub fn err_tracker_password_too_long(locale: &str, max_length: usize) -> String {
    t_args(
        locale,
        "err-tracker-password-too-long",
        &[("max_length", &max_length.to_string())],
    )
}

/// `address` exceeded `MAX_PUBLIC_ADDRESS_LENGTH`.
pub fn err_tracker_address_too_long(locale: &str, max_length: usize) -> String {
    t_args(
        locale,
        "err-tracker-address-too-long",
        &[("max_length", &max_length.to_string())],
    )
}

/// `address` failed semantic validation (not a valid hostname / IP).
pub fn err_tracker_address_invalid(locale: &str) -> String {
    t(locale, "err-tracker-address-invalid")
}

/// `version` exceeded `MAX_VERSION_LENGTH`.
pub fn err_tracker_version_too_long(locale: &str, max_length: usize) -> String {
    t_args(
        locale,
        "err-tracker-version-too-long",
        &[("max_length", &max_length.to_string())],
    )
}

/// `version` empty/malformed/non-semver. Used by both register (the
/// server's version) and list (the client's, for compat filtering).
pub fn err_tracker_version_invalid(locale: &str) -> String {
    t(locale, "err-tracker-version-invalid")
}

/// `locale` exceeded `MAX_LOCALE_LENGTH`.
pub fn err_tracker_locale_too_long(locale: &str, max_length: usize) -> String {
    t_args(
        locale,
        "err-tracker-locale-too-long",
        &[("max_length", &max_length.to_string())],
    )
}

/// `locale` contained control characters.
pub fn err_tracker_locale_invalid(locale: &str) -> String {
    t(locale, "err-tracker-locale-invalid")
}

/// `port` field was zero. The advertised BBS port must be non-zero
/// for the entry to be reachable.
pub fn err_tracker_port_zero(locale: &str) -> String {
    t(locale, "err-tracker-port-zero")
}

/// `websocket_port` field was zero. When present, the WebSocket port
/// must be non-zero for the same reason as `port`.
pub fn err_tracker_websocket_port_zero(locale: &str) -> String {
    t(locale, "err-tracker-websocket-port-zero")
}

/// Per-IP rate limit hit (token bucket exhausted).
pub fn err_tracker_rate_limited(locale: &str) -> String {
    t(locale, "err-tracker-rate-limited")
}

/// Refresh targeted a registration that is no longer live.
pub fn err_tracker_refresh_unknown(locale: &str) -> String {
    t(locale, "err-tracker-refresh-unknown")
}

/// Tracker is at `--max-entries`.
pub fn err_tracker_capacity(locale: &str) -> String {
    t(locale, "err-tracker-capacity")
}

/// Source IPv4 address or IPv6 /64 is at `--max-entries-per-ip`.
pub fn err_tracker_per_ip_capacity(locale: &str) -> String {
    t(locale, "err-tracker-per-ip-capacity")
}

/// JSON parse failed for an otherwise-known message envelope (distinct
/// from `err_tracker_frame_error` so registrants get an actionable hint).
pub fn err_tracker_malformed_message(locale: &str) -> String {
    t(locale, "err-tracker-malformed-message")
}

/// Peer sent a non-`Handshake` message before completing the handshake.
pub fn err_tracker_handshake_required(locale: &str) -> String {
    t(locale, "err-tracker-handshake-required")
}

/// `TrackerServerList` on a server connection, or `TrackerServerRegister` on a
/// client connection.
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

/// Unrecognized message `type` (distinct from `err_tracker_frame_error`
/// so a wrong-port peer, e.g. BBS `Login`, sees the real cause).
pub fn err_tracker_unknown_message_type(locale: &str) -> String {
    t(locale, "err-tracker-unknown-message-type")
}

/// Known message `type` sent in the wrong protocol phase or direction.
pub fn err_tracker_unexpected_message_type(locale: &str) -> String {
    t(locale, "err-tracker-unexpected-message-type")
}

/// Magic bytes / framing structure violated.
pub fn err_tracker_frame_error(locale: &str) -> String {
    t(locale, "err-tracker-frame-error")
}

/// Frame payload exceeded the per-message-type maximum.
pub fn err_tracker_payload_too_large(locale: &str) -> String {
    t(locale, "err-tracker-payload-too-large")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every `err_tracker_*` helper resolves to non-empty EN text,
    /// catching key typos that would otherwise panic at runtime.
    #[test]
    fn test_all_helpers_resolve_in_english() {
        let cases: Vec<String> = vec![
            err_tracker_unauthorized("en"),
            err_tracker_fingerprint_invalid("en"),
            err_tracker_name_too_long("en", 64),
            err_tracker_name_empty("en"),
            err_tracker_name_contains_newlines("en"),
            err_tracker_name_invalid_characters("en"),
            err_tracker_description_too_long("en", 512),
            err_tracker_description_contains_newlines("en"),
            err_tracker_description_invalid_characters("en"),
            err_tracker_password_too_long("en", 256),
            err_tracker_address_too_long("en", 253),
            err_tracker_address_invalid("en"),
            err_tracker_version_too_long("en", 32),
            err_tracker_version_invalid("en"),
            err_tracker_locale_too_long("en", 16),
            err_tracker_locale_invalid("en"),
            err_tracker_port_zero("en"),
            err_tracker_websocket_port_zero("en"),
            err_tracker_rate_limited("en"),
            err_tracker_refresh_unknown("en"),
            err_tracker_capacity("en"),
            err_tracker_per_ip_capacity("en"),
            err_tracker_malformed_message("en"),
            err_tracker_handshake_required("en"),
            err_tracker_role_violation("en"),
            err_tracker_handshake_version_invalid("en"),
            err_tracker_protocol_version_mismatch("en", "0.1.0", "1.0.0"),
            err_tracker_unknown_message_type("en"),
            err_tracker_unexpected_message_type("en"),
            err_tracker_frame_error("en"),
            err_tracker_payload_too_large("en"),
        ];
        for s in &cases {
            assert!(!s.is_empty(), "translation produced empty string");
        }
    }
}
