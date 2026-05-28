//! Shared test helpers for the transfers module.

use std::collections::HashSet;

use super::types::AuthenticatedUser;
use crate::db::Permission;

/// Build an `AuthenticatedUser` with the given admin flag and permission set.
pub(crate) fn make_authenticated_user(is_admin: bool, perms: &[Permission]) -> AuthenticatedUser {
    let mut permissions = HashSet::new();
    for p in perms {
        permissions.insert(*p);
    }
    AuthenticatedUser {
        user_id: 1,
        nickname: "tester".to_string(),
        username: "tester".to_string(),
        is_admin,
        is_shared: false,
        permissions,
    }
}
