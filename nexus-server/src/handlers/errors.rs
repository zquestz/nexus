//! Error message constants for handlers

use crate::i18n::{t, t_args};

// Authentication & Session Errors
/// Get translated "not logged in" error
pub fn err_not_logged_in(locale: &str) -> String {
    t(locale, "err-not-logged-in")
}

/// Get translated "authentication" error
pub fn err_authentication(locale: &str) -> String {
    t(locale, "err-authentication")
}

/// Get translated "invalid credentials" error
pub fn err_invalid_credentials(locale: &str) -> String {
    t(locale, "err-invalid-credentials")
}

/// Get translated "handshake required" error
pub fn err_handshake_required(locale: &str) -> String {
    t(locale, "err-handshake-required")
}

/// Get translated "already logged in" error
pub fn err_already_logged_in(locale: &str) -> String {
    t(locale, "err-already-logged-in")
}

/// Get translated "handshake already completed" error
pub fn err_handshake_already_completed(locale: &str) -> String {
    t(locale, "err-handshake-already-completed")
}

/// Get translated "account deleted" error
pub fn err_account_deleted(locale: &str) -> String {
    t(locale, "err-account-deleted")
}

/// Get translated "account disabled by admin" error
pub fn err_account_disabled_by_admin(locale: &str) -> String {
    t(locale, "err-account-disabled-by-admin")
}

// Permission & Access Errors
/// Get translated "permission denied" error
pub fn err_permission_denied(locale: &str) -> String {
    t(locale, "err-permission-denied")
}

// Database Errors
/// Get translated "database" error
pub fn err_database(locale: &str) -> String {
    t(locale, "err-database")
}

// Message Format Errors
/// Get translated "invalid message format" error
pub fn err_invalid_message_format(locale: &str) -> String {
    t(locale, "err-invalid-message-format")
}

// User Management Errors
/// Get translated "cannot delete last admin" error
pub fn err_cannot_delete_last_admin(locale: &str) -> String {
    t(locale, "err-cannot-delete-last-admin")
}

/// Get translated "cannot delete self" error
pub fn err_cannot_delete_self(locale: &str) -> String {
    t(locale, "err-cannot-delete-self")
}

/// Get translated "cannot demote last admin" error
pub fn err_cannot_demote_last_admin(locale: &str) -> String {
    t(locale, "err-cannot-demote-last-admin")
}

/// Get translated "cannot edit self" error
pub fn err_cannot_edit_self(locale: &str) -> String {
    t(locale, "err-cannot-edit-self")
}

/// Get translated "cannot create admin" error
pub fn err_cannot_create_admin(locale: &str) -> String {
    t(locale, "err-cannot-create-admin")
}

/// Get translated "cannot kick self" error
pub fn err_cannot_kick_self(locale: &str) -> String {
    t(locale, "err-cannot-kick-self")
}

/// Get translated "cannot kick admin" error
pub fn err_cannot_kick_admin(locale: &str) -> String {
    t(locale, "err-cannot-kick-admin")
}

/// Get translated "cannot disable last admin" error
pub fn err_cannot_disable_last_admin(locale: &str) -> String {
    t(locale, "err-cannot-disable-last-admin")
}
// Chat Topic Errors
/// Get translated "topic contains newlines" error
pub fn err_topic_contains_newlines(locale: &str) -> String {
    t(locale, "err-topic-contains-newlines")
}

// Message Validation Errors
/// Get translated "message empty" error
pub fn err_message_empty(locale: &str) -> String {
    t(locale, "err-message-empty")
}

// Dynamic error messages with parameters

/// Get translated "broadcast too long" error
pub fn err_broadcast_too_long(locale: &str, max_length: usize) -> String {
    t_args(
        locale,
        "err-broadcast-too-long",
        &[("max_length", &max_length.to_string())],
    )
}

/// Get translated "chat too long" error
pub fn err_chat_too_long(locale: &str, max_length: usize) -> String {
    t_args(
        locale,
        "err-chat-too-long",
        &[("max_length", &max_length.to_string())],
    )
}

/// Get translated "topic too long" error
pub fn err_topic_too_long(locale: &str, max_length: usize) -> String {
    t_args(
        locale,
        "err-topic-too-long",
        &[("max_length", &max_length.to_string())],
    )
}

/// Get translated "version mismatch" error
pub fn err_version_mismatch(locale: &str, server_version: &str, client_version: &str) -> String {
    t_args(
        locale,
        "err-version-mismatch",
        &[
            ("server_version", server_version),
            ("client_version", client_version),
        ],
    )
}

/// Get translated "kicked by" message
pub fn err_kicked_by(locale: &str, username: &str) -> String {
    t_args(locale, "err-kicked-by", &[("username", username)])
}

/// Get translated "username exists" error
pub fn err_username_exists(locale: &str, username: &str) -> String {
    t_args(locale, "err-username-exists", &[("username", username)])
}

/// Get translated "user not found" error
pub fn err_user_not_found(locale: &str, username: &str) -> String {
    t_args(locale, "err-user-not-found", &[("username", username)])
}

/// Get translated "user not online" error
pub fn err_user_not_online(locale: &str, username: &str) -> String {
    t_args(locale, "err-user-not-online", &[("username", username)])
}

/// Get translated "failed to create user" error
pub fn err_failed_to_create_user(locale: &str, username: &str) -> String {
    t_args(
        locale,
        "err-failed-to-create-user",
        &[("username", username)],
    )
}

/// Get translated "account disabled" error
pub fn err_account_disabled(locale: &str, username: &str) -> String {
    t_args(locale, "err-account-disabled", &[("username", username)])
}

/// Get translated "update failed" error
pub fn err_update_failed(locale: &str, username: &str) -> String {
    t_args(locale, "err-update-failed", &[("username", username)])
}
