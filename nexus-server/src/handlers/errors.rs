//! Translated user-facing error messages for handlers (via i18n).

use nexus_common::validators::ChannelError;

use crate::i18n::{t, t_args};

pub fn err_nickname_empty(locale: &str) -> String {
    t(locale, "err-nickname-empty")
}

pub fn err_nickname_in_use(locale: &str) -> String {
    t(locale, "err-nickname-in-use")
}

pub fn err_nickname_invalid(locale: &str) -> String {
    t(locale, "err-nickname-invalid")
}

pub fn err_nickname_is_username(locale: &str) -> String {
    t(locale, "err-nickname-is-username")
}

pub fn err_nickname_required(locale: &str) -> String {
    t(locale, "err-nickname-required")
}

pub fn err_nickname_too_long(locale: &str, max_length: usize) -> String {
    t_args(
        locale,
        "err-nickname-too-long",
        &[("max_length", &max_length.to_string())],
    )
}

pub fn err_shared_cannot_be_admin(locale: &str) -> String {
    t(locale, "err-shared-cannot-be-admin")
}

pub fn err_shared_cannot_self_edit(locale: &str) -> String {
    t(locale, "err-shared-cannot-self-edit")
}

pub fn err_shared_invalid_permissions(locale: &str, permissions: &str) -> String {
    t_args(
        locale,
        "err-shared-invalid-permissions",
        &[("permissions", permissions)],
    )
}

pub fn err_status_too_long(locale: &str, max_length: usize) -> String {
    t_args(
        locale,
        "err-status-too-long",
        &[("max_length", &max_length.to_string())],
    )
}

pub fn err_status_contains_newlines(locale: &str) -> String {
    t(locale, "err-status-contains-newlines")
}

pub fn err_status_invalid_characters(locale: &str) -> String {
    t(locale, "err-status-invalid-characters")
}

pub fn err_guest_disabled(locale: &str) -> String {
    t(locale, "err-guest-disabled")
}

pub fn err_cannot_rename_guest(locale: &str) -> String {
    t(locale, "err-cannot-rename-guest")
}

pub fn err_cannot_change_guest_password(locale: &str) -> String {
    t(locale, "err-cannot-change-guest-password")
}

pub fn err_cannot_delete_guest(locale: &str) -> String {
    t(locale, "err-cannot-delete-guest")
}

pub fn err_account_deleted(locale: &str) -> String {
    t(locale, "err-account-deleted")
}

pub fn err_account_disabled(locale: &str, username: &str) -> String {
    t_args(locale, "err-account-disabled", &[("username", username)])
}

pub fn err_account_disabled_by_admin(locale: &str) -> String {
    t(locale, "err-account-disabled-by-admin")
}

pub fn err_already_logged_in(locale: &str) -> String {
    t(locale, "err-already-logged-in")
}

pub fn err_authentication(locale: &str) -> String {
    t(locale, "err-authentication")
}

pub fn err_avatar_invalid_format(locale: &str) -> String {
    t(locale, "err-avatar-invalid-format")
}

pub fn err_avatar_too_large(locale: &str, max_length: usize) -> String {
    t_args(
        locale,
        "err-avatar-too-large",
        &[("max_length", &max_length.to_string())],
    )
}

pub fn err_avatar_unsupported_type(locale: &str) -> String {
    t(locale, "err-avatar-unsupported-type")
}

pub fn err_avatar_undecodable(locale: &str) -> String {
    t(locale, "err-avatar-undecodable")
}

pub fn err_broadcast_too_long(locale: &str, max_length: usize) -> String {
    t_args(
        locale,
        "err-broadcast-too-long",
        &[("max_length", &max_length.to_string())],
    )
}

pub fn err_cannot_create_admin(locale: &str) -> String {
    t(locale, "err-cannot-create-admin")
}

pub fn err_admin_cannot_have_group(locale: &str) -> String {
    t(locale, "err-admin-cannot-have-group")
}

pub fn err_cannot_delete_last_admin(locale: &str) -> String {
    t(locale, "err-cannot-delete-last-admin")
}

pub fn err_cannot_delete_self(locale: &str) -> String {
    t(locale, "err-cannot-delete-self")
}

pub fn err_cannot_demote_last_admin(locale: &str) -> String {
    t(locale, "err-cannot-demote-last-admin")
}

pub fn err_cannot_disable_last_admin(locale: &str) -> String {
    t(locale, "err-cannot-disable-last-admin")
}

pub fn err_cannot_edit_self(locale: &str) -> String {
    t(locale, "err-cannot-edit-self")
}

pub fn err_current_password_incorrect(locale: &str) -> String {
    t(locale, "err-current-password-incorrect")
}

