//! Group database operations
//!
//! CRUD operations for permission groups and their associated permissions.
//! Groups serve as permission templates that can be assigned to users.

use std::collections::HashMap;

use nexus_common::protocol::GroupInfo;
use nexus_common::validators;
use sqlx::SqliteConnection;
use sqlx::sqlite::SqlitePool;

use crate::db::permissions::{Permission, Permissions};
use crate::db::sql;
use crate::db::util::clamp_db_bandwidth_weight;

/// A group record from the database
#[derive(Debug, Clone)]
pub struct GroupRecord {
    pub id: i64,
    pub name: String,
    pub is_shared: bool,
    pub bandwidth_weight: u16,
}

/// Both permission sets are read inside the write tx; a pre-tx
/// snapshot in the handler would race with concurrent group updates.
#[derive(Debug, Clone)]
pub struct UpdateGroupResult {
    pub group: GroupRecord,
    pub previous_permissions: Permissions,
    pub final_permissions: Permissions,
}

/// Scope of group-permission writes performed inside `update_group`.
#[derive(Debug, Clone, Copy)]
pub enum GroupPermissionWriteScope<'a> {
    /// Admin / system: replace the full permission set with the
    /// requested set.
    ReplaceAll,
    /// Writes only touch rows whose permission is in this slice;
    /// unowned rows are left alone so concurrent admin grants survive.
    OwnedSubset(&'a [Permission]),
    /// Request omitted the permissions field — leave rows untouched.
    NoChange,
}

/// Row type for group queries (matches `SQL_SELECT_GROUP_*`).
/// SQLite returns `bandwidth_weight` as `i64`; the validator bounds
/// (1..=65535) guarantee the cast back to `u16` is lossless.
type GroupRow = (i64, String, bool, i64);

impl From<GroupRow> for GroupRecord {
    fn from(row: GroupRow) -> Self {
        Self {
            id: row.0,
            name: row.1,
            is_shared: row.2,
            bandwidth_weight: clamp_db_bandwidth_weight(row.3),
        }
    }
}

/// Database access for group operations
#[derive(Clone)]
pub struct GroupDb {
    pool: SqlitePool,
}

impl GroupDb {
    /// Create a new GroupDb instance
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    // ========================================================================
    // Query Methods
    // ========================================================================

    /// Get all groups ordered by name (case-insensitive)
    pub async fn get_all_groups(&self) -> Result<Vec<GroupRecord>, sqlx::Error> {
        let rows: Vec<GroupRow> = sqlx::query_as(sql::SQL_SELECT_ALL_GROUPS)
            .fetch_all(&self.pool)
            .await?;

        Ok(rows.into_iter().map(GroupRecord::from).collect())
    }

    /// Get a single group by ID
    pub async fn get_group_by_id(&self, id: i64) -> Result<Option<GroupRecord>, sqlx::Error> {
        let mut conn = self.pool.acquire().await?;
        Self::get_group_by_id_in_tx(&mut conn, id).await
    }

    pub async fn get_group_by_id_in_tx(
        conn: &mut SqliteConnection,
        id: i64,
    ) -> Result<Option<GroupRecord>, sqlx::Error> {
        let row: Option<GroupRow> = sqlx::query_as(sql::SQL_SELECT_GROUP_BY_ID)
            .bind(id)
            .fetch_optional(&mut *conn)
            .await?;

        Ok(row.map(GroupRecord::from))
    }

    /// Get a single group by name (case-insensitive)
    pub async fn get_group_by_name(&self, name: &str) -> Result<Option<GroupRecord>, sqlx::Error> {
        let row: Option<GroupRow> = sqlx::query_as(sql::SQL_SELECT_GROUP_BY_NAME)
            .bind(name)
            .fetch_optional(&self.pool)
            .await?;

        Ok(row.map(GroupRecord::from))
    }

    /// Get all permissions for a group
    ///
    /// Returns permissions sorted alphabetically.
    pub async fn get_group_permissions(
        &self,
        group_id: i64,
    ) -> Result<Vec<Permission>, sqlx::Error> {
        let rows: Vec<(String,)> = sqlx::query_as(sql::SQL_SELECT_GROUP_PERMISSIONS)
            .bind(group_id)
            .fetch_all(&self.pool)
            .await?;

        Ok(rows
            .into_iter()
            .filter_map(|(p,)| Permission::parse(&p))
            .collect())
    }

    /// Count the number of users assigned to a group
    pub async fn get_member_count(&self, group_id: i64) -> Result<u32, sqlx::Error> {
        let (count,): (i64,) = sqlx::query_as(sql::SQL_COUNT_GROUP_MEMBERS)
            .bind(group_id)
            .fetch_one(&self.pool)
            .await?;

        Ok(count as u32)
    }

