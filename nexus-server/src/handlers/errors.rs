//! Error message functions for handlers
//!
//! All user-facing error messages are translated via the i18n system.
//! Functions are organized alphabetically for easy lookup.

use crate::i18n::{t, t_args};

// ========================================================================
// Nickname Validation Errors
// ========================================================================

/// Get translated "nickname empty" error
pub fn err_nickname_empty(locale: &str) -> String {
    t(locale, "err-nickname-empty")
}

/// Get translated "nickname in use" error
pub fn err_nickname_in_use(locale: &str) -> String {
    t(locale, "err-nickname-in-use")
}

/// Get translated "nickname invalid" error
pub fn err_nickname_invalid(locale: &str) -> String {
    t(locale, "err-nickname-invalid")
}

/// Get translated "nickname is username" error
pub fn err_nickname_is_username(locale: &str) -> String {
    t(locale, "err-nickname-is-username")
}

/// Get translated "nickname required" error
pub fn err_nickname_required(locale: &str) -> String {
    t(locale, "err-nickname-required")
}

/// Get translated "nickname too long" error
pub fn err_nickname_too_long(locale: &str, max_length: usize) -> String {
    t_args(
        locale,
        "err-nickname-too-long",
        &[("max_length", &max_length.to_string())],
    )
}

// ========================================================================
// Shared Account Errors
// ========================================================================

/// Get translated "shared cannot be admin" error
pub fn err_shared_cannot_be_admin(locale: &str) -> String {
    t(locale, "err-shared-cannot-be-admin")
}

/// Get translated "shared cannot change password" error
pub fn err_shared_cannot_change_password(locale: &str) -> String {
    t(locale, "err-shared-cannot-change-password")
}

/// Get translated "shared invalid permissions" error
pub fn err_shared_invalid_permissions(locale: &str, permissions: &str) -> String {
    t_args(
        locale,
        "err-shared-invalid-permissions",
        &[("permissions", permissions)],
    )
}

// ========================================================================
// Status Message Errors
// ========================================================================

/// Get translated "status message too long" error
pub fn err_status_too_long(locale: &str, max_length: usize) -> String {
    t_args(
        locale,
        "err-status-too-long",
        &[("max_length", &max_length.to_string())],
    )
}

/// Get translated "status message contains newlines" error
pub fn err_status_contains_newlines(locale: &str) -> String {
    t(locale, "err-status-contains-newlines")
}

/// Get translated "status message invalid characters" error
pub fn err_status_invalid_characters(locale: &str) -> String {
    t(locale, "err-status-invalid-characters")
}

// ========================================================================
// Guest Account Errors
// ========================================================================

/// Get translated "guest disabled" error
pub fn err_guest_disabled(locale: &str) -> String {
    t(locale, "err-guest-disabled")
}

/// Get translated "cannot rename guest" error
pub fn err_cannot_rename_guest(locale: &str) -> String {
    t(locale, "err-cannot-rename-guest")
}

/// Get translated "cannot change guest password" error
pub fn err_cannot_change_guest_password(locale: &str) -> String {
    t(locale, "err-cannot-change-guest-password")
}

/// Get translated "cannot delete guest" error
pub fn err_cannot_delete_guest(locale: &str) -> String {
    t(locale, "err-cannot-delete-guest")
}

// ========================================================================
// Account & Session Errors
// ========================================================================

/// Get translated "account deleted" error
pub fn err_account_deleted(locale: &str) -> String {
    t(locale, "err-account-deleted")
}

/// Get translated "account disabled" error
pub fn err_account_disabled(locale: &str, username: &str) -> String {
    t_args(locale, "err-account-disabled", &[("username", username)])
}

/// Get translated "account disabled by admin" error
pub fn err_account_disabled_by_admin(locale: &str) -> String {
    t(locale, "err-account-disabled-by-admin")
}

/// Get translated "already logged in" error
pub fn err_already_logged_in(locale: &str) -> String {
    t(locale, "err-already-logged-in")
}

/// Get translated "authentication" error
pub fn err_authentication(locale: &str) -> String {
    t(locale, "err-authentication")
}

/// Get translated "avatar invalid format" error
pub fn err_avatar_invalid_format(locale: &str) -> String {
    t(locale, "err-avatar-invalid-format")
}

/// Get translated "avatar too large" error
pub fn err_avatar_too_large(locale: &str, max_length: usize) -> String {
    t_args(
        locale,
        "err-avatar-too-large",
        &[("max_length", &max_length.to_string())],
    )
}