pub fn err_current_password_required(locale: &str) -> String {
    t(locale, "err-current-password-required")
}

pub fn err_cannot_kick_admin(locale: &str) -> String {
    t(locale, "err-cannot-kick-admin")
}

pub fn err_cannot_delete_admin(locale: &str) -> String {
    t(locale, "err-cannot-delete-admin")
}

pub fn err_cannot_edit_admin(locale: &str) -> String {
    t(locale, "err-cannot-edit-admin")
}

pub fn err_cannot_kick_self(locale: &str) -> String {
    t(locale, "err-cannot-kick-self")
}

pub fn err_cannot_message_self(locale: &str) -> String {
    t(locale, "err-cannot-message-self")
}

pub fn err_chat_feature_not_enabled(locale: &str) -> String {
    t(locale, "err-chat-feature-not-enabled")
}

pub fn err_chat_target_feature_not_enabled(locale: &str, nickname: &str) -> String {
    t_args(
        locale,
        "err-chat-target-feature-not-enabled",
        &[("nickname", nickname)],
    )
}

pub fn err_chat_too_long(locale: &str, max_length: usize) -> String {
    t_args(
        locale,
        "err-chat-too-long",
        &[("max_length", &max_length.to_string())],
    )
}

/// Maps a `ChannelError` to its localized message (shared by all chat handlers).
pub fn channel_error_to_message(e: ChannelError, locale: &str) -> String {
    match e {
        ChannelError::Empty => err_channel_name_empty(locale),
        ChannelError::TooShort => err_channel_name_too_short(locale),
        ChannelError::TooLong => {
            err_channel_name_too_long(locale, nexus_common::validators::MAX_CHANNEL_LENGTH)
        }
        ChannelError::MissingPrefix => err_channel_name_missing_prefix(locale),
        ChannelError::InvalidCharacters => err_channel_name_invalid(locale),
    }
}

pub fn err_channel_name_empty(locale: &str) -> String {
    t(locale, "err-channel-name-empty")
}

pub fn err_channel_name_too_long(locale: &str, max_length: usize) -> String {
    t_args(
        locale,
        "err-channel-name-too-long",
        &[("max_length", &max_length.to_string())],
    )
}

pub fn err_channel_name_too_short(locale: &str) -> String {
    t(locale, "err-channel-name-too-short")
}

pub fn err_channel_name_invalid(locale: &str) -> String {
    t(locale, "err-channel-name-invalid")
}

pub fn err_channel_name_missing_prefix(locale: &str) -> String {
    t(locale, "err-channel-name-missing-prefix")
}

pub fn err_channel_list_invalid(locale: &str, channel: &str, reason: &str) -> String {
    t_args(
        locale,
        "err-channel-list-invalid",
        &[("channel", channel), ("reason", reason)],
    )
}

pub fn err_channel_not_found(locale: &str, channel: &str) -> String {
    t_args(locale, "err-channel-not-found", &[("channel", channel)])
}

pub fn err_channel_already_member(locale: &str, channel: &str) -> String {
    t_args(
        locale,
        "err-channel-already-member",
        &[("channel", channel)],
    )
}

pub fn err_channel_limit_exceeded(locale: &str, max: usize) -> String {
    t_args(
        locale,
        "err-channel-limit-exceeded",
        &[("max", &max.to_string())],
    )
}

pub fn err_database(locale: &str) -> String {
    t(locale, "err-database")
}

/// Non-user, non-DB failures — e.g. Argon2id hashing fails to allocate or run.
pub fn err_internal_error(locale: &str) -> String {
    t(locale, "err-internal-error")
}

pub fn err_login_permissions_failed(locale: &str) -> String {
    t(locale, "err-login-permissions-failed")
}

pub fn err_login_group_failed(locale: &str) -> String {
    t(locale, "err-login-group-failed")
}

pub fn err_login_bandwidth_failed(locale: &str) -> String {
    t(locale, "err-login-bandwidth-failed")
}

pub fn err_failed_to_create_user(locale: &str, username: &str) -> String {
    t_args(
        locale,
        "err-failed-to-create-user",
        &[("username", username)],
    )
}

pub fn err_features_empty_feature(locale: &str) -> String {
    t(locale, "err-features-empty-feature")
}

pub fn err_features_feature_too_long(locale: &str, max_length: usize) -> String {
    t_args(
        locale,
        "err-features-feature-too-long",
        &[("max_length", &max_length.to_string())],
    )
}

pub fn err_features_invalid_characters(locale: &str) -> String {
    t(locale, "err-features-invalid-characters")
}

