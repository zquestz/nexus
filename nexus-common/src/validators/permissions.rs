//! Permissions validation
//!
//! Validates permission string lists sent in protocol messages.

use crate::PERMISSIONS_COUNT;

/// Maximum length for each permission string in bytes
pub const MAX_PERMISSION_LENGTH: usize = 32;

/// Validation error for permissions
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PermissionsError {
    /// Too many permissions in the list
    TooMany,
    /// A permission string is empty
    EmptyPermission,
    /// A permission string exceeds maximum length
    PermissionTooLong,
    /// A permission string contains newlines
    ContainsNewlines,
    /// A permission string contains invalid characters
    InvalidCharacters,
}

/// Validate a permissions list
///
/// Checks:
/// - Does not exceed maximum count (16 permissions)
/// - Each permission is not empty
/// - Each permission does not exceed maximum length (32 characters)
/// - No newlines in permission strings
/// - No control characters in permission strings
///
/// Note: This validates the format of permission strings, not whether they
/// are recognized permission names. Use `Permission::parse()` to check if
/// a string is a valid permission name.
///
/// # Errors
///
/// Returns a `PermissionsError` variant describing the validation failure.
pub fn validate_permissions(permissions: &[String]) -> Result<(), PermissionsError> {
    if permissions.len() > PERMISSIONS_COUNT {
        return Err(PermissionsError::TooMany);
    }
    for permission in permissions {
        if permission.is_empty() {
            return Err(PermissionsError::EmptyPermission);
        }
        if permission.len() > MAX_PERMISSION_LENGTH {
            return Err(PermissionsError::PermissionTooLong);
        }
        if permission.contains('\n') || permission.contains('\r') {
            return Err(PermissionsError::ContainsNewlines);
        }
        for ch in permission.chars() {
            if ch.is_control() {
                return Err(PermissionsError::InvalidCharacters);
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_valid_permissions() {
        assert!(validate_permissions(&[]).is_ok());
        assert!(validate_permissions(&["user_list".to_string()]).is_ok());
        assert!(validate_permissions(&["user_list".to_string(), "chat_send".to_string()]).is_ok());
        assert!(validate_permissions(&["a".repeat(MAX_PERMISSION_LENGTH)]).is_ok());
        // At the limit
        let max_permissions: Vec<String> = (0..PERMISSIONS_COUNT)
            .map(|i| format!("perm{}", i))
            .collect();
        assert!(validate_permissions(&max_permissions).is_ok());
    }

    #[test]
    fn test_too_many() {
        let too_many: Vec<String> = (0..PERMISSIONS_COUNT + 1)
            .map(|i| format!("perm{}", i))
            .collect();
        assert_eq!(
            validate_permissions(&too_many),
            Err(PermissionsError::TooMany)
        );
    }

    #[test]
    fn test_empty_permission() {
        assert_eq!(
            validate_permissions(&["".to_string()]),
            Err(PermissionsError::EmptyPermission)
        );
        assert_eq!(
            validate_permissions(&["user_list".to_string(), "".to_string()]),
            Err(PermissionsError::EmptyPermission)
        );
    }

    #[test]
    fn test_permission_too_long() {
        assert_eq!(
            validate_permissions(&["a".repeat(MAX_PERMISSION_LENGTH + 1)]),
            Err(PermissionsError::PermissionTooLong)
        );
    }

    #[test]
    fn test_newlines() {
        assert_eq!(
            validate_permissions(&["user_list\n".to_string()]),
            Err(PermissionsError::ContainsNewlines)
        );
        assert_eq!(
            validate_permissions(&["user_list\r".to_string()]),
            Err(PermissionsError::ContainsNewlines)
        );
        assert_eq!(
            validate_permissions(&["user\nlist".to_string()]),
            Err(PermissionsError::ContainsNewlines)
        );
    }

    #[test]
    fn test_control_characters() {
        assert_eq!(
            validate_permissions(&["user_list\0".to_string()]),
            Err(PermissionsError::InvalidCharacters)
        );
        assert_eq!(
            validate_permissions(&["user_list\t".to_string()]),
            Err(PermissionsError::InvalidCharacters)
        );
    }

    #[test]
    fn test_unknown_permission_passes_format_validation() {
        // Unknown permissions pass format validation - semantic validation
        // is done by Permission::parse() in the server
        assert!(validate_permissions(&["unknown_permission".to_string()]).is_ok());
        assert!(validate_permissions(&["not_a_real_perm".to_string()]).is_ok());
    }
}