/// Get translated "avatar unsupported type" error
pub fn err_avatar_unsupported_type(locale: &str) -> String {
    t(locale, "err-avatar-unsupported-type")
}

/// Get translated "broadcast too long" error
pub fn err_broadcast_too_long(locale: &str, max_length: usize) -> String {
    t_args(
        locale,
        "err-broadcast-too-long",
        &[("max_length", &max_length.to_string())],
    )
}

/// Get translated "cannot create admin" error
pub fn err_cannot_create_admin(locale: &str) -> String {
    t(locale, "err-cannot-create-admin")
}

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

/// Get translated "cannot disable last admin" error
pub fn err_cannot_disable_last_admin(locale: &str) -> String {
    t(locale, "err-cannot-disable-last-admin")
}

/// Get translated "cannot edit self" error
pub fn err_cannot_edit_self(locale: &str) -> String {
    t(locale, "err-cannot-edit-self")
}

/// Get translated "current password incorrect" error
pub fn err_current_password_incorrect(locale: &str) -> String {
    t(locale, "err-current-password-incorrect")
}

/// Get translated "current password required" error
pub fn err_current_password_required(locale: &str) -> String {
    t(locale, "err-current-password-required")
}

/// Get translated "cannot kick admin" error
pub fn err_cannot_kick_admin(locale: &str) -> String {
    t(locale, "err-cannot-kick-admin")
}

/// Get translated "cannot delete admin" error
pub fn err_cannot_delete_admin(locale: &str) -> String {
    t(locale, "err-cannot-delete-admin")
}

/// Get translated "cannot edit admin" error
pub fn err_cannot_edit_admin(locale: &str) -> String {
    t(locale, "err-cannot-edit-admin")
}

/// Get translated "cannot kick self" error
pub fn err_cannot_kick_self(locale: &str) -> String {
    t(locale, "err-cannot-kick-self")
}

/// Get translated "cannot message self" error
pub fn err_cannot_message_self(locale: &str) -> String {
    t(locale, "err-cannot-message-self")
}

/// Get translated "chat feature not enabled" error
pub fn err_chat_feature_not_enabled(locale: &str) -> String {
    t(locale, "err-chat-feature-not-enabled")
}

/// Get translated "chat too long" error
pub fn err_chat_too_long(locale: &str, max_length: usize) -> String {
    t_args(
        locale,
        "err-chat-too-long",
        &[("max_length", &max_length.to_string())],
    )
}

/// Get translated "database" error
pub fn err_database(locale: &str) -> String {
    t(locale, "err-database")
}

/// Get translated "failed to create user" error
pub fn err_failed_to_create_user(locale: &str, username: &str) -> String {
    t_args(
        locale,
        "err-failed-to-create-user",
        &[("username", username)],
    )
}

/// Get translated "features empty feature" error
pub fn err_features_empty_feature(locale: &str) -> String {
    t(locale, "err-features-empty-feature")
}

/// Get translated "features feature too long" error
pub fn err_features_feature_too_long(locale: &str, max_length: usize) -> String {
    t_args(
        locale,
        "err-features-feature-too-long",
        &[("max_length", &max_length.to_string())],
    )
}

/// Get translated "features invalid characters" error
pub fn err_features_invalid_characters(locale: &str) -> String {
    t(locale, "err-features-invalid-characters")
}

/// Get translated "features too many" error
pub fn err_features_too_many(locale: &str, max_count: usize) -> String {
    t_args(
        locale,
        "err-features-too-many",
        &[("max_count", &max_count.to_string())],
    )
}

/// Get translated "handshake already completed" error
pub fn err_handshake_already_completed(locale: &str) -> String {
    t(locale, "err-handshake-already-completed")
}

/// Get translated "handshake required" error
pub fn err_handshake_required(locale: &str) -> String {
    t(locale, "err-handshake-required")
}

/// Get translated "invalid credentials" error
pub fn err_invalid_credentials(locale: &str) -> String {
    t(locale, "err-invalid-credentials")
}

/// Get translated "invalid message format" error
pub fn err_invalid_message_format(locale: &str) -> String {
    t(locale, "err-invalid-message-format")
}

/// Get translated "message not supported" error
pub fn err_message_not_supported(locale: &str) -> String {
    t(locale, "err-message-not-supported")
}

/// Get translated "kicked by" message
pub fn err_kicked_by(locale: &str, username: &str) -> String {
    t_args(locale, "err-kicked-by", &[("username", username)])
}