pub fn err_features_too_many(locale: &str, max_count: usize) -> String {
    t_args(
        locale,
        "err-features-too-many",
        &[("max_count", &max_count.to_string())],
    )
}

pub fn err_handshake_already_completed(locale: &str) -> String {
    t(locale, "err-handshake-already-completed")
}

pub fn err_handshake_required(locale: &str) -> String {
    t(locale, "err-handshake-required")
}

pub fn err_invalid_credentials(locale: &str) -> String {
    t(locale, "err-invalid-credentials")
}

pub fn err_invalid_message_format(locale: &str) -> String {
    t(locale, "err-invalid-message-format")
}

pub fn err_unexpected_message_type(locale: &str) -> String {
    t(locale, "err-unexpected-message-type")
}

pub fn err_message_not_supported(locale: &str) -> String {
    t(locale, "err-message-not-supported")
}

pub fn err_kicked_by(locale: &str, username: &str) -> String {
    t_args(locale, "err-kicked-by", &[("username", username)])
}

pub fn err_kicked_by_with_reason(locale: &str, username: &str, reason: &str) -> String {
    t_args(
        locale,
        "err-kicked-by-reason",
        &[("username", username), ("reason", reason)],
    )
}

pub fn err_kick_reason_too_long(locale: &str, max_length: usize) -> String {
    t_args(
        locale,
        "err-kick-reason-too-long",
        &[("max_length", &max_length.to_string())],
    )
}

pub fn err_kick_reason_invalid_characters(locale: &str) -> String {
    t(locale, "err-kick-reason-invalid-characters")
}

pub fn err_locale_invalid_characters(locale: &str) -> String {
    t(locale, "err-locale-invalid-characters")
}

pub fn err_locale_too_long(locale: &str, max_length: usize) -> String {
    t_args(
        locale,
        "err-locale-too-long",
        &[("max_length", &max_length.to_string())],
    )
}

pub fn err_message_contains_newlines(locale: &str) -> String {
    t(locale, "err-message-contains-newlines")
}

pub fn err_message_empty(locale: &str) -> String {
    t(locale, "err-message-empty")
}

pub fn err_message_invalid_characters(locale: &str) -> String {
    t(locale, "err-message-invalid-characters")
}

pub fn err_not_logged_in(locale: &str) -> String {
    t(locale, "err-not-logged-in")
}

pub fn err_password_empty(locale: &str) -> String {
    t(locale, "err-password-empty")
}

pub fn err_password_too_long(locale: &str, max_length: usize) -> String {
    t_args(
        locale,
        "err-password-too-long",
        &[("max_length", &max_length.to_string())],
    )
}

pub fn err_password_too_weak(locale: &str, required: u8) -> String {
    t_args(
        locale,
        "err-password-too-weak",
        &[("required", &required.to_string())],
    )
}

pub fn err_permission_denied(locale: &str) -> String {
    t(locale, "err-permission-denied")
}

pub fn err_permissions_contains_newlines(locale: &str) -> String {
    t(locale, "err-permissions-contains-newlines")
}

pub fn err_permissions_empty_permission(locale: &str) -> String {
    t(locale, "err-permissions-empty-permission")
}

pub fn err_permissions_invalid_characters(locale: &str) -> String {
    t(locale, "err-permissions-invalid-characters")
}

pub fn err_permissions_permission_too_long(locale: &str, max_length: usize) -> String {
    t_args(
        locale,
        "err-permissions-permission-too-long",
        &[("max_length", &max_length.to_string())],
    )
}

pub fn err_permissions_too_many(locale: &str, max_count: usize) -> String {
    t_args(
        locale,
        "err-permissions-too-many",
        &[("max_count", &max_count.to_string())],
    )
}

pub fn err_permission_grant_revoke_conflict(locale: &str, permission: &str) -> String {
    t_args(
        locale,
        "err-permission-grant-revoke-conflict",
        &[("permission", permission)],
    )
}

pub fn err_topic_contains_newlines(locale: &str) -> String {
    t(locale, "err-topic-contains-newlines")
}

pub fn err_topic_invalid_characters(locale: &str) -> String {
    t(locale, "err-topic-invalid-characters")
}

pub fn err_topic_too_long(locale: &str, max_length: usize) -> String {
    t_args(
        locale,
        "err-topic-too-long",
        &[("max_length", &max_length.to_string())],
    )
}

pub fn err_unknown_permission(locale: &str, permission: &str) -> String {
    t_args(
        locale,
        "err-unknown-permission",
        &[("permission", permission)],
    )
}

pub fn err_update_failed(locale: &str, username: &str) -> String {
    t_args(locale, "err-update-failed", &[("username", username)])
}