    /// Get all groups with member counts and permissions in bulk (3 queries total)
    ///
    /// More efficient than calling `get_all_groups()` + `get_member_count()` +
    /// `get_group_permissions()` per group (which is 1 + 2N queries).
    pub async fn get_all_groups_with_details(&self) -> Result<Vec<GroupInfo>, sqlx::Error> {
        // 1. Fetch all groups
        let groups = self.get_all_groups().await?;

        // 2. Fetch all member counts in one query
        let count_rows: Vec<(i64, i64)> = sqlx::query_as(sql::SQL_COUNT_ALL_GROUP_MEMBERS)
            .fetch_all(&self.pool)
            .await?;
        let member_counts: HashMap<i64, u32> = count_rows
            .into_iter()
            .map(|(gid, count)| (gid, count as u32))
            .collect();

        // 3. Fetch all permissions in one query
        let perm_rows: Vec<(i64, String)> = sqlx::query_as(sql::SQL_SELECT_ALL_GROUP_PERMISSIONS)
            .fetch_all(&self.pool)
            .await?;
        let mut group_perms: HashMap<i64, Vec<String>> = HashMap::new();
        for (gid, perm_str) in perm_rows {
            if Permission::parse(&perm_str).is_some() {
                group_perms.entry(gid).or_default().push(perm_str);
            }
        }

        // Assemble GroupInfo list
        Ok(groups
            .into_iter()
            .map(|g| GroupInfo {
                member_count: member_counts.get(&g.id).copied().unwrap_or(0),
                permissions: group_perms.remove(&g.id).unwrap_or_default(),
                id: g.id,
                name: g.name,
                is_shared: g.is_shared,
                bandwidth_weight: g.bandwidth_weight,
            })
            .collect())
    }

    // ========================================================================
    // Mutation Methods
    // ========================================================================

    /// See [`GroupPermissionWriteScope`].
    async fn set_permissions_in_tx(
        tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
        group_id: i64,
        requested: &Permissions,
        scope: GroupPermissionWriteScope<'_>,
    ) -> Result<(), sqlx::Error> {
        match scope {
            GroupPermissionWriteScope::ReplaceAll => {
                sqlx::query(sql::SQL_DELETE_GROUP_PERMISSIONS)
                    .bind(group_id)
                    .execute(&mut **tx)
                    .await?;

                for perm in requested.iter() {
                    sqlx::query(sql::SQL_INSERT_GROUP_PERMISSION)
                        .bind(group_id)
                        .bind(perm.as_str())
                        .execute(&mut **tx)
                        .await?;
                }
            }
            GroupPermissionWriteScope::OwnedSubset(owned) => {
                for perm in owned {
                    if requested.permissions.contains(perm) {
                        sqlx::query(sql::SQL_INSERT_GROUP_PERMISSION_OR_IGNORE)
                            .bind(group_id)
                            .bind(perm.as_str())
                            .execute(&mut **tx)
                            .await?;
                    } else {
                        sqlx::query(sql::SQL_DELETE_GROUP_PERMISSION_BY_NAME)
                            .bind(group_id)
                            .bind(perm.as_str())
                            .execute(&mut **tx)
                            .await?;
                    }
                }
            }
            GroupPermissionWriteScope::NoChange => {}
        }

        Ok(())
    }

    async fn get_permissions_in_tx(
        tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
        group_id: i64,
    ) -> Result<Permissions, sqlx::Error> {
        let rows: Vec<(String,)> = sqlx::query_as(sql::SQL_SELECT_GROUP_PERMISSIONS)
            .bind(group_id)
            .fetch_all(&mut **tx)
            .await?;
        let mut perms = Permissions::new();
        for (s,) in rows {
            if let Some(p) = Permission::parse(&s) {
                perms.permissions.insert(p);
            }
        }
        Ok(perms)
    }

    /// Create a new group with permissions
    ///
    /// Returns the created group record.
    ///
    /// # Errors
    ///
    /// Returns `sqlx::Error::Database` if a group with the same name already
    /// exists (case-insensitive unique constraint violation).
    pub async fn create_group(
        &self,
        name: &str,
        is_shared: bool,
        permissions: &Permissions,
        bandwidth_weight: u16,
    ) -> Result<GroupRecord, sqlx::Error> {
        // Validate group name (failsafe - handlers should also validate)
        if let Err(e) = validators::validate_group_name(name) {
            return Err(sqlx::Error::Protocol(format!("{:?}", e)));
        }

        // Defense-in-depth: bandwidth weight is u16 (max 65535) but the
        // validator rejects 0.
        if let Err(e) = validators::validate_bandwidth_weight(bandwidth_weight) {
            return Err(sqlx::Error::Protocol(format!("{:?}", e)));
        }

        let mut tx = self.pool.begin().await?;

        let result = sqlx::query(sql::SQL_INSERT_GROUP)
            .bind(name)
            .bind(is_shared)
            .bind(i64::from(bandwidth_weight))
            .execute(&mut *tx)
            .await?;

        let group_id = result.last_insert_rowid();

        Self::set_permissions_in_tx(
            &mut tx,
            group_id,
            permissions,
            GroupPermissionWriteScope::ReplaceAll,
        )
        .await?;

        tx.commit().await?;

        Ok(GroupRecord {
            id: group_id,
            name: name.to_string(),
            is_shared,
            bandwidth_weight,
        })
    }