/// Get translated "kicked by" message with reason
pub fn err_kicked_by_with_reason(locale: &str, username: &str, reason: &str) -> String {
    t_args(
        locale,
        "err-kicked-by-reason",
        &[("username", username), ("reason", reason)],
    )
}

/// Get translated "locale invalid characters" error
pub fn err_locale_invalid_characters(locale: &str) -> String {
    t(locale, "err-locale-invalid-characters")
}

/// Get translated "locale too long" error
pub fn err_locale_too_long(locale: &str, max_length: usize) -> String {
    t_args(
        locale,
        "err-locale-too-long",
        &[("max_length", &max_length.to_string())],
    )
}

/// Get translated "message contains newlines" error
pub fn err_message_contains_newlines(locale: &str) -> String {
    t(locale, "err-message-contains-newlines")
}

/// Get translated "message empty" error
pub fn err_message_empty(locale: &str) -> String {
    t(locale, "err-message-empty")
}

/// Get translated "message invalid characters" error
pub fn err_message_invalid_characters(locale: &str) -> String {
    t(locale, "err-message-invalid-characters")
}

/// Get translated "not logged in" error
pub fn err_not_logged_in(locale: &str) -> String {
    t(locale, "err-not-logged-in")
}

/// Get translated "password empty" error
pub fn err_password_empty(locale: &str) -> String {
    t(locale, "err-password-empty")
}

/// Get translated "password too long" error
pub fn err_password_too_long(locale: &str, max_length: usize) -> String {
    t_args(
        locale,
        "err-password-too-long",
        &[("max_length", &max_length.to_string())],
    )
}

/// Get translated "permission denied" error
pub fn err_permission_denied(locale: &str) -> String {
    t(locale, "err-permission-denied")
}

/// Get translated "permissions contains newlines" error
pub fn err_permissions_contains_newlines(locale: &str) -> String {
    t(locale, "err-permissions-contains-newlines")
}

/// Get translated "permissions empty permission" error
pub fn err_permissions_empty_permission(locale: &str) -> String {
    t(locale, "err-permissions-empty-permission")
}

/// Get translated "permissions invalid characters" error
pub fn err_permissions_invalid_characters(locale: &str) -> String {
    t(locale, "err-permissions-invalid-characters")
}

/// Get translated "permissions permission too long" error
pub fn err_permissions_permission_too_long(locale: &str, max_length: usize) -> String {
    t_args(
        locale,
        "err-permissions-permission-too-long",
        &[("max_length", &max_length.to_string())],
    )
}

/// Get translated "permissions too many" error
pub fn err_permissions_too_many(locale: &str, max_count: usize) -> String {
    t_args(
        locale,
        "err-permissions-too-many",
        &[("max_count", &max_count.to_string())],
    )
}

/// Get translated "topic contains newlines" error
pub fn err_topic_contains_newlines(locale: &str) -> String {
    t(locale, "err-topic-contains-newlines")
}

/// Get translated "topic invalid characters" error
pub fn err_topic_invalid_characters(locale: &str) -> String {
    t(locale, "err-topic-invalid-characters")
}

/// Get translated "topic too long" error
pub fn err_topic_too_long(locale: &str, max_length: usize) -> String {
    t_args(
        locale,
        "err-topic-too-long",
        &[("max_length", &max_length.to_string())],
    )
}

/// Get translated "unknown permission" error
pub fn err_unknown_permission(locale: &str, permission: &str) -> String {
    t_args(
        locale,
        "err-unknown-permission",
        &[("permission", permission)],
    )
}

/// Get translated "update failed" error
pub fn err_update_failed(locale: &str, username: &str) -> String {
    t_args(locale, "err-update-failed", &[("username", username)])
}

/// Get translated "user not found" error (for admin operations using account identifier)
pub fn err_user_not_found(locale: &str, username: &str) -> String {
    t_args(locale, "err-user-not-found", &[("username", username)])
}

/// Get translated "nickname not online" error (for user operations using display name)
pub fn err_nickname_not_online(locale: &str, nickname: &str) -> String {
    t_args(locale, "err-nickname-not-online", &[("nickname", nickname)])
}

/// Get translated "username empty" error
pub fn err_username_empty(locale: &str) -> String {
    t(locale, "err-username-empty")
}

/// Get translated "username exists" error
pub fn err_username_exists(locale: &str, username: &str) -> String {
    t_args(locale, "err-username-exists", &[("username", username)])
}