/// Admin operations key on the account identifier (username).
pub fn err_user_not_found(locale: &str, username: &str) -> String {
    t_args(locale, "err-user-not-found", &[("username", username)])
}

/// User operations key on the display name (nickname).
pub fn err_nickname_not_online(locale: &str, nickname: &str) -> String {
    t_args(locale, "err-nickname-not-online", &[("nickname", nickname)])
}

pub fn err_username_empty(locale: &str) -> String {
    t(locale, "err-username-empty")
}

pub fn err_username_exists(locale: &str, username: &str) -> String {
    t_args(locale, "err-username-exists", &[("username", username)])
}

pub fn err_personal_file_area_exists(locale: &str, username: &str) -> String {
    t_args(
        locale,
        "err-personal-file-area-exists",
        &[("username", username)],
    )
}

pub fn err_personal_file_area_migration_failed(locale: &str) -> String {
    t(locale, "err-personal-file-area-migration-failed")
}

pub fn err_personal_file_area_busy(locale: &str) -> String {
    t(locale, "err-personal-file-area-busy")
}

pub fn err_personal_file_area_rollback_failed_warning(
    locale: &str,
    old_username: &str,
    new_username: &str,
) -> String {
    t_args(
        locale,
        "err-personal-file-area-rollback-failed-warning",
        &[
            ("old_username", old_username),
            ("new_username", new_username),
        ],
    )
}

pub fn err_username_invalid(locale: &str) -> String {
    t(locale, "err-username-invalid")
}

pub fn err_username_is_active_nickname(locale: &str) -> String {
    t(locale, "err-username-is-active-nickname")
}

pub fn err_username_too_long(locale: &str, max_length: usize) -> String {
    t_args(
        locale,
        "err-username-too-long",
        &[("max_length", &max_length.to_string())],
    )
}

pub fn err_version_empty(locale: &str) -> String {
    t(locale, "err-version-empty")
}

pub fn err_version_invalid_semver(locale: &str) -> String {
    t(locale, "err-version-invalid-semver")
}

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

/// Pre-1.0 minor-version incompatibility.
pub fn err_version_minor_mismatch(
    locale: &str,
    server_version: &str,
    client_version: &str,
) -> String {
    t_args(
        locale,
        "err-version-minor-mismatch",
        &[
            ("server_version", server_version),
            ("client_version", client_version),
        ],
    )
}

pub fn err_version_too_long(locale: &str, max_length: usize) -> String {
    t_args(
        locale,
        "err-version-too-long",
        &[("max_length", &max_length.to_string())],
    )
}

pub fn err_admin_required(locale: &str) -> String {
    t(locale, "err-admin-required")
}

pub fn err_server_name_empty(locale: &str) -> String {
    t(locale, "err-server-name-empty")
}

pub fn err_server_name_too_long(locale: &str, max_length: usize) -> String {
    t_args(
        locale,
        "err-server-name-too-long",
        &[("max_length", &max_length.to_string())],
    )
}

pub fn err_server_name_contains_newlines(locale: &str) -> String {
    t(locale, "err-server-name-contains-newlines")
}

pub fn err_server_name_invalid_characters(locale: &str) -> String {
    t(locale, "err-server-name-invalid-characters")
}

pub fn err_server_description_too_long(locale: &str, max_length: usize) -> String {
    t_args(
        locale,
        "err-server-description-too-long",
        &[("max_length", &max_length.to_string())],
    )
}

pub fn err_server_description_contains_newlines(locale: &str) -> String {
    t(locale, "err-server-description-contains-newlines")
}

pub fn err_server_description_invalid_characters(locale: &str) -> String {
    t(locale, "err-server-description-invalid-characters")
}

pub fn err_server_image_too_large(locale: &str) -> String {
    t(locale, "err-server-image-too-large")
}

pub fn err_server_image_invalid_format(locale: &str) -> String {
    t(locale, "err-server-image-invalid-format")
}

pub fn err_server_image_unsupported_type(locale: &str) -> String {
    t(locale, "err-server-image-unsupported-type")
}

pub fn err_server_image_undecodable(locale: &str) -> String {
    t(locale, "err-server-image-undecodable")
}

pub fn err_public_address_too_long(locale: &str, max_length: usize) -> String {
    t_args(
        locale,
        "err-public-address-too-long",
        &[("max_length", &max_length.to_string())],
    )
}

pub fn err_public_address_contains_scheme(locale: &str) -> String {
    t(locale, "err-public-address-contains-scheme")
}

pub fn err_public_address_contains_brackets(locale: &str) -> String {
    t(locale, "err-public-address-contains-brackets")
}

