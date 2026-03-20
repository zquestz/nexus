//! Group database operations
//!
//! CRUD operations for permission groups and their associated permissions.
//! Groups serve as permission templates that can be assigned to users.

use nexus_common::validators;
use sqlx::sqlite::SqlitePool;

use crate::db::sql;

/// A group record from the database
#[derive(Debug, Clone)]
pub struct GroupRecord {
    pub id: i64,
    pub name: String,
    pub is_shared: bool,
}

/// Row type for group queries
type GroupRow = (i64, String, bool);

impl From<GroupRow> for GroupRecord {
    fn from(row: GroupRow) -> Self {
        Self {
            id: row.0,
            name: row.1,
            is_shared: row.2,
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
        let row: Option<GroupRow> = sqlx::query_as(sql::SQL_SELECT_GROUP_BY_ID)
            .bind(id)
            .fetch_optional(&self.pool)
            .await?;

        Ok(row.map(GroupRecord::from))
    }

    /// Get all permissions for a group
    ///
    /// Returns permission strings sorted alphabetically.
    pub async fn get_group_permissions(&self, group_id: i64) -> Result<Vec<String>, sqlx::Error> {
        let rows: Vec<(String,)> = sqlx::query_as(sql::SQL_SELECT_GROUP_PERMISSIONS)
            .bind(group_id)
            .fetch_all(&self.pool)
            .await?;

        Ok(rows.into_iter().map(|(p,)| p).collect())
    }

    /// Count the number of users assigned to a group
    pub async fn get_member_count(&self, group_id: i64) -> Result<u32, sqlx::Error> {
        let (count,): (i64,) = sqlx::query_as(sql::SQL_COUNT_GROUP_MEMBERS)
            .bind(group_id)
            .fetch_one(&self.pool)
            .await?;

        Ok(count as u32)
    }

    // ========================================================================
    // Mutation Methods
    // ========================================================================

    /// Set permissions for a group within an existing transaction
    ///
    /// Deletes all existing permissions and inserts the new ones.
    async fn set_permissions_in_tx(
        tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
        group_id: i64,
        permissions: &[String],
    ) -> Result<(), sqlx::Error> {
        sqlx::query(sql::SQL_DELETE_GROUP_PERMISSIONS)
            .bind(group_id)
            .execute(&mut **tx)
            .await?;

        for perm in permissions {
            sqlx::query(sql::SQL_INSERT_GROUP_PERMISSION)
                .bind(group_id)
                .bind(perm.as_str())
                .execute(&mut **tx)
                .await?;
        }

        Ok(())
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
        permissions: &[String],
    ) -> Result<GroupRecord, sqlx::Error> {
        // Validate group name (failsafe - handlers should also validate)
        if let Err(e) = validators::validate_group_name(name) {
            return Err(sqlx::Error::Protocol(format!("{:?}", e)));
        }

        let mut tx = self.pool.begin().await?;

        let result = sqlx::query(sql::SQL_INSERT_GROUP)
            .bind(name)
            .bind(is_shared)
            .execute(&mut *tx)
            .await?;

        let group_id = result.last_insert_rowid();

        Self::set_permissions_in_tx(&mut tx, group_id, permissions).await?;

        tx.commit().await?;

        Ok(GroupRecord {
            id: group_id,
            name: name.to_string(),
            is_shared,
        })
    }

    /// Update a group's name, shared status, and permissions
    ///
    /// Returns the updated group record, or `None` if the group doesn't exist.
    ///
    /// # Errors
    ///
    /// Returns `sqlx::Error::Database` if the new name conflicts with an
    /// existing group (case-insensitive unique constraint violation).
    pub async fn update_group(
        &self,
        id: i64,
        name: &str,
        is_shared: bool,
        permissions: &[String],
    ) -> Result<Option<GroupRecord>, sqlx::Error> {
        // Validate group name (failsafe - handlers should also validate)
        if let Err(e) = validators::validate_group_name(name) {
            return Err(sqlx::Error::Protocol(format!("{:?}", e)));
        }

        let mut tx = self.pool.begin().await?;

        let result = sqlx::query(sql::SQL_UPDATE_GROUP)
            .bind(name)
            .bind(is_shared)
            .bind(id)
            .execute(&mut *tx)
            .await?;

        if result.rows_affected() == 0 {
            tx.rollback().await?;
            return Ok(None);
        }

        Self::set_permissions_in_tx(&mut tx, id, permissions).await?;

        tx.commit().await?;

        Ok(Some(GroupRecord {
            id,
            name: name.to_string(),
            is_shared,
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
                &["chat_send".into(), "user_kick".into()],
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
                &["chat_send".into(), "chat_receive".into()],
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

        let group = group_db.create_group("Empty", false, &[]).await.unwrap();

        let perms = group_db.get_group_permissions(group.id).await.unwrap();
        assert!(perms.is_empty());
    }

    #[tokio::test]
    async fn test_create_group_duplicate_name_rejected() {
        let pool = create_test_db().await;
        let group_db = GroupDb::new(pool);

        group_db.create_group("Admins", false, &[]).await.unwrap();

        // Same name (exact case) should fail
        let result = group_db.create_group("Admins", false, &[]).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_create_group_duplicate_name_case_insensitive() {
        let pool = create_test_db().await;
        let group_db = GroupDb::new(pool);

        group_db.create_group("Admins", false, &[]).await.unwrap();

        // Different case should also fail (case-insensitive unique index)
        let result = group_db.create_group("admins", false, &[]).await;
        assert!(result.is_err());

        let result = group_db.create_group("ADMINS", false, &[]).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_create_group_invalid_name_rejected() {
        let pool = create_test_db().await;
        let group_db = GroupDb::new(pool);

        // Empty name
        let result = group_db.create_group("", false, &[]).await;
        assert!(result.is_err());

        // Name with forbidden characters
        let result = group_db.create_group("bad/name", false, &[]).await;
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
            .create_group("TestGroup", false, &[])
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

        group_db.create_group("Zebra", false, &[]).await.unwrap();
        group_db.create_group("alpha", false, &[]).await.unwrap();
        group_db.create_group("Mods", true, &[]).await.unwrap();

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
                &["user_kick".into(), "chat_send".into(), "ban_create".into()],
            )
            .await
            .unwrap();

        let perms = group_db.get_group_permissions(group.id).await.unwrap();
        // Sorted alphabetically by the SQL query
        assert_eq!(perms, vec!["ban_create", "chat_send", "user_kick"]);
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

        let group = group_db.create_group("Empty", false, &[]).await.unwrap();

        let count = group_db.get_member_count(group.id).await.unwrap();
        assert_eq!(count, 0);
    }

    #[tokio::test]
    async fn test_get_member_count_with_members() {
        let pool = create_test_db().await;
        let group_db = GroupDb::new(pool.clone());
        let user_db = crate::db::UserDb::new(pool.clone());

        let group = group_db.create_group("Team", false, &[]).await.unwrap();

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
            .create_group("OldName", false, &["chat_send".into()])
            .await
            .unwrap();

        let updated = group_db
            .update_group(group.id, "NewName", false, &["chat_send".into()])
            .await
            .unwrap()
            .unwrap();

        assert_eq!(updated.name, "NewName");
        assert!(!updated.is_shared);
    }

    #[tokio::test]
    async fn test_update_group_shared_status() {
        let pool = create_test_db().await;
        let group_db = GroupDb::new(pool);

        let group = group_db.create_group("Flex", false, &[]).await.unwrap();
        assert!(!group.is_shared);

        let updated = group_db
            .update_group(group.id, "Flex", true, &[])
            .await
            .unwrap()
            .unwrap();

        assert!(updated.is_shared);
    }

    #[tokio::test]
    async fn test_update_group_permissions() {
        let pool = create_test_db().await;
        let group_db = GroupDb::new(pool);

        let group = group_db
            .create_group("Mods", false, &["chat_send".into()])
            .await
            .unwrap();

        let perms_before = group_db.get_group_permissions(group.id).await.unwrap();
        assert_eq!(perms_before, vec!["chat_send"]);

        // Replace permissions
        group_db
            .update_group(
                group.id,
                "Mods",
                false,
                &["user_kick".into(), "ban_create".into()],
            )
            .await
            .unwrap();

        let perms_after = group_db.get_group_permissions(group.id).await.unwrap();
        assert_eq!(perms_after, vec!["ban_create", "user_kick"]);
    }

    #[tokio::test]
    async fn test_update_group_clear_permissions() {
        let pool = create_test_db().await;
        let group_db = GroupDb::new(pool);

        let group = group_db
            .create_group("Mods", false, &["chat_send".into(), "user_kick".into()])
            .await
            .unwrap();

        group_db
            .update_group(group.id, "Mods", false, &[])
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
            .update_group(99999, "Ghost", false, &[])
            .await
            .unwrap();

        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_update_group_duplicate_name_rejected() {
        let pool = create_test_db().await;
        let group_db = GroupDb::new(pool);

        group_db.create_group("GroupA", false, &[]).await.unwrap();
        let group_b = group_db.create_group("GroupB", false, &[]).await.unwrap();

        // Try to rename GroupB to GroupA
        let result = group_db
            .update_group(group_b.id, "GroupA", false, &[])
            .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_update_group_duplicate_name_case_insensitive() {
        let pool = create_test_db().await;
        let group_db = GroupDb::new(pool);

        group_db.create_group("GroupA", false, &[]).await.unwrap();
        let group_b = group_db.create_group("GroupB", false, &[]).await.unwrap();

        // Try to rename GroupB to "groupa" (case-insensitive conflict)
        let result = group_db
            .update_group(group_b.id, "groupa", false, &[])
            .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_update_group_same_name_preserves_case() {
        let pool = create_test_db().await;
        let group_db = GroupDb::new(pool);

        let group = group_db.create_group("MyGroup", false, &[]).await.unwrap();

        // Updating with the same name (same case) should succeed
        let updated = group_db
            .update_group(group.id, "MyGroup", false, &["chat_send".into()])
            .await
            .unwrap()
            .unwrap();

        assert_eq!(updated.name, "MyGroup");
    }

    #[tokio::test]
    async fn test_update_group_rename_own_case() {
        let pool = create_test_db().await;
        let group_db = GroupDb::new(pool);

        let group = group_db.create_group("mygroup", false, &[]).await.unwrap();

        // Changing case of own name should succeed (same row, no conflict)
        let updated = group_db
            .update_group(group.id, "MyGroup", false, &[])
            .await
            .unwrap()
            .unwrap();

        assert_eq!(updated.name, "MyGroup");
    }

    #[tokio::test]
    async fn test_update_group_invalid_name_rejected() {
        let pool = create_test_db().await;
        let group_db = GroupDb::new(pool);

        let group = group_db.create_group("Valid", false, &[]).await.unwrap();

        // Empty name
        let result = group_db.update_group(group.id, "", false, &[]).await;
        assert!(result.is_err());

        // Forbidden characters
        let result = group_db
            .update_group(group.id, "bad/name", false, &[])
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
            .create_group("ToDelete", false, &["chat_send".into()])
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

        let group = group_db.create_group("Once", false, &[]).await.unwrap();

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

        let group = group_db.create_group("Busy", false, &[]).await.unwrap();

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

        let group = group_db.create_group("Temp", false, &[]).await.unwrap();

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
            .create_group("GroupA", false, &["chat_send".into()])
            .await
            .unwrap();
        let group_b = group_db
            .create_group(
                "GroupB",
                true,
                &["file_download".into(), "file_list".into()],
            )
            .await
            .unwrap();

        let perms_a = group_db.get_group_permissions(group_a.id).await.unwrap();
        let perms_b = group_db.get_group_permissions(group_b.id).await.unwrap();

        assert_eq!(perms_a, vec!["chat_send"]);
        assert_eq!(perms_b, vec!["file_download", "file_list"]);

        // Deleting A doesn't affect B
        group_db.delete_group(group_a.id).await.unwrap();

        let perms_b_after = group_db.get_group_permissions(group_b.id).await.unwrap();
        assert_eq!(perms_b_after, vec!["file_download", "file_list"]);
    }
}