/// Get translated "username invalid" error
pub fn err_username_invalid(locale: &str) -> String {
    t(locale, "err-username-invalid")
}

/// Get translated "username too long" error
pub fn err_username_too_long(locale: &str, max_length: usize) -> String {
    t_args(
        locale,
        "err-username-too-long",
        &[("max_length", &max_length.to_string())],
    )
}

/// Get translated "version empty" error
pub fn err_version_empty(locale: &str) -> String {
    t(locale, "err-version-empty")
}

/// Get translated "version invalid semver" error
pub fn err_version_invalid_semver(locale: &str) -> String {
    t(locale, "err-version-invalid-semver")
}

/// Get translated "version major mismatch" error
pub fn err_version_major_mismatch(locale: &str, server_major: u64, client_major: u64) -> String {
    t_args(
        locale,
        "err-version-major-mismatch",
        &[
            ("server_major", &server_major.to_string()),
            ("client_major", &client_major.to_string()),
        ],
    )
}

/// Get translated "version client too new" error
pub fn err_version_client_too_new(
    locale: &str,
    server_version: &str,
    client_version: &str,
) -> String {
    t_args(
        locale,
        "err-version-client-too-new",
        &[
            ("server_version", server_version),
            ("client_version", client_version),
        ],
    )
}

/// Get translated "version too long" error
pub fn err_version_too_long(locale: &str, max_length: usize) -> String {
    t_args(
        locale,
        "err-version-too-long",
        &[("max_length", &max_length.to_string())],
    )
}

/// Get translated "admin required" error
pub fn err_admin_required(locale: &str) -> String {
    t(locale, "err-admin-required")
}

/// Get translated "server name empty" error
pub fn err_server_name_empty(locale: &str) -> String {
    t(locale, "err-server-name-empty")
}

/// Get translated "server name too long" error
pub fn err_server_name_too_long(locale: &str, max_length: usize) -> String {
    t_args(
        locale,
        "err-server-name-too-long",
        &[("max_length", &max_length.to_string())],
    )
}

/// Get translated "server name contains newlines" error
pub fn err_server_name_contains_newlines(locale: &str) -> String {
    t(locale, "err-server-name-contains-newlines")
}

/// Get translated "server name invalid characters" error
pub fn err_server_name_invalid_characters(locale: &str) -> String {
    t(locale, "err-server-name-invalid-characters")
}

/// Get translated "server description too long" error
pub fn err_server_description_too_long(locale: &str, max_length: usize) -> String {
    t_args(
        locale,
        "err-server-description-too-long",
        &[("max_length", &max_length.to_string())],
    )
}

/// Get translated "server description contains newlines" error
pub fn err_server_description_contains_newlines(locale: &str) -> String {
    t(locale, "err-server-description-contains-newlines")
}

/// Get translated "server description invalid characters" error
pub fn err_server_description_invalid_characters(locale: &str) -> String {
    t(locale, "err-server-description-invalid-characters")
}

/// Get translated "server image too large" error
pub fn err_server_image_too_large(locale: &str) -> String {
    t(locale, "err-server-image-too-large")
}

/// Get translated "server image invalid format" error
pub fn err_server_image_invalid_format(locale: &str) -> String {
    t(locale, "err-server-image-invalid-format")
}

/// Get translated "server image unsupported type" error
pub fn err_server_image_unsupported_type(locale: &str) -> String {
    t(locale, "err-server-image-unsupported-type")
}

/// Get translated "no fields to update" error
pub fn err_no_fields_to_update(locale: &str) -> String {
    t(locale, "err-no-fields-to-update")
}

/// Get translated "news not found" error
pub fn err_news_not_found(locale: &str, id: i64) -> String {
    t_args(locale, "err-news-not-found", &[("id", &id.to_string())])
}

/// Get translated "news body too long" error
pub fn err_news_body_too_long(locale: &str, max_length: usize) -> String {
    t_args(
        locale,
        "err-news-body-too-long",
        &[("max_length", &max_length.to_string())],
    )
}

/// Get translated "news body invalid characters" error
pub fn err_news_body_invalid_characters(locale: &str) -> String {
    t(locale, "err-news-body-invalid-characters")
}

/// Get translated "news image too large" error
pub fn err_news_image_too_large(locale: &str) -> String {
    t(locale, "err-news-image-too-large")
}