pub fn err_public_address_contains_path(locale: &str) -> String {
    t(locale, "err-public-address-contains-path")
}

pub fn err_public_address_contains_userinfo(locale: &str) -> String {
    t(locale, "err-public-address-contains-userinfo")
}

pub fn err_public_address_contains_whitespace(locale: &str) -> String {
    t(locale, "err-public-address-contains-whitespace")
}

pub fn err_public_address_contains_port(locale: &str) -> String {
    t(locale, "err-public-address-contains-port")
}

pub fn err_public_address_contains_zone_id(locale: &str) -> String {
    t(locale, "err-public-address-contains-zone-id")
}

pub fn err_public_address_invalid_format(locale: &str) -> String {
    t(locale, "err-public-address-invalid-format")
}

/// Distinct from invalid-format: tracker rows MUST have an address, whereas the
/// `ServerInfo.public_address` validator accepts empty for "unset".
pub fn err_tracker_address_empty(locale: &str) -> String {
    t(locale, "err-tracker-address-empty")
}

pub fn err_tracker_address_too_long(locale: &str, max_length: usize) -> String {
    t_args(
        locale,
        "err-tracker-address-too-long",
        &[("max_length", &max_length.to_string())],
    )
}

pub fn err_tracker_address_contains_scheme(locale: &str) -> String {
    t(locale, "err-tracker-address-contains-scheme")
}

pub fn err_tracker_address_contains_brackets(locale: &str) -> String {
    t(locale, "err-tracker-address-contains-brackets")
}

pub fn err_tracker_address_contains_path(locale: &str) -> String {
    t(locale, "err-tracker-address-contains-path")
}

pub fn err_tracker_address_contains_userinfo(locale: &str) -> String {
    t(locale, "err-tracker-address-contains-userinfo")
}

pub fn err_tracker_address_contains_whitespace(locale: &str) -> String {
    t(locale, "err-tracker-address-contains-whitespace")
}

pub fn err_tracker_address_contains_port(locale: &str) -> String {
    t(locale, "err-tracker-address-contains-port")
}

pub fn err_tracker_address_contains_zone_id(locale: &str) -> String {
    t(locale, "err-tracker-address-contains-zone-id")
}

pub fn err_tracker_address_invalid_format(locale: &str) -> String {
    t(locale, "err-tracker-address-invalid-format")
}

pub fn err_no_fields_to_update(locale: &str) -> String {
    t(locale, "err-no-fields-to-update")
}

pub fn err_invalid_password_strength(locale: &str) -> String {
    t(locale, "err-invalid-password-strength")
}

pub fn err_news_not_found(locale: &str, id: i64) -> String {
    t_args(locale, "err-news-not-found", &[("id", &id.to_string())])
}

pub fn err_news_body_too_long(locale: &str, max_length: usize) -> String {
    t_args(
        locale,
        "err-news-body-too-long",
        &[("max_length", &max_length.to_string())],
    )
}

pub fn err_news_body_invalid_characters(locale: &str) -> String {
    t(locale, "err-news-body-invalid-characters")
}

pub fn err_news_image_too_large(locale: &str) -> String {
    t(locale, "err-news-image-too-large")
}

pub fn err_news_image_invalid_format(locale: &str) -> String {
    t(locale, "err-news-image-invalid-format")
}

pub fn err_news_image_unsupported_type(locale: &str) -> String {
    t(locale, "err-news-image-unsupported-type")
}

pub fn err_news_image_undecodable(locale: &str) -> String {
    t(locale, "err-news-image-undecodable")
}

/// Fires when neither body nor image is provided.
pub fn err_news_empty_content(locale: &str) -> String {
    t(locale, "err-news-empty-content")
}

pub fn err_cannot_edit_admin_news(locale: &str) -> String {
    t(locale, "err-cannot-edit-admin-news")
}

pub fn err_cannot_delete_admin_news(locale: &str) -> String {
    t(locale, "err-cannot-delete-admin-news")
}

pub fn err_file_path_too_long(locale: &str, max_length: usize) -> String {
    t_args(
        locale,
        "err-file-path-too-long",
        &[("max_length", &max_length.to_string())],
    )
}

pub fn err_file_path_invalid(locale: &str) -> String {
    t(locale, "err-file-path-invalid")
}

pub fn err_file_not_found(locale: &str) -> String {
    t(locale, "err-file-not-found")
}

pub fn err_file_not_directory(locale: &str) -> String {
    t(locale, "err-file-not-directory")
}

pub fn err_dir_name_empty(locale: &str) -> String {
    t(locale, "err-dir-name-empty")
}