    /// Returns `None` if the group doesn't exist.
    ///
    /// # Errors
    ///
    /// `sqlx::Error::Database` on name collision (unique-constraint).
    pub async fn update_group(
        &self,
        id: i64,
        name: &str,
        is_shared: bool,
        requested_permissions: &Permissions,
        bandwidth_weight: u16,
        permission_write_scope: GroupPermissionWriteScope<'_>,
    ) -> Result<Option<UpdateGroupResult>, sqlx::Error> {
        // Validate group name (failsafe - handlers should also validate)
        if let Err(e) = validators::validate_group_name(name) {
            return Err(sqlx::Error::Protocol(format!("{:?}", e)));
        }

        // Defense-in-depth: bandwidth weight is u16 (max 65535) but the
        // validator rejects 0.
        if let Err(e) = validators::validate_bandwidth_weight(bandwidth_weight) {
            return Err(sqlx::Error::Protocol(format!("{:?}", e)));
        }

        let mut tx = self.pool.begin().await?;

        let result = sqlx::query(sql::SQL_UPDATE_GROUP)
            .bind(name)
            .bind(is_shared)
            .bind(i64::from(bandwidth_weight))
            .bind(id)
            .bind(is_shared) // Duplicate: atomic shared-toggle check (is_shared = ?)
            .bind(id) // Duplicate: member-count subquery (group_id = ?)
            .execute(&mut *tx)
            .await?;

        if result.rows_affected() == 0 {
            tx.rollback().await?;
            return Ok(None);
        }

        // Read previous + final from inside the tx so the handler's
        // cascade diffs against committed state, not a pre-tx
        // snapshot that could race with a concurrent group update.
        let previous_permissions = Self::get_permissions_in_tx(&mut tx, id).await?;

        Self::set_permissions_in_tx(&mut tx, id, requested_permissions, permission_write_scope)
            .await?;

        let final_permissions = Self::get_permissions_in_tx(&mut tx, id).await?;

        tx.commit().await?;

        Ok(Some(UpdateGroupResult {
            group: GroupRecord {
                id,
                name: name.to_string(),
                is_shared,
                bandwidth_weight,
            },
            previous_permissions,
            final_permissions,
        }))
    }