/// Get translated "news image invalid format" error
pub fn err_news_image_invalid_format(locale: &str) -> String {
    t(locale, "err-news-image-invalid-format")
}

/// Get translated "news image unsupported type" error
pub fn err_news_image_unsupported_type(locale: &str) -> String {
    t(locale, "err-news-image-unsupported-type")
}

/// Get translated "news empty content" error (neither body nor image provided)
pub fn err_news_empty_content(locale: &str) -> String {
    t(locale, "err-news-empty-content")
}

/// Get translated "cannot edit admin news" error
pub fn err_cannot_edit_admin_news(locale: &str) -> String {
    t(locale, "err-cannot-edit-admin-news")
}

/// Get translated "cannot delete admin news" error
pub fn err_cannot_delete_admin_news(locale: &str) -> String {
    t(locale, "err-cannot-delete-admin-news")
}

// =============================================================================
// File Area Errors
// =============================================================================

/// Get translated "file path too long" error
pub fn err_file_path_too_long(locale: &str, max_length: usize) -> String {
    t_args(
        locale,
        "err-file-path-too-long",
        &[("max_length", &max_length.to_string())],
    )
}

/// Get translated "file path invalid" error
pub fn err_file_path_invalid(locale: &str) -> String {
    t(locale, "err-file-path-invalid")
}

/// Get translated "file not found" error
pub fn err_file_not_found(locale: &str) -> String {
    t(locale, "err-file-not-found")
}

/// Get translated "file not directory" error
pub fn err_file_not_directory(locale: &str) -> String {
    t(locale, "err-file-not-directory")
}

/// Get translated "directory name empty" error
pub fn err_dir_name_empty(locale: &str) -> String {
    t(locale, "err-dir-name-empty")
}

/// Get translated "directory name too long" error
pub fn err_dir_name_too_long(locale: &str, max_length: usize) -> String {
    t_args(
        locale,
        "err-dir-name-too-long",
        &[("max_length", &max_length.to_string())],
    )
}

/// Get translated "directory name invalid" error
pub fn err_dir_name_invalid(locale: &str) -> String {
    t(locale, "err-dir-name-invalid")
}

/// Get translated "directory already exists" error
pub fn err_dir_already_exists(locale: &str) -> String {
    t(locale, "err-dir-already-exists")
}

/// Get translated "directory creation failed" error
pub fn err_dir_create_failed(locale: &str) -> String {
    t(locale, "err-dir-create-failed")
}

/// Get translated "directory not empty" error
pub fn err_dir_not_empty(locale: &str) -> String {
    t(locale, "err-dir-not-empty")
}

/// Get translated "delete failed" error
pub fn err_delete_failed(locale: &str) -> String {
    t(locale, "err-delete-failed")
}

/// Get translated "rename failed" error
pub fn err_rename_failed(locale: &str) -> String {
    t(locale, "err-rename-failed")
}

/// Get translated "rename target exists" error
pub fn err_rename_target_exists(locale: &str) -> String {
    t(locale, "err-rename-target-exists")
}

/// Get translated "move failed" error
pub fn err_move_failed(locale: &str) -> String {
    t(locale, "err-move-failed")
}

/// Get translated "copy failed" error
pub fn err_copy_failed(locale: &str) -> String {
    t(locale, "err-copy-failed")
}

/// Get translated "destination exists" error
pub fn err_destination_exists(locale: &str) -> String {
    t(locale, "err-destination-exists")
}

/// Get translated "cannot move into itself" error
pub fn err_cannot_move_into_itself(locale: &str) -> String {
    t(locale, "err-cannot-move-into-itself")
}

/// Get translated "cannot copy into itself" error
pub fn err_cannot_copy_into_itself(locale: &str) -> String {
    t(locale, "err-cannot-copy-into-itself")
}

/// Get translated "destination not a directory" error
pub fn err_destination_not_directory(locale: &str) -> String {
    t(locale, "err-destination-not-directory")
}

// =============================================================================
// File Transfer Errors
// =============================================================================

/// Get translated "file area not configured" error
pub fn err_file_area_not_configured(locale: &str) -> String {
    t(locale, "err-file-area-not-configured")
}

/// Get translated "file area not accessible" error
pub fn err_file_area_not_accessible(locale: &str) -> String {
    t(locale, "err-file-area-not-accessible")
}

/// Get translated "transfer path too long" error
pub fn err_transfer_path_too_long(locale: &str) -> String {
    t(locale, "err-transfer-path-too-long")
}