pub fn err_dir_name_too_long(locale: &str, max_length: usize) -> String {
    t_args(
        locale,
        "err-dir-name-too-long",
        &[("max_length", &max_length.to_string())],
    )
}

pub fn err_dir_name_invalid(locale: &str) -> String {
    t(locale, "err-dir-name-invalid")
}

pub fn err_dir_already_exists(locale: &str) -> String {
    t(locale, "err-dir-already-exists")
}

pub fn err_dir_create_failed(locale: &str) -> String {
    t(locale, "err-dir-create-failed")
}

pub fn err_dir_not_empty(locale: &str) -> String {
    t(locale, "err-dir-not-empty")
}

pub fn err_delete_failed(locale: &str) -> String {
    t(locale, "err-delete-failed")
}

pub fn err_rename_failed(locale: &str) -> String {
    t(locale, "err-rename-failed")
}

pub fn err_rename_target_exists(locale: &str) -> String {
    t(locale, "err-rename-target-exists")
}

pub fn err_move_failed(locale: &str) -> String {
    t(locale, "err-move-failed")
}

pub fn err_copy_failed(locale: &str) -> String {
    t(locale, "err-copy-failed")
}

pub fn err_destination_exists(locale: &str) -> String {
    t(locale, "err-destination-exists")
}

pub fn err_cannot_move_into_itself(locale: &str) -> String {
    t(locale, "err-cannot-move-into-itself")
}

pub fn err_cannot_copy_into_itself(locale: &str) -> String {
    t(locale, "err-cannot-copy-into-itself")
}

pub fn err_destination_not_directory(locale: &str) -> String {
    t(locale, "err-destination-not-directory")
}

/// Fires when a source-side BBS file operation collides with active
/// filesystem activity on the same path or a protected related path.
pub fn err_source_busy(locale: &str) -> String {
    t(locale, "err-source-busy")
}

/// Fires when a destination-side BBS file operation collides with active
/// filesystem activity on the same path or a protected related path.
pub fn err_destination_busy(locale: &str) -> String {
    t(locale, "err-destination-busy")
}

pub fn err_file_area_not_configured(locale: &str) -> String {
    t(locale, "err-file-area-not-configured")
}

pub fn err_file_area_not_accessible(locale: &str) -> String {
    t(locale, "err-file-area-not-accessible")
}

pub fn err_transfer_path_too_long(locale: &str) -> String {
    t(locale, "err-transfer-path-too-long")
}

pub fn err_transfer_path_invalid(locale: &str) -> String {
    t(locale, "err-transfer-path-invalid")
}

pub fn err_transfer_access_denied(locale: &str) -> String {
    t(locale, "err-transfer-access-denied")
}

pub fn err_transfer_read_failed(locale: &str) -> String {
    t(locale, "err-transfer-read-failed")
}

pub fn err_transfer_path_not_found(locale: &str) -> String {
    t(locale, "err-transfer-path-not-found")
}

pub fn err_transfer_file_failed(locale: &str, path: &str, error: &str) -> String {
    t_args(
        locale,
        "err-transfer-file-failed",
        &[("path", path), ("error", error)],
    )
}

pub fn err_upload_destination_not_allowed(locale: &str) -> String {
    t(locale, "err-upload-destination-not-allowed")
}

pub fn err_upload_write_failed(locale: &str) -> String {
    t(locale, "err-upload-write-failed")
}

pub fn err_upload_hash_mismatch(locale: &str) -> String {
    t(locale, "err-upload-hash-mismatch")
}

pub fn err_upload_path_invalid(locale: &str) -> String {
    t(locale, "err-upload-path-invalid")
}

/// Another upload to the same filename is in progress.
pub fn err_upload_conflict(locale: &str) -> String {
    t(locale, "err-upload-conflict")
}

/// A file already exists with different content.
pub fn err_upload_file_exists(locale: &str) -> String {
    t(locale, "err-upload-file-exists")
}

/// An upload must contain at least one file.
pub fn err_upload_empty(locale: &str) -> String {
    t(locale, "err-upload-empty")
}

pub fn err_upload_protocol_error(locale: &str) -> String {
    t(locale, "err-upload-protocol-error")
}

pub fn err_upload_connection_lost(locale: &str) -> String {
    t(locale, "err-upload-connection-lost")
}

/// Rate-limited: includes wait time and violation count.
pub fn err_flood_warning(
    locale: &str,
    wait_seconds: u32,
    violation: u32,
    max_violations: u32,
) -> String {
    t_args(
        locale,
        "err-flood-warning",
        &[
            ("seconds", &wait_seconds.to_string()),
            ("violation", &violation.to_string()),
            ("max_violations", &max_violations.to_string()),
        ],
    )
}

