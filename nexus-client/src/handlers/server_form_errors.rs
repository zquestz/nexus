//! Field-shape error translation shared by the connection form
//! (`handlers/connection.rs`) and the bookmark form (`handlers/bookmarks.rs`).
//! Both accept the same identifying field set (server name, server
//! address, optional username/nickname/password, optional cert pin)
//! and need to render identical localized error text. Sharing the
//! field-shape translation prevents silent drift if a new validator
//! variant is added — adding a variant to `PublicAddressError` or any
//! other validator here forces a compile error in the single match arm
//! rather than passing one form and missing the other.

use nexus_common::fingerprint::is_canonical_fingerprint;
use nexus_common::validators::{
    self, ConnectionAddressError, MAX_NICKNAME_LENGTH, MAX_PASSWORD_LENGTH,
    MAX_PUBLIC_ADDRESS_LENGTH, MAX_SERVER_NAME_LENGTH, MAX_USERNAME_LENGTH, NicknameError,
    ServerNameError, UsernameError,
};

use crate::i18n::{t, t_args};

/// Run field-shape checks in canonical order. Returns the first
/// localized error encountered, or `Ok(())` if all checks pass.
///
/// `server_name` and `server_address` are required (must be non-empty
/// after trim). `username`, `nickname`, `password`, and `fingerprint`
/// are validated only when non-empty — empty is the "field unset"
/// signal in connection / bookmark forms.
pub fn translate_server_form_errors(
    server_name: &str,
    server_address: &str,
    username: &str,
    nickname: &str,
    password: &str,
    fingerprint: &str,
) -> Result<(), String> {
    if let Err(e) = validators::validate_server_name(server_name) {
        return Err(translate_server_name_error(e));
    }

    // Server address: required + connection-target shape rules
    // (no scheme, path, userinfo, whitespace, embedded port outside
    // IPv6, length cap, valid hostname/IP). Allows IPv6 literal
    // brackets `[::1]` and zone identifiers `fe80::1%eth0`, which
    // `validate_public_address` rejects but the resolver accepts.
    if let Err(e) = validators::validate_connection_address(server_address) {
        return Err(translate_server_address_error(e));
    }

    if !username.is_empty()
        && let Err(e) = validators::validate_username(username)
    {
        return Err(translate_username_error(e));
    }

    if !nickname.is_empty()
        && let Err(e) = validators::validate_nickname(nickname)
    {
        return Err(translate_nickname_error(e));
    }

    // Password: only length-capped on the login path (empty allowed
    // for guest accounts; control chars / unicode / passphrases all
    // permitted). `validate_password_input` returns only `TooLong`,
    // so the error mapping is unconditional.
    if validators::validate_password_input(password).is_err() {
        return Err(t_args(
            "err-password-too-long",
            &[("max", &MAX_PASSWORD_LENGTH.to_string())],
        ));
    }

    let fp = fingerprint.trim();
    if !fp.is_empty() && !is_canonical_fingerprint(fp) {
        return Err(t("err-fingerprint-invalid"));
    }

    Ok(())
}

fn translate_server_name_error(e: ServerNameError) -> String {
    match e {
        ServerNameError::Empty => t("err-server-name-empty"),
        ServerNameError::TooLong => t_args(
            "err-server-name-too-long",
            &[("max", &MAX_SERVER_NAME_LENGTH.to_string())],
        ),
        ServerNameError::ContainsNewlines => t("err-server-name-contains-newlines"),
        ServerNameError::InvalidCharacters => t("err-server-name-invalid-characters"),
    }
}

pub(crate) fn translate_server_address_error(e: ConnectionAddressError) -> String {
    match e {
        ConnectionAddressError::Empty => t("err-server-address-empty"),
        ConnectionAddressError::TooLong => t_args(
            "err-server-address-too-long",
            &[("max", &MAX_PUBLIC_ADDRESS_LENGTH.to_string())],
        ),
        ConnectionAddressError::ContainsScheme => t("err-server-address-contains-scheme"),
        ConnectionAddressError::ContainsPath => t("err-server-address-contains-path"),
        ConnectionAddressError::ContainsUserinfo => t("err-server-address-contains-userinfo"),
        ConnectionAddressError::ContainsWhitespace => t("err-server-address-contains-whitespace"),
        ConnectionAddressError::ContainsPort => t("err-server-address-contains-port"),
        ConnectionAddressError::InvalidFormat => t("err-server-address-invalid-format"),
    }
}

fn translate_username_error(e: UsernameError) -> String {
    match e {
        UsernameError::Empty => t("err-username-empty"),
        UsernameError::TooLong => t_args(
            "err-username-too-long",
            &[("max", &MAX_USERNAME_LENGTH.to_string())],
        ),
        UsernameError::InvalidCharacters => t("err-username-invalid"),
    }
}

fn translate_nickname_error(e: NicknameError) -> String {
    match e {
        NicknameError::Empty => t("err-nickname-empty"),
        NicknameError::TooLong => t_args(
            "err-nickname-too-long",
            &[("max", &MAX_NICKNAME_LENGTH.to_string())],
        ),
        NicknameError::InvalidCharacters => t("err-nickname-invalid"),
    }
}