/// Get translated "transfer path invalid" error
pub fn err_transfer_path_invalid(locale: &str) -> String {
    t(locale, "err-transfer-path-invalid")
}

/// Get translated "transfer access denied" error
pub fn err_transfer_access_denied(locale: &str) -> String {
    t(locale, "err-transfer-access-denied")
}

/// Get translated "transfer read failed" error
pub fn err_transfer_read_failed(locale: &str) -> String {
    t(locale, "err-transfer-read-failed")
}

/// Get translated "transfer path not found" error
pub fn err_transfer_path_not_found(locale: &str) -> String {
    t(locale, "err-transfer-path-not-found")
}

/// Get translated "transfer file failed" error
pub fn err_transfer_file_failed(locale: &str, path: &str, error: &str) -> String {
    t_args(
        locale,
        "err-transfer-file-failed",
        &[("path", path), ("error", error)],
    )
}

/// Get translated "upload destination not allowed" error
pub fn err_upload_destination_not_allowed(locale: &str) -> String {
    t(locale, "err-upload-destination-not-allowed")
}

/// Get translated "upload write failed" error
pub fn err_upload_write_failed(locale: &str) -> String {
    t(locale, "err-upload-write-failed")
}

/// Get translated "upload hash mismatch" error
pub fn err_upload_hash_mismatch(locale: &str) -> String {
    t(locale, "err-upload-hash-mismatch")
}

/// Get translated "upload path invalid" error
pub fn err_upload_path_invalid(locale: &str) -> String {
    t(locale, "err-upload-path-invalid")
}

/// Get translated "upload conflict" error (another upload to same filename in progress)
pub fn err_upload_conflict(locale: &str) -> String {
    t(locale, "err-upload-conflict")
}

/// Get translated "upload file exists" error (file already exists with different content)
pub fn err_upload_file_exists(locale: &str) -> String {
    t(locale, "err-upload-file-exists")
}

/// Get translated "upload empty" error (upload must contain at least one file)
pub fn err_upload_empty(locale: &str) -> String {
    t(locale, "err-upload-empty")
}

/// Get translated "upload protocol error" error
pub fn err_upload_protocol_error(locale: &str) -> String {
    t(locale, "err-upload-protocol-error")
}

/// Get translated "upload connection lost" error
pub fn err_upload_connection_lost(locale: &str) -> String {
    t(locale, "err-upload-connection-lost")
}

// =============================================================================
// Ban System Errors
// =============================================================================

/// Get translated "ban self" error (cannot ban yourself)
pub fn err_ban_self(locale: &str) -> String {
    t(locale, "err-ban-self")
}

/// Get translated "ban admin by nickname" error (cannot ban administrators)
pub fn err_ban_admin_by_nickname(locale: &str) -> String {
    t(locale, "err-ban-admin-by-nickname")
}

/// Get translated "ban admin by IP" error (cannot ban this IP - generic to prevent info leak)
pub fn err_ban_admin_by_ip(locale: &str) -> String {
    t(locale, "err-ban-admin-by-ip")
}

/// Get translated "ban invalid target" error (invalid IP address or hostname)
pub fn err_ban_invalid_target(locale: &str) -> String {
    t(locale, "err-ban-invalid-target")
}

/// Get translated "ban invalid duration" error (invalid duration format)
pub fn err_ban_invalid_duration(locale: &str) -> String {
    t(locale, "err-ban-invalid-duration")
}

/// Get translated "ban not found" error (no ban found for target)
pub fn err_ban_not_found(locale: &str, target: &str) -> String {
    t_args(locale, "err-ban-not-found", &[("target", target)])
}

/// Get translated "reason too long" error
pub fn err_reason_too_long(locale: &str, max_length: usize) -> String {
    t_args(
        locale,
        "err-reason-too-long",
        &[("max_length", &max_length.to_string())],
    )
}

/// Get translated "reason invalid" error (contains control characters)
pub fn err_reason_invalid(locale: &str) -> String {
    t(locale, "err-reason-invalid")
}

/// Get translated "banned permanent" error (sent to user being disconnected)
pub fn err_banned_permanent(locale: &str) -> String {
    t(locale, "err-banned-permanent")
}

/// Get translated "banned with expiry" error (sent to user being disconnected)
pub fn err_banned_with_expiry(locale: &str, remaining: &str) -> String {
    t_args(
        locale,
        "err-banned-with-expiry",
        &[("remaining", remaining)],
    )
}