pub fn err_flood_disconnect(locale: &str) -> String {
    t(locale, "err-flood-disconnect")
}

pub fn err_slow_client_disconnect(locale: &str) -> String {
    t(locale, "err-slow-client-disconnect")
}

pub fn err_ban_self(locale: &str) -> String {
    t(locale, "err-ban-self")
}

pub fn err_ban_admin_by_nickname(locale: &str) -> String {
    t(locale, "err-ban-admin-by-nickname")
}

/// Generic message to avoid leaking whether an IP belongs to an admin.
pub fn err_ban_admin_by_ip(locale: &str) -> String {
    t(locale, "err-ban-admin-by-ip")
}

pub fn err_ban_invalid_target(locale: &str) -> String {
    t(locale, "err-ban-invalid-target")
}

pub fn err_target_too_long(locale: &str, max_length: usize) -> String {
    t_args(
        locale,
        "err-target-too-long",
        &[("max_length", &max_length.to_string())],
    )
}

pub fn err_ban_invalid_duration(locale: &str) -> String {
    t(locale, "err-ban-invalid-duration")
}

pub fn err_ban_not_found(locale: &str, target: &str) -> String {
    t_args(locale, "err-ban-not-found", &[("target", target)])
}

pub fn err_reason_too_long(locale: &str, max_length: usize) -> String {
    t_args(
        locale,
        "err-reason-too-long",
        &[("max_length", &max_length.to_string())],
    )
}

/// Reason contains control characters.
pub fn err_reason_invalid(locale: &str) -> String {
    t(locale, "err-reason-invalid")
}

pub fn err_banned_permanent(locale: &str) -> String {
    t(locale, "err-banned-permanent")
}

pub fn err_banned_with_expiry(locale: &str, remaining: &str) -> String {
    t_args(
        locale,
        "err-banned-with-expiry",
        &[("remaining", remaining)],
    )
}

pub fn err_trust_invalid_target(locale: &str) -> String {
    t(locale, "err-trust-invalid-target")
}

pub fn err_trust_invalid_duration(locale: &str) -> String {
    t(locale, "err-trust-invalid-duration")
}

pub fn err_trust_not_found(locale: &str, target: &str) -> String {
    t_args(locale, "err-trust-not-found", &[("target", target)])
}

pub fn err_search_query_empty(locale: &str) -> String {
    t(locale, "err-search-query-empty")
}

pub fn err_search_query_too_short(locale: &str, min_length: usize) -> String {
    t_args(
        locale,
        "err-search-query-too-short",
        &[("min_length", &min_length.to_string())],
    )
}

pub fn err_search_query_too_long(locale: &str, max_length: usize) -> String {
    t_args(
        locale,
        "err-search-query-too-long",
        &[("max_length", &max_length.to_string())],
    )
}

pub fn err_search_query_invalid(locale: &str) -> String {
    t(locale, "err-search-query-invalid")
}

pub fn err_search_failed(locale: &str) -> String {
    t(locale, "err-search-failed")
}

pub fn err_voice_listen_required(locale: &str) -> String {
    t(locale, "err-voice-listen-required")
}

pub fn err_voice_feature_not_enabled(locale: &str) -> String {
    t(locale, "err-voice-feature-not-enabled")
}

pub fn err_voice_already_joined(locale: &str) -> String {
    t(locale, "err-voice-already-joined")
}

pub fn err_voice_not_joined(locale: &str) -> String {
    t(locale, "err-voice-not-joined")
}

pub fn err_voice_not_channel_member(locale: &str, channel: &str) -> String {
    t_args(
        locale,
        "err-voice-not-channel-member",
        &[("channel", channel)],
    )
}

pub fn err_voice_target_not_online(locale: &str, nickname: &str) -> String {
    t_args(
        locale,
        "err-voice-target-not-online",
        &[("nickname", nickname)],
    )
}

pub fn err_voice_target_feature_not_enabled(locale: &str, nickname: &str) -> String {
    t_args(
        locale,
        "err-voice-target-feature-not-enabled",
        &[("nickname", nickname)],
    )
}

pub fn err_voice_invalid_target(locale: &str) -> String {
    t(locale, "err-voice-invalid-target")
}

pub fn err_group_name_empty(locale: &str) -> String {
    t(locale, "err-group-name-empty")
}

pub fn err_group_name_too_long(locale: &str, max_length: usize) -> String {
    t_args(
        locale,
        "err-group-name-too-long",
        &[("max_length", &max_length.to_string())],
    )
}

pub fn err_group_name_invalid(locale: &str) -> String {
    t(locale, "err-group-name-invalid")
}

