//! Shared validation helpers for group handlers.

use nexus_common::is_shared_account_permission;
use nexus_common::validators::{
    self, GroupNameError, PermissionsError, validate_group_name, validate_permissions,
};

use super::{
    err_group_name_empty, err_group_name_invalid, err_group_name_too_long,
    err_group_shared_permission, err_permissions_contains_newlines,
    err_permissions_empty_permission, err_permissions_invalid_characters,
    err_permissions_permission_too_long, err_permissions_too_many, err_unknown_permission,
};
use crate::db::Permission;

pub(super) fn validate_group_name_for_handler(name: &str, locale: &str) -> Result<(), String> {
    validate_group_name(name).map_err(|e| match e {
        GroupNameError::Empty => err_group_name_empty(locale),
        GroupNameError::TooLong => {
            err_group_name_too_long(locale, validators::MAX_GROUP_NAME_LENGTH)
        }
        GroupNameError::InvalidCharacters => err_group_name_invalid(locale),
    })
}

pub(super) fn validate_group_permissions_for_handler(
    permissions: &[String],
    locale: &str,
) -> Result<(), String> {
    validate_permissions(permissions).map_err(|e| match e {
        PermissionsError::TooMany => {
            err_permissions_too_many(locale, nexus_common::PERMISSIONS_COUNT)
        }
        PermissionsError::EmptyPermission => err_permissions_empty_permission(locale),
        PermissionsError::PermissionTooLong => {
            err_permissions_permission_too_long(locale, validators::MAX_PERMISSION_LENGTH)
        }
        PermissionsError::ContainsNewlines => err_permissions_contains_newlines(locale),
        PermissionsError::InvalidCharacters => err_permissions_invalid_characters(locale),
    })
}

pub(super) fn parse_group_permissions_for_handler(
    permissions: &[String],
    locale: &str,
) -> Result<Vec<Permission>, String> {
    permissions
        .iter()
        .map(|perm| parse_group_permission_for_handler(perm, locale))
        .collect()
}

pub(super) fn parse_group_permission_for_handler(
    permission: &str,
    locale: &str,
) -> Result<Permission, String> {
    Permission::parse(permission).ok_or_else(|| err_unknown_permission(locale, permission))
}

pub(super) fn validate_shared_group_permission_names(
    permissions: &[String],
    locale: &str,
) -> Result<(), String> {
    if permissions
        .iter()
        .any(|perm| !is_shared_account_permission(perm))
    {
        return Err(err_group_shared_permission(locale));
    }
    Ok(())
}

pub(super) fn validate_shared_group_permissions(
    permissions: &[Permission],
    locale: &str,
) -> Result<(), String> {
    if permissions
        .iter()
        .any(|perm| !is_shared_account_permission(perm.as_str()))
    {
        return Err(err_group_shared_permission(locale));
    }
    Ok(())
}