    /// Atomically delete a group by ID, only if it has no assigned members
    ///
    /// Returns `true` if the group was deleted, `false` if it doesn't exist
    /// or has members. The caller should pre-check member count to provide
    /// the appropriate user-facing error message; this atomic SQL ensures
    /// no TOCTOU race between the check and the delete.
    pub async fn delete_group(&self, id: i64) -> Result<bool, sqlx::Error> {
        let result = sqlx::query(sql::SQL_DELETE_GROUP)
            .bind(id)
            .bind(id)
            .execute(&self.pool)
            .await?;

        Ok(result.rows_affected() > 0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::permissions::{Permission, Permissions};
    use crate::db::testing::create_test_db;

    // ========================================================================
    // Create
    // ========================================================================

    #[tokio::test]
    async fn test_create_group() {
        let pool = create_test_db().await;
        let group_db = GroupDb::new(pool);

        let group = group_db
            .create_group(
                "Moderators",
                false,
                &Permissions::from(&[Permission::ChatSend, Permission::UserKick]),
                1,
            )
            .await
            .unwrap();

        assert_eq!(group.name, "Moderators");
        assert!(!group.is_shared);
        assert!(group.id > 0);
    }

    #[tokio::test]
    async fn test_create_shared_group() {
        let pool = create_test_db().await;
        let group_db = GroupDb::new(pool);

        let group = group_db
            .create_group(
                "SharedUsers",
                true,
                &Permissions::from(&[Permission::ChatSend, Permission::ChatReceive]),
                1,
            )
            .await
            .unwrap();

        assert_eq!(group.name, "SharedUsers");
        assert!(group.is_shared);
    }

    #[tokio::test]
    async fn test_create_group_with_no_permissions() {
        let pool = create_test_db().await;
        let group_db = GroupDb::new(pool);

        let group = group_db
            .create_group("Empty", false, &Permissions::new(), 1)
            .await
            .unwrap();

        let perms = group_db.get_group_permissions(group.id).await.unwrap();
        assert!(perms.is_empty());
    }

    #[tokio::test]
    async fn test_create_group_duplicate_name_rejected() {
        let pool = create_test_db().await;
        let group_db = GroupDb::new(pool);

        group_db
            .create_group("Admins", false, &Permissions::new(), 1)
            .await
            .unwrap();

        // Same name (exact case) should fail
        let result = group_db
            .create_group("Admins", false, &Permissions::new(), 1)
            .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_create_group_duplicate_name_case_insensitive() {
        let pool = create_test_db().await;
        let group_db = GroupDb::new(pool);

        group_db
            .create_group("Admins", false, &Permissions::new(), 1)
            .await
            .unwrap();

        // Different case should also fail (case-insensitive unique index)
        let result = group_db
            .create_group("admins", false, &Permissions::new(), 1)
            .await;
        assert!(result.is_err());

        let result = group_db
            .create_group("ADMINS", false, &Permissions::new(), 1)
            .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_create_group_invalid_name_rejected() {
        let pool = create_test_db().await;
        let group_db = GroupDb::new(pool);

        // Empty name
        let result = group_db
            .create_group("", false, &Permissions::new(), 1)
            .await;
        assert!(result.is_err());

        // Name with forbidden characters
        let result = group_db
            .create_group("bad/name", false, &Permissions::new(), 1)
            .await;
        assert!(result.is_err());
    }

    // ========================================================================
    // Read
    // ========================================================================

    #[tokio::test]
    async fn test_get_group_by_id() {
        let pool = create_test_db().await;
        let group_db = GroupDb::new(pool);

        let created = group_db
            .create_group("TestGroup", false, &Permissions::new(), 1)
            .await
            .unwrap();

        let fetched = group_db.get_group_by_id(created.id).await.unwrap();
        assert!(fetched.is_some());
        let fetched = fetched.unwrap();
        assert_eq!(fetched.id, created.id);
        assert_eq!(fetched.name, "TestGroup");
        assert!(!fetched.is_shared);
    }

    #[tokio::test]
    async fn test_get_group_by_id_not_found() {
        let pool = create_test_db().await;
        let group_db = GroupDb::new(pool);

        let result = group_db.get_group_by_id(99999).await.unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_get_all_groups_empty() {
        let pool = create_test_db().await;
        let group_db = GroupDb::new(pool);

        let groups = group_db.get_all_groups().await.unwrap();
        assert!(groups.is_empty());
    }

    #[tokio::test]
    async fn test_get_all_groups_ordered() {
        let pool = create_test_db().await;
        let group_db = GroupDb::new(pool);

        group_db
            .create_group("Zebra", false, &Permissions::new(), 1)
            .await
            .unwrap();
        group_db
            .create_group("alpha", false, &Permissions::new(), 1)
            .await
            .unwrap();
        group_db
            .create_group("Mods", true, &Permissions::new(), 1)
            .await
            .unwrap();

        let groups = group_db.get_all_groups().await.unwrap();
        assert_eq!(groups.len(), 3);
        // Case-insensitive alphabetical order
        assert_eq!(groups[0].name, "alpha");
        assert_eq!(groups[1].name, "Mods");
        assert_eq!(groups[2].name, "Zebra");
    }

    #[tokio::test]
    async fn test_get_group_permissions() {
        let pool = create_test_db().await;
        let group_db = GroupDb::new(pool);

        let group = group_db
            .create_group(
                "Mods",
                false,
                &Permissions::from(&[
                    Permission::UserKick,
                    Permission::ChatSend,
                    Permission::BanCreate,
                ]),
                1,
            )
            .await
            .unwrap();

        let perms = group_db.get_group_permissions(group.id).await.unwrap();
        // Sorted alphabetically by the SQL query
        assert_eq!(
            perms,
            vec![
                Permission::BanCreate,
                Permission::ChatSend,
                Permission::UserKick
            ]
        );
    }

    #[tokio::test]
    async fn test_get_group_permissions_nonexistent() {
        let pool = create_test_db().await;
        let group_db = GroupDb::new(pool);

        let perms = group_db.get_group_permissions(99999).await.unwrap();
        assert!(perms.is_empty());
    }

    #[tokio::test]
    async fn test_get_member_count_no_members() {
        let pool = create_test_db().await;
        let group_db = GroupDb::new(pool);

        let group = group_db
            .create_group("Empty", false, &Permissions::new(), 1)
            .await
            .unwrap();

        let count = group_db.get_member_count(group.id).await.unwrap();
        assert_eq!(count, 0);
    }

    #[tokio::test]
    async fn test_get_member_count_with_members() {
        let pool = create_test_db().await;
        let group_db = GroupDb::new(pool.clone());
        let user_db = crate::db::UserDb::new(pool.clone());

        let group = group_db
            .create_group("Team", false, &Permissions::new(), 1)
            .await
            .unwrap();

        // Create users and assign them to the group
        let user1 = user_db
            .create_user(crate::db::CreateUserParams {
                username: "alice",
                hashed_password: "hash",
                is_admin: false,
                is_shared: false,
                enabled: true,
                permissions: &crate::db::Permissions::new(),
                group_id: None,
                revokes: &[],
                bandwidth_weight: None,
            })
            .await
            .unwrap();
        let user2 = user_db
            .create_user(crate::db::CreateUserParams {
                username: "bob",
                hashed_password: "hash",
                is_admin: false,
                is_shared: false,
                enabled: true,
                permissions: &crate::db::Permissions::new(),
                group_id: None,
                revokes: &[],
                bandwidth_weight: None,
            })
            .await
            .unwrap();

        // Assign users to group via direct SQL (update_user doesn't have group_id yet)
        sqlx::query("UPDATE users SET group_id = ? WHERE id = ?")
            .bind(group.id)
            .bind(user1.id)
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("UPDATE users SET group_id = ? WHERE id = ?")
            .bind(group.id)
            .bind(user2.id)
            .execute(&pool)
            .await
            .unwrap();

        let count = group_db.get_member_count(group.id).await.unwrap();
        assert_eq!(count, 2);
    }

    // ========================================================================
    // Update
    // ========================================================================

    #[tokio::test]
    async fn test_update_group_name() {
        let pool = create_test_db().await;
        let group_db = GroupDb::new(pool);

        let group = group_db
            .create_group(
                "OldName",
                false,
                &Permissions::from(&[Permission::ChatSend]),
                1,
            )
            .await
            .unwrap();

        let updated = group_db
            .update_group(
                group.id,
                "NewName",
                false,
                &Permissions::from(&[Permission::ChatSend]),
                1,
                GroupPermissionWriteScope::ReplaceAll,
            )
            .await
            .unwrap()
            .unwrap();

        assert_eq!(updated.group.name, "NewName");
        assert!(!updated.group.is_shared);
    }

    #[tokio::test]
    async fn test_update_group_shared_status() {
        let pool = create_test_db().await;
        let group_db = GroupDb::new(pool);

        let group = group_db
            .create_group("Flex", false, &Permissions::new(), 1)
            .await
            .unwrap();
        assert!(!group.is_shared);

        let updated = group_db
            .update_group(
                group.id,
                "Flex",
                true,
                &Permissions::new(),
                1,
                GroupPermissionWriteScope::ReplaceAll,
            )
            .await
            .unwrap()
            .unwrap();

        assert!(updated.group.is_shared);
    }

    #[tokio::test]
    async fn test_update_group_permissions() {
        let pool = create_test_db().await;
        let group_db = GroupDb::new(pool);

        let group = group_db
            .create_group(
                "Mods",
                false,
                &Permissions::from(&[Permission::ChatSend]),
                1,
            )
            .await
            .unwrap();

        let perms_before = group_db.get_group_permissions(group.id).await.unwrap();
        assert_eq!(perms_before, vec![Permission::ChatSend]);

        // Replace permissions
        group_db
            .update_group(
                group.id,
                "Mods",
                false,
                &Permissions::from(&[Permission::UserKick, Permission::BanCreate]),
                1,
                GroupPermissionWriteScope::ReplaceAll,
            )
            .await
            .unwrap();

        let perms_after = group_db.get_group_permissions(group.id).await.unwrap();
        assert_eq!(
            perms_after,
            vec![Permission::BanCreate, Permission::UserKick]
        );
    }

    /// `OwnedSubset` must only touch rows whose permission is in the
    /// owned slice. Unowned rows survive — this is what closes the
    /// non-admin merge-then-replace race.
    #[tokio::test]
    async fn test_update_group_owned_subset_preserves_unowned() {
        let pool = create_test_db().await;
        let group_db = GroupDb::new(pool);

        // Group starts with one owned perm + one unowned perm.
        let group = group_db
            .create_group(
                "Mixed",
                false,
                &Permissions::from(&[Permission::ChatSend, Permission::UserKick]),
                1,
            )
            .await
            .unwrap();

        // Non-admin requester owns ChatSend (not UserKick) and asks
        // for [ChatSend, BanCreate]. Result must be:
        //   - UserKick stays (unowned, untouched)
        //   - ChatSend stays (owned, in requested set)
        //   - BanCreate added (owned, in requested set)
        let owned = [Permission::ChatSend, Permission::BanCreate];
        let requested = Permissions::from(&[Permission::ChatSend, Permission::BanCreate]);
        let result = group_db
            .update_group(
                group.id,
                "Mixed",
                false,
                &requested,
                1,
                GroupPermissionWriteScope::OwnedSubset(&owned),
            )
            .await
            .unwrap()
            .unwrap();

        let mut final_perms: Vec<Permission> = result.final_permissions.iter().copied().collect();
        final_perms.sort_by_key(|p| p.as_str().to_string());
        assert_eq!(
            final_perms,
            vec![
                Permission::BanCreate,
                Permission::ChatSend,
                Permission::UserKick,
            ],
            "UserKick (unowned) must survive; ChatSend kept; BanCreate added"
        );

        // Requesting an empty set with ChatSend owned should remove
        // ChatSend but keep UserKick.
        let owned = [Permission::ChatSend];
        let requested = Permissions::new();
        let result = group_db
            .update_group(
                group.id,
                "Mixed",
                false,
                &requested,
                1,
                GroupPermissionWriteScope::OwnedSubset(&owned),
            )
            .await
            .unwrap()
            .unwrap();

        // previous_permissions is read inside the tx and reflects the
        // committed state right before the write — i.e. the result of
        // the first update_group call above.
        let mut prev_perms: Vec<Permission> = result.previous_permissions.iter().copied().collect();
        prev_perms.sort_by_key(|p| p.as_str().to_string());
        assert_eq!(
            prev_perms,
            vec![
                Permission::BanCreate,
                Permission::ChatSend,
                Permission::UserKick,
            ],
            "previous_permissions reflects committed state before the second write"
        );

        let mut final_perms: Vec<Permission> = result.final_permissions.iter().copied().collect();
        final_perms.sort_by_key(|p| p.as_str().to_string());
        assert_eq!(
            final_perms,
            vec![Permission::BanCreate, Permission::UserKick],
            "ChatSend (owned, omitted) removed; BanCreate + UserKick (unowned) survive"
        );
    }

    /// `NoChange` leaves the permission set untouched even if a
    /// non-empty `requested` is supplied (the variant wins).
    #[tokio::test]
    async fn test_update_group_no_change_leaves_permissions_alone() {
        let pool = create_test_db().await;
        let group_db = GroupDb::new(pool);

        let group = group_db
            .create_group(
                "Stable",
                false,
                &Permissions::from(&[Permission::ChatSend, Permission::UserKick]),
                1,
            )
            .await
            .unwrap();

        // A non-empty requested set is ignored under NoChange.
        let bogus_requested = Permissions::from(&[Permission::BanCreate]);
        let result = group_db
            .update_group(
                group.id,
                "Stable",
                false,
                &bogus_requested,
                1,
                GroupPermissionWriteScope::NoChange,
            )
            .await
            .unwrap()
            .unwrap();

        let mut prev_perms: Vec<Permission> = result.previous_permissions.iter().copied().collect();
        prev_perms.sort_by_key(|p| p.as_str().to_string());
        assert_eq!(
            prev_perms,
            vec![Permission::ChatSend, Permission::UserKick],
            "previous_permissions reflects pre-write state"
        );

        let mut final_perms: Vec<Permission> = result.final_permissions.iter().copied().collect();
        final_perms.sort_by_key(|p| p.as_str().to_string());
        assert_eq!(
            final_perms,
            vec![Permission::ChatSend, Permission::UserKick],
            "NoChange must leave the original set intact"
        );
    }

    #[tokio::test]
    async fn test_update_group_clear_permissions() {
        let pool = create_test_db().await;
        let group_db = GroupDb::new(pool);

        let group = group_db
            .create_group(
                "Mods",
                false,
                &Permissions::from(&[Permission::ChatSend, Permission::UserKick]),
                1,
            )
            .await
            .unwrap();

        group_db
            .update_group(
                group.id,
                "Mods",
                false,
                &Permissions::new(),
                1,
                GroupPermissionWriteScope::ReplaceAll,
            )
            .await
            .unwrap();

        let perms = group_db.get_group_permissions(group.id).await.unwrap();
        assert!(perms.is_empty());
    }

    #[tokio::test]
    async fn test_update_group_not_found() {
        let pool = create_test_db().await;
        let group_db = GroupDb::new(pool);

        let result = group_db
            .update_group(
                99999,
                "Ghost",
                false,
                &Permissions::new(),
                1,
                GroupPermissionWriteScope::ReplaceAll,
            )
            .await
            .unwrap();

        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_update_group_duplicate_name_rejected() {
        let pool = create_test_db().await;
        let group_db = GroupDb::new(pool);

        group_db
            .create_group("GroupA", false, &Permissions::new(), 1)
            .await
            .unwrap();
        let group_b = group_db
            .create_group("GroupB", false, &Permissions::new(), 1)
            .await
            .unwrap();

        // Try to rename GroupB to GroupA
        let result = group_db
            .update_group(
                group_b.id,
                "GroupA",
                false,
                &Permissions::new(),
                1,
                GroupPermissionWriteScope::ReplaceAll,
            )
            .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_update_group_duplicate_name_case_insensitive() {
        let pool = create_test_db().await;
        let group_db = GroupDb::new(pool);

        group_db
            .create_group("GroupA", false, &Permissions::new(), 1)
            .await
            .unwrap();
        let group_b = group_db
            .create_group("GroupB", false, &Permissions::new(), 1)
            .await
            .unwrap();

        // Try to rename GroupB to "groupa" (case-insensitive conflict)
        let result = group_db
            .update_group(
                group_b.id,
                "groupa",
                false,
                &Permissions::new(),
                1,
                GroupPermissionWriteScope::ReplaceAll,
            )
            .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_update_group_same_name_preserves_case() {
        let pool = create_test_db().await;
        let group_db = GroupDb::new(pool);

        let group = group_db
            .create_group("MyGroup", false, &Permissions::new(), 1)
            .await
            .unwrap();

        // Updating with the same name (same case) should succeed
        let updated = group_db
            .update_group(
                group.id,
                "MyGroup",
                false,
                &Permissions::from(&[Permission::ChatSend]),
                1,
                GroupPermissionWriteScope::ReplaceAll,
            )
            .await
            .unwrap()
            .unwrap();

        assert_eq!(updated.group.name, "MyGroup");
    }

    #[tokio::test]
    async fn test_update_group_rename_own_case() {
        let pool = create_test_db().await;
        let group_db = GroupDb::new(pool);

        let group = group_db
            .create_group("mygroup", false, &Permissions::new(), 1)
            .await
            .unwrap();

        // Changing case of own name should succeed (same row, no conflict)
        let updated = group_db
            .update_group(
                group.id,
                "MyGroup",
                false,
                &Permissions::new(),
                1,
                GroupPermissionWriteScope::ReplaceAll,
            )
            .await
            .unwrap()
            .unwrap();

        assert_eq!(updated.group.name, "MyGroup");
    }

    #[tokio::test]
    async fn test_update_group_invalid_name_rejected() {
        let pool = create_test_db().await;
        let group_db = GroupDb::new(pool);

        let group = group_db
            .create_group("Valid", false, &Permissions::new(), 1)
            .await
            .unwrap();

        // Empty name
        let result = group_db
            .update_group(
                group.id,
                "",
                false,
                &Permissions::new(),
                1,
                GroupPermissionWriteScope::ReplaceAll,
            )
            .await;
        assert!(result.is_err());

        // Forbidden characters
        let result = group_db
            .update_group(
                group.id,
                "bad/name",
                false,
                &Permissions::new(),
                1,
                GroupPermissionWriteScope::ReplaceAll,
            )
            .await;
        assert!(result.is_err());
    }

    // ========================================================================
    // Delete
    // ========================================================================

    #[tokio::test]
    async fn test_delete_group() {
        let pool = create_test_db().await;
        let group_db = GroupDb::new(pool);

        let group = group_db
            .create_group(
                "ToDelete",
                false,
                &Permissions::from(&[Permission::ChatSend]),
                1,
            )
            .await
            .unwrap();

        let deleted = group_db.delete_group(group.id).await.unwrap();
        assert!(deleted);

        // Verify it's gone
        let fetched = group_db.get_group_by_id(group.id).await.unwrap();
        assert!(fetched.is_none());

        // Permissions should be cascade deleted
        let perms = group_db.get_group_permissions(group.id).await.unwrap();
        assert!(perms.is_empty());
    }

    #[tokio::test]
    async fn test_delete_group_not_found() {
        let pool = create_test_db().await;
        let group_db = GroupDb::new(pool);

        let deleted = group_db.delete_group(99999).await.unwrap();
        assert!(!deleted);
    }

    #[tokio::test]
    async fn test_delete_group_already_deleted() {
        let pool = create_test_db().await;
        let group_db = GroupDb::new(pool);

        let group = group_db
            .create_group("Once", false, &Permissions::new(), 1)
            .await
            .unwrap();

        let first = group_db.delete_group(group.id).await.unwrap();
        assert!(first);

        let second = group_db.delete_group(group.id).await.unwrap();
        assert!(!second);
    }

    #[tokio::test]
    async fn test_delete_group_with_members_rejected() {
        let pool = create_test_db().await;
        let group_db = GroupDb::new(pool.clone());
        let user_db = crate::db::UserDb::new(pool.clone());

        let group = group_db
            .create_group("Busy", false, &Permissions::new(), 1)
            .await
            .unwrap();

        // Create a user and assign to group
        let user = user_db
            .create_user(crate::db::CreateUserParams {
                username: "alice",
                hashed_password: "hash",
                is_admin: false,
                is_shared: false,
                enabled: true,
                permissions: &crate::db::Permissions::new(),
                group_id: None,
                revokes: &[],
                bandwidth_weight: None,
            })
            .await
            .unwrap();

        sqlx::query("UPDATE users SET group_id = ? WHERE id = ?")
            .bind(group.id)
            .bind(user.id)
            .execute(&pool)
            .await
            .unwrap();

        // Delete should return false (atomic SQL prevents delete when members exist)
        let result = group_db.delete_group(group.id).await.unwrap();
        assert!(
            !result,
            "delete_group should return false when group has members"
        );

        // Group should still exist
        let fetched = group_db.get_group_by_id(group.id).await.unwrap();
        assert!(fetched.is_some());
    }

    // ========================================================================
    // Interaction with users
    // ========================================================================

    #[tokio::test]
    async fn test_delete_group_after_unassigning_members() {
        let pool = create_test_db().await;
        let group_db = GroupDb::new(pool.clone());
        let user_db = crate::db::UserDb::new(pool.clone());

        let group = group_db
            .create_group("Temp", false, &Permissions::new(), 1)
            .await
            .unwrap();

        let user = user_db
            .create_user(crate::db::CreateUserParams {
                username: "alice",
                hashed_password: "hash",
                is_admin: false,
                is_shared: false,
                enabled: true,
                permissions: &crate::db::Permissions::new(),
                group_id: None,
                revokes: &[],
                bandwidth_weight: None,
            })
            .await
            .unwrap();

        // Assign then unassign
        sqlx::query("UPDATE users SET group_id = ? WHERE id = ?")
            .bind(group.id)
            .bind(user.id)
            .execute(&pool)
            .await
            .unwrap();

        sqlx::query("UPDATE users SET group_id = NULL WHERE id = ?")
            .bind(user.id)
            .execute(&pool)
            .await
            .unwrap();

        // Now delete should succeed
        let deleted = group_db.delete_group(group.id).await.unwrap();
        assert!(deleted);
    }

    #[tokio::test]
    async fn test_multiple_groups_independent() {
        let pool = create_test_db().await;
        let group_db = GroupDb::new(pool);

        let group_a = group_db
            .create_group(
                "GroupA",
                false,
                &Permissions::from(&[Permission::ChatSend]),
                1,
            )
            .await
            .unwrap();
        let group_b = group_db
            .create_group(
                "GroupB",
                true,
                &Permissions::from(&[Permission::FileDownload, Permission::FileList]),
                1,
            )
            .await
            .unwrap();

        let perms_a = group_db.get_group_permissions(group_a.id).await.unwrap();
        let perms_b = group_db.get_group_permissions(group_b.id).await.unwrap();

        assert_eq!(perms_a, vec![Permission::ChatSend]);
        assert_eq!(
            perms_b,
            vec![Permission::FileDownload, Permission::FileList]
        );

        // Deleting A doesn't affect B
        group_db.delete_group(group_a.id).await.unwrap();

        let perms_b_after = group_db.get_group_permissions(group_b.id).await.unwrap();
        assert_eq!(
            perms_b_after,
            vec![Permission::FileDownload, Permission::FileList]
        );
    }

    // ========================================================================
    // Bandwidth Weight Tests
    // ========================================================================

    #[tokio::test]
    async fn test_create_group_with_custom_bandwidth_weight() {
        let pool = create_test_db().await;
        let group_db = GroupDb::new(pool);

        let group = group_db
            .create_group("VIP", false, &Permissions::new(), 100)
            .await
            .unwrap();
        assert_eq!(group.bandwidth_weight, 100);

        let fetched = group_db.get_group_by_id(group.id).await.unwrap().unwrap();
        assert_eq!(fetched.bandwidth_weight, 100);
    }

    #[tokio::test]
    async fn test_update_group_changes_bandwidth_weight() {
        let pool = create_test_db().await;
        let group_db = GroupDb::new(pool);

        let group = group_db
            .create_group("Mods", false, &Permissions::new(), 1)
            .await
            .unwrap();

        let updated = group_db
            .update_group(
                group.id,
                "Mods",
                false,
                &Permissions::new(),
                42,
                GroupPermissionWriteScope::ReplaceAll,
            )
            .await
            .unwrap();
        assert!(updated.is_some());
        assert_eq!(updated.unwrap().group.bandwidth_weight, 42);

        let fetched = group_db.get_group_by_id(group.id).await.unwrap().unwrap();
        assert_eq!(fetched.bandwidth_weight, 42);
    }

    #[tokio::test]
    async fn test_get_all_groups_with_details_includes_bandwidth_weight() {
        let pool = create_test_db().await;
        let group_db = GroupDb::new(pool);

        group_db
            .create_group("Alpha", false, &Permissions::new(), 5)
            .await
            .unwrap();
        group_db
            .create_group("Beta", false, &Permissions::new(), 50)
            .await
            .unwrap();

        let groups = group_db.get_all_groups_with_details().await.unwrap();
        // Sorted alphabetically by `get_all_groups`.
        assert_eq!(groups.len(), 2);
        assert_eq!(groups[0].name, "Alpha");
        assert_eq!(groups[0].bandwidth_weight, 5);
        assert_eq!(groups[1].name, "Beta");
        assert_eq!(groups[1].bandwidth_weight, 50);
    }

    /// Source-of-truth contract: the `group` field of `UpdateGroupResult`
    /// reflects what was actually written, including the new weight.
    /// Pins the invariant the production handler depends on (line
    /// `updated_group.bandwidth_weight` drives the inheriting-member
    /// cache write).
    #[tokio::test]
    async fn test_update_group_returns_group_record_matching_written_state() {
        let pool = create_test_db().await;
        let group_db = GroupDb::new(pool);

        let group = group_db
            .create_group("Tier1", false, &Permissions::new(), 10)
            .await
            .unwrap();

        let result = group_db
            .update_group(
                group.id,
                "Renamed",
                true,
                &Permissions::new(),
                42,
                GroupPermissionWriteScope::ReplaceAll,
            )
            .await
            .unwrap()
            .unwrap();

        assert_eq!(result.group.id, group.id);
        assert_eq!(result.group.name, "Renamed");
        assert!(result.group.is_shared);
        assert_eq!(result.group.bandwidth_weight, 42);
    }

    /// Pins the contract that `DEFAULT_BANDWIDTH_WEIGHT` is the value
    /// the system default actually flows through as. Most tests use
    /// literal `1` for fixture concreteness (fixed values, not
    /// "the current default"); this one anchors the constant so a
    /// future bump of the default surfaces as a test failure here
    /// rather than as silent divergence between fixtures and
    /// production state.
    #[tokio::test]
    async fn test_create_group_with_default_bandwidth_weight() {
        use nexus_common::validators::DEFAULT_BANDWIDTH_WEIGHT;

        let pool = create_test_db().await;
        let group_db = GroupDb::new(pool);

        let group = group_db
            .create_group(
                "Default",
                false,
                &Permissions::new(),
                DEFAULT_BANDWIDTH_WEIGHT,
            )
            .await
            .unwrap();

        assert_eq!(group.bandwidth_weight, DEFAULT_BANDWIDTH_WEIGHT);

        let fetched = group_db.get_group_by_id(group.id).await.unwrap().unwrap();
        assert_eq!(fetched.bandwidth_weight, DEFAULT_BANDWIDTH_WEIGHT);
    }
}