pub fn err_group_not_found(locale: &str) -> String {
    t(locale, "err-group-not-found")
}

pub fn err_group_already_exists(locale: &str) -> String {
    t(locale, "err-group-already-exists")
}

pub fn err_group_shared_permission(locale: &str) -> String {
    t(locale, "err-group-shared-permission")
}

pub fn err_group_not_empty_delete(locale: &str) -> String {
    t(locale, "err-group-not-empty-delete")
}

pub fn err_group_not_empty_modify(locale: &str) -> String {
    t(locale, "err-group-not-empty-modify")
}

pub fn err_group_no_fields(locale: &str) -> String {
    t(locale, "err-group-no-fields")
}

pub fn err_group_shared_mismatch(locale: &str) -> String {
    t(locale, "err-group-shared-mismatch")
}

pub fn err_tracker_not_found(locale: &str) -> String {
    t(locale, "err-tracker-not-found")
}

/// Fires when `TrackerAcceptFingerprint` runs but the row has no
/// `pending_fingerprint` to promote.
pub fn err_tracker_no_pending_fingerprint(locale: &str) -> String {
    t(locale, "err-tracker-no-pending-fingerprint")
}

/// Control chars / non-printable input (distinct from empty / newline cases).
pub fn err_tracker_name_invalid(locale: &str) -> String {
    t(locale, "err-tracker-name-invalid")
}

pub fn err_tracker_name_empty(locale: &str) -> String {
    t(locale, "err-tracker-name-empty")
}

pub fn err_tracker_name_contains_newlines(locale: &str) -> String {
    t(locale, "err-tracker-name-contains-newlines")
}

pub fn err_tracker_name_too_long(locale: &str, max_length: usize) -> String {
    t_args(
        locale,
        "err-tracker-name-too-long",
        &[("max_length", &max_length.to_string())],
    )
}

/// Zero or out-of-range (the latter is also caught by `u16` deserialization).
pub fn err_tracker_port_invalid(locale: &str) -> String {
    t(locale, "err-tracker-port-invalid")
}

/// Fingerprint not in canonical 95-byte uppercase form.
pub fn err_tracker_fingerprint_invalid(locale: &str) -> String {
    t(locale, "err-tracker-fingerprint-invalid")
}

pub fn err_tracker_password_too_long(locale: &str, max_length: usize) -> String {
    t_args(
        locale,
        "err-tracker-password-too-long",
        &[("max_length", &max_length.to_string())],
    )
}

/// UNIQUE-constraint violation on `(address, port)`.
pub fn err_tracker_endpoint_duplicate(locale: &str) -> String {
    t(locale, "err-tracker-endpoint-duplicate")
}

/// UNIQUE-constraint violation on the `name` column (case-insensitive, COLLATE NOCASE).
pub fn err_tracker_name_duplicate(locale: &str) -> String {
    t(locale, "err-tracker-name-duplicate")
}

/// Row count is already at `nexus_common::framing::MAX_TRACKERS_PER_SERVER`.
pub fn err_tracker_too_many(locale: &str, max: usize) -> String {
    t_args(locale, "err-tracker-too-many", &[("max", &max.to_string())])
}

/// Non-admin attempt to *set* a bandwidth tier above the requester's own
/// resolved weight. Companion to `err_bandwidth_weight_inherit_would_elevate`
/// (the inherit-via-clear path).
pub fn err_bandwidth_weight_delegation(locale: &str) -> String {
    t(locale, "err-bandwidth-weight-delegation")
}

/// Non-admin clears a target's override (`inherit_bandwidth_weight: Some(true)`)
/// but the inherited tier would exceed the requester's. Distinct from
/// `err_bandwidth_weight_delegation`: nothing was *set*, the resolved-via-
/// inheritance value is what elevates.
pub fn err_bandwidth_weight_inherit_would_elevate(locale: &str) -> String {
    t(locale, "err-bandwidth-weight-inherit-would-elevate")
}

pub fn err_bandwidth_weight_zero(locale: &str, min: u16) -> String {
    t_args(
        locale,
        "err-bandwidth-weight-zero",
        &[("min", &min.to_string())],
    )
}

pub fn err_bandwidth_chunk_size_too_small(locale: &str, min: u32) -> String {
    t_args(
        locale,
        "err-bandwidth-chunk-size-too-small",
        &[("min", &min.to_string())],
    )
}

pub fn err_bandwidth_chunk_size_too_large(locale: &str, max: u32) -> String {
    t_args(
        locale,
        "err-bandwidth-chunk-size-too-large",
        &[("max", &max.to_string())],
    )
}
