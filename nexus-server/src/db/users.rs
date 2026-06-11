//! User account database operations

use nexus_common::names::fold_name;
use nexus_common::validators;
use nexus_common::validators::{DEFAULT_BANDWIDTH_WEIGHT, resolve_bandwidth_weight};
use sqlx::{SqliteConnection, SqlitePool};

use super::permissions::{Permission, Permissions};
use super::util::{clamp_db_bandwidth_weight, is_unique_violation};
use crate::db::sql;

pub struct CreateUserParams<'a> {
    pub username: &'a str,
    pub hashed_password: &'a str,
    pub is_admin: bool,
    pub is_shared: bool,
    pub enabled: bool,
    pub permissions: &'a Permissions,
    pub group_id: Option<i64>,
    pub revokes: &'a [Permission],
    /// `None` stores NULL (resolver falls back to group or system default).
    pub bandwidth_weight: Option<u16>,
}

/// All fields except `username` are optional — only provided fields are changed.
///
/// Group change: `remove_group` takes precedence over `group_id`.
/// - `remove_group: true` — remove from group
/// - `group_id: Some(id)` — assign to group
/// - Both false/None — no group change
///
/// Bandwidth weight change: `inherit_bandwidth_weight` takes precedence over `bandwidth_weight`.
/// - `inherit_bandwidth_weight: true` — store NULL (inherit from group)
/// - `bandwidth_weight: Some(w)` — store override
/// - Both false/None — no change
pub struct UpdateUserParams<'a> {
    pub username: &'a str,
    pub new_username: Option<&'a str>,
    pub new_password_hash: Option<&'a str>,
    pub is_admin: Option<bool>,
    pub enabled: Option<bool>,
    pub permissions: Option<&'a Permissions>,
    pub revokes: Option<&'a [Permission]>,
    pub remove_group: bool,
    pub group_id: Option<i64>,
    pub bandwidth_weight: Option<u16>,
    pub inherit_bandwidth_weight: bool,
    /// Threaded into the atomic SQL guard so a non-admin's in-flight
    /// edit can't land on a row promoted to admin in the interim.
    /// Internal / system callers pass `true` to skip the guard.
    pub requester_is_admin: bool,
    /// See [`PermissionWriteScope`].
    pub permission_write_scope: PermissionWriteScope<'a>,
    /// Bandwidth ceiling for the in-tx group-auth check. `None` means
    /// no ceiling (admin / system).
    ///
    /// API invariant: `requester_is_admin == false` requires both
    /// `Some(_)` here AND `OwnedSubset(_)` in
    /// `permission_write_scope`. Violating either returns
    /// `Err(sqlx::Error::Protocol(_))` from `update_user`.
    pub requester_bandwidth_max: Option<u16>,
}

/// Scope of grant / revoke writes performed inside `update_user`.
/// Explicit variants (rather than `Option<&[Permission]>` with
/// `None`-means-replace) so the surgical path is opt-in by intent.
#[derive(Debug, Clone, Copy)]
pub enum PermissionWriteScope<'a> {
    /// Admin / system: replace the full grant + revoke set.
    ReplaceAll,
    /// Non-admin bounded by their own perms — writes only touch rows
    /// whose permission is in this slice, so concurrent admin writes
    /// to unowned permissions can't be clobbered. Upserts flip
    /// `override_type` when the requester asks for the opposite kind.
    OwnedSubset(&'a [Permission]),
}

/// Outcome of `update_user`.
///
/// Distinguishes a successful update — which carries the user's new
/// effective `bandwidth_weight` resolved inside the same transaction as
/// the write — from a constraint-block (last-admin protection, duplicate
/// username, user not found). Both arms are `Ok(_)`; only true DB errors
/// produce `Err(sqlx::Error)`.
///
/// Computing the resolved weight inside the same transaction as the
/// write is the key invariant: no secondary read can fail after the
/// write succeeded, so callers never observe a "succeeded-but-bandwidth-
/// unknown" torn state.
#[derive(Debug, Clone)]
pub enum UpdateUserResult {
    /// Update succeeded.
    ///
    /// `account` is the post-update `UserAccount` assembled from the
    /// in-scope values the UPDATE just wrote — same shape as
    /// `update_group`'s `UpdateGroupResult.group`. Callers consume this
    /// instead of re-fetching the row via `get_user_by_username`,
    /// removing both an extra DB round-trip and the
    /// "what-we-asked-to-write vs what-we-actually-wrote" divergence
    /// where the handler computes its own `final_username` /
    /// `final_is_admin`.
    ///
    /// `resolved_bandwidth_weight` is the user's post-update effective
    /// weight (override → admin default → group → system default),
    /// resolved inside the same transaction. Suitable for writing
    /// straight to session caches.
    ///
    /// `permissions` is the post-update effective set (group ∪
    /// grants − revokes; empty for admins, who bypass at
    /// `UserSession::has_permission`). Resolved in-tx so callers
    /// flip the auth cache atomically without a follow-up read.
    Updated {
        account: UserAccount,
        resolved_bandwidth_weight: u16,
        permissions: Permissions,
    },
    /// Update was blocked by an atomic constraint or the user was not
    /// found. The caller distinguishes the specific reason with a
    /// follow-up SELECT (existing pattern).
    Blocked,
    /// In-tx group-auth re-validation refused the write — an admin
    /// altered the target group between the handler's pre-check and
    /// this tx. Distinct from `Blocked` so a same-request username
    /// change isn't mis-attributed to duplicate / last-admin.
    BlockedForGroupAuth,
}

/// User account stored in database.
#[derive(Debug, Clone)]
pub struct UserAccount {
    pub id: i64,
    pub username: String,
    pub hashed_password: String,
    pub is_admin: bool,
    pub is_shared: bool,
    pub enabled: bool,
    pub created_at: i64,
    pub group_id: Option<i64>,
    /// Raw stored weight (`None` = inherit from group or system default).
    pub bandwidth_weight: Option<u16>,
}

/// Row type for user queries (matches `SQL_SELECT_USER_BY_*`).
/// SQLite returns the nullable `bandwidth_weight` column as `Option<i64>`; the
/// validator bounds (1..=65535) guarantee the cast back to `u16` is lossless.
type UserRow = (
    i64,
    String,
    String,
    bool,
    bool,
    bool,
    i64,
    Option<i64>,
    Option<i64>,
);

impl From<UserRow> for UserAccount {
    fn from(row: UserRow) -> Self {
        Self {
            id: row.0,
            username: row.1,
            hashed_password: row.2,
            is_admin: row.3,
            is_shared: row.4,
            enabled: row.5,
            created_at: row.6,
            group_id: row.7,
            bandwidth_weight: row.8.map(clamp_db_bandwidth_weight),
        }
    }
}

/// Database operations for user accounts.
///
/// # Atomic Protection
///
/// Several methods use atomic SQL operations to prevent race conditions:
/// - `delete_user()` - Prevents deleting the last admin
/// - `update_user()` - Prevents disabling last enabled admin or demoting last admin
/// - `create_first_user_if_none_exist()` - Prevents multiple first users
#[derive(Clone)]
pub struct UserDb {
    pool: SqlitePool,
}

impl UserDb {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    pub async fn get_user_by_id(&self, user_id: i64) -> Result<Option<UserAccount>, sqlx::Error> {
        let mut conn = self.pool.acquire().await?;
        Self::get_user_by_id_in_tx(&mut conn, user_id).await
    }

    pub async fn get_user_by_id_in_tx(
        conn: &mut SqliteConnection,
        user_id: i64,
    ) -> Result<Option<UserAccount>, sqlx::Error> {
        let row: Option<UserRow> = sqlx::query_as(sql::SQL_SELECT_USER_BY_ID)
            .bind(user_id)
            .fetch_optional(&mut *conn)
            .await?;

        Ok(row.map(UserAccount::from))
    }

    /// Case-insensitive lookup via the folded `username_lower` key (`fold_name`).
    pub async fn get_user_by_username(
        &self,
        username: &str,
    ) -> Result<Option<UserAccount>, sqlx::Error> {
        // Failsafe validation — handlers should also validate.
        if let Err(e) = validators::validate_username(username) {
            return Err(sqlx::Error::Protocol(format!("{:?}", e)));
        }

        let row: Option<UserRow> = sqlx::query_as(sql::SQL_SELECT_USER_BY_USERNAME)
            .bind(fold_name(username))
            .fetch_optional(&self.pool)
            .await?;

        Ok(row.map(UserAccount::from))
    }

    /// Case-insensitive existence check via the folded `username_lower` key.
    pub async fn username_exists(&self, username: &str) -> Result<bool, sqlx::Error> {
        // Failsafe validation — handlers should also validate.
        if let Err(e) = validators::validate_username(username) {
            return Err(sqlx::Error::Protocol(format!("{:?}", e)));
        }

        let (count,): (i64,) = sqlx::query_as(sql::SQL_CHECK_USERNAME_EXISTS)
            .bind(fold_name(username))
            .fetch_one(&self.pool)
            .await?;
        Ok(count > 0)
    }

    /// Whether the guest account is currently enabled.
    ///
    /// Used by the tracker task to populate
    /// `TrackerServerRegister.allows_guest`. Returns `false` if the
    /// guest account row is missing (catastrophic; bootstrap migration
    /// ensures it exists).
    pub async fn guest_enabled(&self) -> Result<bool, sqlx::Error> {
        let row: Option<(bool,)> = sqlx::query_as(sql::SQL_GET_GUEST_ENABLED)
            .fetch_optional(&self.pool)
            .await?;
        Ok(row.map(|(enabled,)| enabled).unwrap_or(false))
    }

    /// All users, sorted alphabetically by username.
    pub async fn get_all_users(&self) -> Result<Vec<UserAccount>, sqlx::Error> {
        let rows: Vec<UserRow> = sqlx::query_as(sql::SQL_SELECT_ALL_USERS)
            .fetch_all(&self.pool)
            .await?;

        Ok(rows.into_iter().map(UserAccount::from).collect())
    }

    /// Resolve a user's effective bandwidth weight
    ///
    /// Resolution:
    /// 1. `user.bandwidth_weight` — explicit per-user override always wins.
    /// 2. If the user is admin → `DEFAULT_ADMIN_BANDWIDTH_WEIGHT` (admins
    ///    don't belong to groups, so this slots in where group resolution
    ///    would otherwise apply).
    /// 3. `group.bandwidth_weight` — inherited from the user's group.
    /// 4. `DEFAULT_BANDWIDTH_WEIGHT` — server-wide fallback.
    ///
    /// Returns `DEFAULT_BANDWIDTH_WEIGHT` for unknown users (intended for
    /// scheduler hot path — caller does not need to handle "user gone" as an
    /// error case).
    pub async fn get_resolved_bandwidth_weight_in_tx(
        conn: &mut SqliteConnection,
        user_id: i64,
    ) -> Result<u16, sqlx::Error> {
        let row: Option<(Option<i64>, Option<i64>, bool)> =
            sqlx::query_as(sql::SQL_SELECT_USER_AND_GROUP_BANDWIDTH_WEIGHT)
                .bind(user_id)
                .fetch_optional(&mut *conn)
                .await?;

        let Some((user_weight, group_weight, is_admin)) = row else {
            return Ok(DEFAULT_BANDWIDTH_WEIGHT);
        };

        Ok(resolve_bandwidth_weight(
            user_weight.map(clamp_db_bandwidth_weight),
            group_weight.map(clamp_db_bandwidth_weight),
            is_admin,
        ))
    }

    /// Resolve the user's effective bandwidth weight *as if their per-user
    /// override were cleared* AND their group were `proposed_group_id`.
    /// Used by the delegation check for `inherit_bandwidth_weight:
    /// Some(true)` — we need to know what the resolver will return AFTER
    /// the override is removed, accounting for any pending group change in
    /// the same request.
    ///
    /// `proposed_group_id`:
    /// - `Some(gid)` → user will be in group `gid` after the update
    /// - `None` → user will have no group after the update
    ///
    /// When the request leaves the group unchanged, the caller passes the
    /// user's current group_id (so the join uses the same value the
    /// post-update resolver would see).
    ///
    /// Resolution (mirrors `get_resolved_bandwidth_weight` with the user
    /// override stripped and the group substituted):
    /// 1. If the user is admin → `DEFAULT_ADMIN_BANDWIDTH_WEIGHT`.
    /// 2. If `proposed_group_id` is `Some(gid)` and the group exists →
    ///    that group's `bandwidth_weight`.
    /// 3. Otherwise → `DEFAULT_BANDWIDTH_WEIGHT`.
    ///
    /// Returns `DEFAULT_BANDWIDTH_WEIGHT` for unknown users.
    pub async fn get_inherited_bandwidth_weight(
        &self,
        user_id: i64,
        proposed_group_id: Option<i64>,
    ) -> Result<u16, sqlx::Error> {
        let row: Option<(bool, Option<i64>)> =
            sqlx::query_as(sql::SQL_SELECT_USER_ADMIN_AND_PROPOSED_GROUP_WEIGHT)
                .bind(proposed_group_id)
                .bind(user_id)
                .fetch_optional(&self.pool)
                .await?;

        let Some((is_admin, group_weight)) = row else {
            return Ok(DEFAULT_BANDWIDTH_WEIGHT);
        };

        // `None` for the override: this query is for the "what does the
        // resolver return AFTER the override is cleared" delegation check,
        // so we deliberately bypass the user-override branch.
        Ok(resolve_bandwidth_weight(
            None,
            group_weight.map(clamp_db_bandwidth_weight),
            is_admin,
        ))
    }

    pub async fn get_group_bandwidth_inheritor_user_ids(
        &self,
        group_id: i64,
    ) -> Result<Vec<i64>, sqlx::Error> {
        let rows: Vec<(i64,)> = sqlx::query_as(sql::SQL_SELECT_GROUP_BANDWIDTH_INHERITOR_USER_IDS)
            .bind(group_id)
            .fetch_all(&self.pool)
            .await?;
        Ok(rows.into_iter().map(|(user_id,)| user_id).collect())
    }

    /// Get effective permissions for a user, resolving group + overrides
    ///
    /// Resolution logic:
    /// - If user has a group: `(group_permissions ∪ grant_overrides) - revoke_overrides`
    /// - If user has no group: grant overrides only (legacy behavior)
    pub async fn get_user_permissions(&self, user_id: i64) -> Result<Permissions, sqlx::Error> {
        let mut conn = self.pool.acquire().await?;
        Self::get_user_permissions_in_tx(&mut conn, user_id).await
    }

    /// The three internal reads share the connection, so a shared
    /// read tx sees a coherent group+overrides view.
    pub async fn get_user_permissions_in_tx(
        conn: &mut SqliteConnection,
        user_id: i64,
    ) -> Result<Permissions, sqlx::Error> {
        let group_id: Option<(Option<i64>,)> = sqlx::query_as(sql::SQL_SELECT_USER_GROUP_ID)
            .bind(user_id)
            .fetch_optional(&mut *conn)
            .await?;

        let Some((group_id,)) = group_id else {
            return Ok(Permissions::new());
        };

        let perm_rows: Vec<(String, String)> = sqlx::query_as(sql::SQL_SELECT_PERMISSIONS)
            .bind(user_id)
            .fetch_all(&mut *conn)
            .await?;

        let mut permissions = Permissions::new();

        if let Some(gid) = group_id {
            // User has a group — resolve: (group_perms ∪ grants) - revokes
            let group_perm_rows: Vec<(String,)> = sqlx::query_as(sql::SQL_SELECT_GROUP_PERMISSIONS)
                .bind(gid)
                .fetch_all(&mut *conn)
                .await?;

            for (perm_str,) in &group_perm_rows {
                if let Some(perm) = Permission::parse(perm_str) {
                    permissions.permissions.insert(perm);
                }
            }

            for (perm_str, override_type) in &perm_rows {
                if let Some(perm) = Permission::parse(perm_str) {
                    if override_type == "grant" {
                        permissions.permissions.insert(perm);
                    } else if override_type == "revoke" {
                        permissions.permissions.remove(&perm);
                    }
                }
            }
        } else {
            // No group — legacy behavior (grants only)
            for (perm_str, override_type) in &perm_rows {
                if override_type == "grant"
                    && let Some(perm) = Permission::parse(perm_str)
                {
                    permissions.permissions.insert(perm);
                }
            }
        }

        Ok(permissions)
    }

    /// Set permissions within an existing transaction
    ///
    /// Deletes all existing permissions and inserts the new ones as grants.
    /// Optionally inserts revoke overrides for group-based permission denial.
    /// The caller is responsible for committing or rolling back the transaction.
    async fn set_permissions_in_tx(
        tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
        user_id: i64,
        permissions: &Permissions,
        revokes: Option<&[Permission]>,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(sql::SQL_DELETE_PERMISSIONS)
            .bind(user_id)
            .execute(&mut **tx)
            .await?;

        for perm in permissions.iter() {
            sqlx::query(sql::SQL_INSERT_PERMISSION)
                .bind(user_id)
                .bind(perm.as_str())
                .execute(&mut **tx)
                .await?;
        }

        if let Some(revoke_perms) = revokes {
            for perm in revoke_perms {
                sqlx::query(sql::SQL_INSERT_PERMISSION_OVERRIDE)
                    .bind(user_id)
                    .bind(perm.as_str())
                    .bind("revoke")
                    .execute(&mut **tx)
                    .await?;
            }
        }

        Ok(())
    }

    /// Surgical grant overrides for non-admin callers: per `p` in
    /// `owned`, upsert grant if `p ∈ requested_grants` (flipping an
    /// existing revoke), else delete the grant row (revoke untouched).
    /// Rows for permissions outside `owned` are never touched —
    /// concurrent admin writes to unowned perms survive.
    async fn set_grant_overrides_for_owned_in_tx(
        tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
        user_id: i64,
        requested_grants: &Permissions,
        owned: &[Permission],
    ) -> Result<(), sqlx::Error> {
        for owned_perm in owned {
            if requested_grants.permissions.contains(owned_perm) {
                sqlx::query(sql::SQL_UPSERT_GRANT_PERMISSION)
                    .bind(user_id)
                    .bind(owned_perm.as_str())
                    .execute(&mut **tx)
                    .await?;
            } else {
                sqlx::query(sql::SQL_DELETE_GRANT_PERMISSION)
                    .bind(user_id)
                    .bind(owned_perm.as_str())
                    .execute(&mut **tx)
                    .await?;
            }
        }
        Ok(())
    }

    /// Symmetric to [`Self::set_grant_overrides_for_owned_in_tx`] with
    /// grant ↔ revoke swapped.
    async fn set_revoke_overrides_for_owned_in_tx(
        tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
        user_id: i64,
        requested_revokes: &[Permission],
        owned: &[Permission],
    ) -> Result<(), sqlx::Error> {
        for owned_perm in owned {
            if requested_revokes.contains(owned_perm) {
                sqlx::query(sql::SQL_UPSERT_REVOKE_PERMISSION)
                    .bind(user_id)
                    .bind(owned_perm.as_str())
                    .execute(&mut **tx)
                    .await?;
            } else {
                sqlx::query(sql::SQL_DELETE_REVOKE_PERMISSION)
                    .bind(user_id)
                    .bind(owned_perm.as_str())
                    .execute(&mut **tx)
                    .await?;
            }
        }
        Ok(())
    }

    /// Returns `true` when every permission the group currently holds
    /// (read inside `tx`) is in `owned`. Parse failures are treated
    /// conservatively as unowned.
    async fn group_perms_within_owned_in_tx(
        tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
        group_id: i64,
        owned: &[Permission],
    ) -> Result<bool, sqlx::Error> {
        let rows: Vec<(String,)> = sqlx::query_as(sql::SQL_SELECT_GROUP_PERMISSIONS)
            .bind(group_id)
            .fetch_all(&mut **tx)
            .await?;
        let owned_set: std::collections::HashSet<Permission> = owned.iter().copied().collect();
        for (perm_str,) in rows {
            match Permission::parse(&perm_str) {
                Some(perm) if owned_set.contains(&perm) => continue,
                _ => return Ok(false),
            }
        }
        Ok(true)
    }

    pub async fn create_user(
        &self,
        params: CreateUserParams<'_>,
    ) -> Result<UserAccount, sqlx::Error> {
        // Failsafe validation — handlers should also validate.
        if let Err(e) = validators::validate_username(params.username) {
            return Err(sqlx::Error::Protocol(format!("{:?}", e)));
        }

        // Defense-in-depth: per-user override (when present) must be non-zero.
        // u16 type caps the upper bound; the validator rejects 0.
        if let Some(w) = params.bandwidth_weight
            && let Err(e) = validators::validate_bandwidth_weight(w)
        {
            return Err(sqlx::Error::Protocol(format!("{:?}", e)));
        }

        let created_at = chrono::Utc::now().timestamp();

        // Transaction so user row + permissions land atomically.
        let mut tx = self.pool.begin().await?;

        let result = sqlx::query(sql::SQL_INSERT_USER)
            .bind(params.username)
            .bind(fold_name(params.username))
            .bind(params.hashed_password)
            .bind(params.is_admin)
            .bind(params.is_shared)
            .bind(params.enabled)
            .bind(created_at)
            .bind(params.group_id)
            .bind(params.bandwidth_weight.map(i64::from))
            .execute(&mut *tx)
            .await?;

        let user_id = result.last_insert_rowid();

        // Only set permissions for non-admin users
        // Admins automatically get all permissions via has_permission()
        if !params.is_admin {
            let revokes = if params.revokes.is_empty() {
                None
            } else {
                Some(params.revokes)
            };
            Self::set_permissions_in_tx(&mut tx, user_id, params.permissions, revokes).await?;
        }

        // If assigned to a group, remove duplicate grants (group already provides them)
        if let Some(group_id) = params.group_id {
            let group_perm_rows: Vec<(String,)> = sqlx::query_as(sql::SQL_SELECT_GROUP_PERMISSIONS)
                .bind(group_id)
                .fetch_all(&mut *tx)
                .await?;
            let group_perms: std::collections::HashSet<String> =
                group_perm_rows.into_iter().map(|(p,)| p).collect();

            let grant_rows: Vec<(String,)> = sqlx::query_as(sql::SQL_SELECT_GRANT_PERMISSIONS)
                .bind(user_id)
                .fetch_all(&mut *tx)
                .await?;

            for (perm,) in grant_rows {
                if group_perms.contains(&perm) {
                    sqlx::query(sql::SQL_DELETE_GRANT_PERMISSION)
                        .bind(user_id)
                        .bind(&perm)
                        .execute(&mut *tx)
                        .await?;
                }
            }
        }

        tx.commit().await?;

        Ok(UserAccount {
            id: user_id,
            username: params.username.to_string(),
            hashed_password: params.hashed_password.to_string(),
            is_admin: params.is_admin,
            is_shared: params.is_shared,
            enabled: params.enabled,
            created_at,
            group_id: params.group_id,
            bandwidth_weight: params.bandwidth_weight,
        })
    }

    /// Atomically create the first user as admin if no users exist
    ///
    /// This method uses a transaction to check if any users exist and create
    /// the first user atomically, preventing race conditions where multiple
    /// simultaneous logins could all become admin.
    ///
    /// Returns:
    /// - Ok(Some(account)) - First user created successfully as admin
    /// - Ok(None) - Users already exist, did not create
    /// - Err(e) - Database error
    pub async fn create_first_user_if_none_exist(
        &self,
        username: &str,
        hashed_password: &str,
    ) -> Result<Option<UserAccount>, sqlx::Error> {
        // Failsafe validation — handlers should also validate.
        if let Err(e) = validators::validate_username(username) {
            return Err(sqlx::Error::Protocol(format!("{:?}", e)));
        }

        let mut tx = self.pool.begin().await?;

        // Guest account excluded so the first real user becomes admin.
        let count: (i64,) = sqlx::query_as(sql::SQL_COUNT_NON_GUEST_USERS)
            .fetch_one(&mut *tx)
            .await?;

        if count.0 > 0 {
            tx.rollback().await?;
            return Ok(None);
        }

        let created_at = chrono::Utc::now().timestamp();

        let result = sqlx::query(sql::SQL_INSERT_USER)
            .bind(username)
            .bind(fold_name(username))
            .bind(hashed_password)
            .bind(true) // is_admin = true
            .bind(false) // is_shared = false (first user cannot be shared)
            .bind(true) // enabled = true
            .bind(created_at)
            .bind(None::<i64>) // group_id = None (first user never has a group)
            .bind(None::<i64>) // bandwidth_weight = NULL (inherit / system default)
            .execute(&mut *tx)
            .await?;

        let user_id = result.last_insert_rowid();

        tx.commit().await?;

        Ok(Some(UserAccount {
            id: user_id,
            username: username.to_string(),
            hashed_password: hashed_password.to_string(),
            is_admin: true,
            is_shared: false,
            enabled: true,
            created_at,
            group_id: None,
            bandwidth_weight: None,
        }))
    }

    /// Whether any non-guest user exists.
    ///
    /// Guest is excluded so a fresh install can validate the first real user's
    /// login inputs before attempting the atomic first-admin insert.
    pub async fn has_non_guest_users(&self) -> Result<bool, sqlx::Error> {
        let count: (i64,) = sqlx::query_as(sql::SQL_COUNT_NON_GUEST_USERS)
            .fetch_one(&self.pool)
            .await?;
        Ok(count.0 > 0)
    }

    /// Delete a user. `Ok(true)` on success, `Ok(false)` if the row
    /// is missing OR the atomic guard blocked (last-admin or
    /// non-admin requester targeting an admin row). Internal/system
    /// callers pass `requester_is_admin: true` to skip the
    /// admin-target guard.
    pub async fn delete_user(
        &self,
        user_id: i64,
        requester_is_admin: bool,
    ) -> Result<bool, sqlx::Error> {
        let result = sqlx::query(sql::SQL_DELETE_USER_ATOMIC)
            .bind(user_id)
            .bind(requester_is_admin)
            .execute(&self.pool)
            .await?;

        Ok(result.rows_affected() > 0)
    }

    /// # Atomic Protection
    ///
    /// This method uses atomic SQL to prevent race conditions in two scenarios:
    ///
    /// 1. **Last Admin Demotion**: Prevents demoting the last admin (atomic SQL)
    /// 2. **Last Enabled Admin Disable**: Prevents disabling the last enabled admin (atomic SQL)
    ///
    /// The SQL UPDATE includes WHERE clauses with compound conditions:
    /// ```sql
    /// WHERE id = ?
    /// AND (
    ///     ? = 1                                -- Allow if enabling (final_enabled = true)
    ///     OR is_admin = 0                      -- Allow if target is not admin
    ///     OR (COUNT enabled admins) > 1        -- Allow if multiple enabled admins exist
    /// )
    /// AND (
    ///     ? = 1                                -- Allow if promoting (final_is_admin = true)
    ///     OR is_admin = 0                      -- Allow if target is currently not admin
    ///     OR (COUNT admins) > 1                -- Allow if multiple admins exist
    /// )
    /// ```
    ///
    /// This prevents TOCTOU (Time-Of-Check-To-Time-Of-Use) vulnerabilities where two admins
    /// could simultaneously disable or demote each other, leaving zero enabled/any admins.
    ///
    /// # Return Value
    ///
    /// - `Ok(UpdateUserResult::Updated { resolved_bandwidth_weight })`:
    ///   Update succeeded. The resolved weight is computed inside the
    ///   same transaction as the write, so callers can refresh session
    ///   caches without a follow-up DB read (which could itself fail and
    ///   produce a torn state).
    /// - `Ok(UpdateUserResult::Blocked)`: Update blocked by atomic
    ///   constraint or user not found.
    ///
    /// # Note
    ///
    /// When `Blocked` is returned, the caller must distinguish between:
    /// - User not found
    /// - Last admin demotion attempt
    /// - Last enabled admin disable attempt
    /// - Duplicate username conflict
    pub async fn update_user(
        &self,
        params: UpdateUserParams<'_>,
    ) -> Result<UpdateUserResult, sqlx::Error> {
        let user = match self.get_user_by_username(params.username).await? {
            Some(u) => u,
            None => return Ok(UpdateUserResult::Blocked),
        };

        if let Some(new_name) = params.new_username
            && fold_name(new_name) != fold_name(params.username)
            && self.get_user_by_username(new_name).await?.is_some()
        {
            return Ok(UpdateUserResult::Blocked);
        }

        let final_username = params.new_username.unwrap_or(params.username);

        // Failsafe validation — handlers should also validate.
        if let Some(new_username) = params.new_username
            && let Err(e) = validators::validate_username(new_username)
        {
            return Err(sqlx::Error::Protocol(format!("{:?}", e)));
        }

        // Defense-in-depth: bandwidth_weight override (when present and
        // actually about to be stored) must be non-zero. Skip when
        // `inherit_bandwidth_weight` takes precedence — the value will be
        // discarded by the precedence resolution below, so a client sending
        // {bandwidth_weight: Some(0), inherit_bandwidth_weight: true} must
        // not be rejected as if the zero were going to land in the DB.
        // u16 type caps the upper bound; the validator rejects 0.
        if !params.inherit_bandwidth_weight
            && let Some(w) = params.bandwidth_weight
            && let Err(e) = validators::validate_bandwidth_weight(w)
        {
            return Err(sqlx::Error::Protocol(format!("{:?}", e)));
        }

        let final_password = params.new_password_hash.unwrap_or(&user.hashed_password);
        let final_is_admin = params.is_admin.unwrap_or(user.is_admin);
        let final_enabled = params.enabled.unwrap_or(user.enabled);
        // inherit_bandwidth_weight takes precedence over an explicit value (matches
        // the remove_group ↔ group_id precedence above and in the protocol).
        let final_bandwidth_weight: Option<i64> = if params.inherit_bandwidth_weight {
            None
        } else if let Some(w) = params.bandwidth_weight {
            Some(w.into())
        } else {
            user.bandwidth_weight.map(i64::from)
        };

        // Transaction so the row update + permission writes land atomically.
        let mut tx = self.pool.begin().await?;

        // Promotion auto-clean: when flipping from non-admin to admin, NULL
        // group_id and wipe permission override rows. The schema CHECK
        // forbids admin + group_id together, so without this step
        // SQL_UPDATE_USER below would fail on any user who currently has a
        // group. Permission rows are dead state for admins (resolver
        // short-circuits to all-permissions); clearing them prevents
        // orphan revokes from resurfacing on a future demote.
        //
        // The companion invariant (admin XOR shared) takes the opposite
        // approach — see `handlers/user_update.rs` where promoting a
        // shared account is rejected outright rather than auto-cleaned.
        // The asymmetry is deliberate: clearing a group is benign,
        // demoting `is_shared` would orphan a shared account's
        // per-session nicknames.
        if final_is_admin && !user.is_admin {
            sqlx::query(sql::SQL_UPDATE_USER_GROUP)
                .bind(None::<i64>)
                .bind(user.id)
                .execute(&mut *tx)
                .await?;
            sqlx::query(sql::SQL_DELETE_PERMISSIONS)
                .bind(user.id)
                .execute(&mut *tx)
                .await?;
        }

        // Execute update with atomic last-admin protection
        // The SQL includes conditions to prevent:
        // 1. Disabling the last enabled admin
        // 2. Demoting the last admin
        let result = match sqlx::query(sql::SQL_UPDATE_USER)
            .bind(final_username)
            .bind(fold_name(final_username))
            .bind(final_password)
            .bind(final_is_admin)
            .bind(final_enabled)
            .bind(final_bandwidth_weight)
            .bind(user.id)
            .bind(final_enabled) // Final enabled status for the "enabling" check
            .bind(final_is_admin) // Final admin status for the "promoting" check
            .bind(params.requester_is_admin) // Non-admin cannot edit admin target
            .execute(&mut *tx)
            .await
        {
            Ok(r) => r,
            // `username_lower` UNIQUE collision: a fold-equal username exists.
            // Pre-checked above and serialized under `user_state_lock`, so
            // normally unreachable — map to Blocked (→ err_username_exists),
            // never a generic DB error.
            Err(e) if is_unique_violation(&e) => {
                tx.rollback().await?;
                return Ok(UpdateUserResult::Blocked);
            }
            Err(e) => return Err(e),
        };

        // 0 rows affected: last-admin protection blocked it, or the user
        // is gone. Roll back and re-read to distinguish the cases (the
        // caller surfaces a different error for each).
        if result.rows_affected() == 0 {
            tx.rollback().await?;
            if self.get_user_by_username(params.username).await?.is_some() {
                return Ok(UpdateUserResult::Blocked);
            }
            return Ok(UpdateUserResult::Blocked);
        }

        // Admins resolve to all permissions via `has_permission`'s
        // bypass, so override rows are dead state for them — clear.
        if let Some(perms) = params.permissions {
            if final_is_admin {
                sqlx::query(sql::SQL_DELETE_PERMISSIONS)
                    .bind(user.id)
                    .execute(&mut *tx)
                    .await?;
            } else {
                match params.permission_write_scope {
                    PermissionWriteScope::ReplaceAll => {
                        Self::set_permissions_in_tx(&mut tx, user.id, perms, params.revokes)
                            .await?;
                    }
                    PermissionWriteScope::OwnedSubset(owned) => {
                        Self::set_grant_overrides_for_owned_in_tx(&mut tx, user.id, perms, owned)
                            .await?;
                        // `revokes: None` here still runs revoke
                        // surgery with an empty set, mirroring the
                        // grant side. To leave revokes alone, pass
                        // both `permissions: None` and `revokes: None`.
                        let revokes_for_surgery: &[Permission] = params.revokes.unwrap_or(&[]);
                        Self::set_revoke_overrides_for_owned_in_tx(
                            &mut tx,
                            user.id,
                            revokes_for_surgery,
                            owned,
                        )
                        .await?;
                    }
                }
            }
        } else if let Some(revokes) = params.revokes
            && !final_is_admin
        {
            match params.permission_write_scope {
                PermissionWriteScope::ReplaceAll => {
                    // Upsert (not INSERT) so a revoke request flips an
                    // existing grant row instead of colliding on the PK.
                    sqlx::query(sql::SQL_DELETE_REVOKE_PERMISSIONS)
                        .bind(user.id)
                        .execute(&mut *tx)
                        .await?;
                    for perm in revokes {
                        sqlx::query(sql::SQL_UPSERT_REVOKE_PERMISSION)
                            .bind(user.id)
                            .bind(perm.as_str())
                            .execute(&mut *tx)
                            .await?;
                    }
                }
                PermissionWriteScope::OwnedSubset(owned) => {
                    Self::set_revoke_overrides_for_owned_in_tx(&mut tx, user.id, revokes, owned)
                        .await?;
                }
            }
        }

        // In-tx re-validation: the handler's pre-tx group checks ran
        // against a snapshot that an admin could have invalidated.
        // Re-read the target row and the relevant group rows here so
        // the rules are enforced against the same state the group
        // writes below will land on.
        if !params.requester_is_admin {
            // API invariant: non-admin must supply OwnedSubset + max_bw.
            // Returning Err rolls the tx back rather than silently
            // bypassing the checks below.
            let (owned, max_bw) = match (
                params.permission_write_scope,
                params.requester_bandwidth_max,
            ) {
                (PermissionWriteScope::OwnedSubset(o), Some(m)) => (o, m),
                _ => {
                    return Err(sqlx::Error::Protocol(
                        "non-admin update_user caller must supply OwnedSubset scope \
                         and requester_bandwidth_max"
                            .into(),
                    ));
                }
            };

            let current_row: Option<(Option<i64>, bool)> =
                sqlx::query_as(sql::SQL_SELECT_USER_GROUP_AND_SHARED)
                    .bind(user.id)
                    .fetch_optional(&mut *tx)
                    .await?;
            let Some((current_group_id, current_is_shared)) = current_row else {
                // Row vanished after our own UPDATE — corruption-grade,
                // not a normal race. Roll back rather than panic.
                tx.rollback().await?;
                return Ok(UpdateUserResult::BlockedForGroupAuth);
            };

            if params.remove_group {
                if let Some(old_gid) = current_group_id
                    && !Self::group_perms_within_owned_in_tx(&mut tx, old_gid, owned).await?
                {
                    tx.rollback().await?;
                    return Ok(UpdateUserResult::BlockedForGroupAuth);
                }
            } else if let Some(new_gid) = params.group_id {
                if let Some(old_gid) = current_group_id
                    && old_gid != new_gid
                    && !Self::group_perms_within_owned_in_tx(&mut tx, old_gid, owned).await?
                {
                    tx.rollback().await?;
                    return Ok(UpdateUserResult::BlockedForGroupAuth);
                }

                let new_group_row: Option<(bool, i64)> =
                    sqlx::query_as(sql::SQL_SELECT_GROUP_SHARED_AND_BANDWIDTH)
                        .bind(new_gid)
                        .fetch_optional(&mut *tx)
                        .await?;
                let Some((new_is_shared, new_bw_raw)) = new_group_row else {
                    // Group deleted in the gap — block rather than let
                    // the next UPDATE fall through to a FK error.
                    tx.rollback().await?;
                    return Ok(UpdateUserResult::BlockedForGroupAuth);
                };
                let new_bw = clamp_db_bandwidth_weight(new_bw_raw);

                if current_is_shared != new_is_shared
                    || new_bw > max_bw
                    || !Self::group_perms_within_owned_in_tx(&mut tx, new_gid, owned).await?
                {
                    tx.rollback().await?;
                    return Ok(UpdateUserResult::BlockedForGroupAuth);
                }
            }
        }

        // Update group assignment if requested (atomic with permissions above)
        // remove_group takes precedence over group_id
        if params.remove_group {
            sqlx::query(sql::SQL_UPDATE_USER_GROUP)
                .bind(None::<i64>)
                .bind(user.id)
                .execute(&mut *tx)
                .await?;

            // Cleanup: clear revokes, keep grants (no group to revoke against)
            sqlx::query(sql::SQL_DELETE_REVOKE_PERMISSIONS)
                .bind(user.id)
                .execute(&mut *tx)
                .await?;
        } else if let Some(new_group_id) = params.group_id {
            sqlx::query(sql::SQL_UPDATE_USER_GROUP)
                .bind(Some(new_group_id))
                .bind(user.id)
                .execute(&mut *tx)
                .await?;

            // Cleanup: remove duplicate grants (group already provides them)
            let group_perm_rows: Vec<(String,)> = sqlx::query_as(sql::SQL_SELECT_GROUP_PERMISSIONS)
                .bind(new_group_id)
                .fetch_all(&mut *tx)
                .await?;
            let group_perms: std::collections::HashSet<String> =
                group_perm_rows.into_iter().map(|(p,)| p).collect();

            let grant_rows: Vec<(String,)> = sqlx::query_as(sql::SQL_SELECT_GRANT_PERMISSIONS)
                .bind(user.id)
                .fetch_all(&mut *tx)
                .await?;

            for (perm,) in grant_rows {
                if group_perms.contains(&perm) {
                    sqlx::query(sql::SQL_DELETE_GRANT_PERMISSION)
                        .bind(user.id)
                        .bind(&perm)
                        .execute(&mut *tx)
                        .await?;
                }
            }
        }

        // In-transaction resolution: compute the post-update effective
        // bandwidth weight before commit. A successful update +
        // failed secondary read would be a torn state — the write would
        // land but the caller couldn't refresh session caches. Doing the
        // resolution here means either everything succeeds together or
        // everything rolls back together.
        let resolve_row: Option<(Option<i64>, Option<i64>, bool)> =
            sqlx::query_as(sql::SQL_SELECT_USER_AND_GROUP_BANDWIDTH_WEIGHT)
                .bind(user.id)
                .fetch_optional(&mut *tx)
                .await?;

        // Defense: we just confirmed the UPDATE affected this user's row,
        // so a missing row here would be a corruption-grade invariant
        // break. Fall back to the no-row resolution (matches the standalone
        // resolver) rather than panic — the daemon must not die because a
        // FK was concurrently broken under us.
        let resolved_bandwidth_weight = match resolve_row {
            Some((user_weight, group_weight, is_admin)) => resolve_bandwidth_weight(
                user_weight.map(clamp_db_bandwidth_weight),
                group_weight.map(clamp_db_bandwidth_weight),
                is_admin,
            ),
            None => DEFAULT_BANDWIDTH_WEIGHT,
        };

        // Precedence: promotion auto-clears, then remove_group, then
        // group_id, then unchanged.
        let promotion_auto_clean = final_is_admin && !user.is_admin;
        let final_group_id = if promotion_auto_clean || params.remove_group {
            None
        } else if let Some(new_group_id) = params.group_id {
            Some(new_group_id)
        } else {
            user.group_id
        };

        // In-tx resolution mirroring `get_user_permissions`. Admins
        // resolve to empty (their bypass lives in
        // `UserSession::has_permission`).
        let mut final_permissions = Permissions::new();
        if !final_is_admin {
            let perm_rows: Vec<(String, String)> = sqlx::query_as(sql::SQL_SELECT_PERMISSIONS)
                .bind(user.id)
                .fetch_all(&mut *tx)
                .await?;
            if let Some(gid) = final_group_id {
                let group_perm_rows: Vec<(String,)> =
                    sqlx::query_as(sql::SQL_SELECT_GROUP_PERMISSIONS)
                        .bind(gid)
                        .fetch_all(&mut *tx)
                        .await?;
                for (perm_str,) in &group_perm_rows {
                    if let Some(perm) = Permission::parse(perm_str) {
                        final_permissions.permissions.insert(perm);
                    }
                }
                for (perm_str, override_type) in &perm_rows {
                    if let Some(perm) = Permission::parse(perm_str) {
                        if override_type == "grant" {
                            final_permissions.permissions.insert(perm);
                        } else if override_type == "revoke" {
                            final_permissions.permissions.remove(&perm);
                        }
                    }
                }
            } else {
                for (perm_str, override_type) in &perm_rows {
                    if override_type == "grant"
                        && let Some(perm) = Permission::parse(perm_str)
                    {
                        final_permissions.permissions.insert(perm);
                    }
                }
            }
        }

        tx.commit().await?;

        // Assemble the post-update `UserAccount` from the values we just
        // wrote — same shape as `update_group`'s `GroupRecord`. No extra
        // read; everything is already in scope.
        Ok(UpdateUserResult::Updated {
            account: UserAccount {
                id: user.id,
                username: final_username.to_string(),
                hashed_password: final_password.to_string(),
                is_admin: final_is_admin,
                is_shared: user.is_shared,
                enabled: final_enabled,
                created_at: user.created_at,
                group_id: final_group_id,
                bandwidth_weight: final_bandwidth_weight.map(|w| w as u16),
            },
            resolved_bandwidth_weight,
            permissions: final_permissions,
        })
    }

    pub async fn get_revoke_permissions(
        &self,
        user_id: i64,
    ) -> Result<Vec<Permission>, sqlx::Error> {
        let rows: Vec<(String,)> = sqlx::query_as(sql::SQL_SELECT_REVOKE_PERMISSIONS)
            .bind(user_id)
            .fetch_all(&self.pool)
            .await?;
        Ok(rows
            .into_iter()
            .filter_map(|(p,)| Permission::parse(&p))
            .collect())
    }

    pub async fn get_resolved_bandwidth_weight(&self, user_id: i64) -> Result<u16, sqlx::Error> {
        let mut conn = self.pool.acquire().await?;
        Self::get_resolved_bandwidth_weight_in_tx(&mut conn, user_id).await
    }
}

#[cfg(test)]
impl UserDb {
    /// Check if user has a specific permission with admin override and group
    /// resolution. Production code uses cached permissions on `User`.
    pub async fn has_permission(
        &self,
        user_id: i64,
        permission: Permission,
    ) -> Result<bool, sqlx::Error> {
        let is_admin: Option<(bool,)> = sqlx::query_as(sql::test_sql::SQL_CHECK_IS_ADMIN)
            .bind(user_id)
            .fetch_optional(&self.pool)
            .await?;

        let Some((is_admin,)) = is_admin else {
            return Ok(false);
        };

        if is_admin {
            return Ok(true);
        }

        // Resolves group + overrides: (group_perms ∪ grants) - revokes
        let permissions = self.get_user_permissions(user_id).await?;
        Ok(permissions.permissions.contains(&permission))
    }
}

#[cfg(test)]
mod tests {
    use super::CreateUserParams;
    use super::*;
    use crate::db::testing::*;
    use nexus_common::validators::DEFAULT_ADMIN_BANDWIDTH_WEIGHT;

    impl UserDb {
        /// Test helper: replace all revoke overrides with the given list.
        async fn set_revoke_permissions(
            &self,
            user_id: i64,
            revokes: &[Permission],
        ) -> Result<(), sqlx::Error> {
            let mut tx = self.pool.begin().await?;

            sqlx::query(sql::SQL_DELETE_REVOKE_PERMISSIONS)
                .bind(user_id)
                .execute(&mut *tx)
                .await?;
            for perm in revokes {
                sqlx::query(sql::SQL_INSERT_PERMISSION_OVERRIDE)
                    .bind(user_id)
                    .bind(perm.as_str())
                    .bind("revoke")
                    .execute(&mut *tx)
                    .await?;
            }

            tx.commit().await?;
            Ok(())
        }
    }

    #[tokio::test]
    async fn test_get_user_by_username() {
        let pool = create_test_db().await;
        let db = UserDb::new(pool.clone());

        let created = db
            .create_user(CreateUserParams {
                username: "alice",
                hashed_password: "hash123",
                is_admin: false,
                is_shared: false,
                enabled: true,
                permissions: &Permissions::new(),
                group_id: None,
                revokes: &[],
                bandwidth_weight: None,
            })
            .await
            .unwrap();

        let retrieved = db.get_user_by_username("alice").await.unwrap().unwrap();

        assert_eq!(retrieved.id, created.id);
        assert_eq!(retrieved.username, "alice");
        assert_eq!(retrieved.hashed_password, "hash123");
        assert!(!retrieved.is_admin);
    }

    #[tokio::test]
    async fn test_get_user_by_username_not_found() {
        let pool = create_test_db().await;
        let db = UserDb::new(pool.clone());

        let result = db.get_user_by_username("nonexistent").await.unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_get_user_by_username_case_insensitive() {
        let pool = create_test_db().await;
        let db = UserDb::new(pool.clone());

        db.create_user(CreateUserParams {
            username: "Alice",
            hashed_password: "hash123",
            is_admin: false,
            is_shared: false,
            enabled: true,
            permissions: &Permissions::new(),
            group_id: None,
            revokes: &[],
            bandwidth_weight: None,
        })
        .await
        .unwrap();

        // All case variations should match (case-insensitive lookup)
        let user1 = db.get_user_by_username("Alice").await.unwrap().unwrap();
        let user2 = db.get_user_by_username("alice").await.unwrap().unwrap();
        let user3 = db.get_user_by_username("ALICE").await.unwrap().unwrap();

        assert_eq!(user1.id, user2.id);
        assert_eq!(user1.id, user3.id);

        // But the stored username should preserve original case
        assert_eq!(user1.username, "Alice");
        assert_eq!(user2.username, "Alice");
        assert_eq!(user3.username, "Alice");

        // Cannot create another user with different casing
        let result = db
            .create_user(CreateUserParams {
                username: "alice",
                hashed_password: "hash456",
                is_admin: false,
                is_shared: false,
                enabled: true,
                permissions: &Permissions::new(),
                group_id: None,
                revokes: &[],
                bandwidth_weight: None,
            })
            .await;
        assert!(result.is_err()); // Should fail due to unique constraint
    }

    #[tokio::test]
    async fn test_username_unicode_case_insensitive() {
        let pool = create_test_db().await;
        let db = UserDb::new(pool.clone());

        // Non-ASCII case pair: uppercase É folds to é only under the Unicode
        // `username_lower`. The old ASCII-only COLLATE NOCASE kept them distinct,
        // so these lookups missed and the duplicate create wrongly succeeded.
        db.create_user(CreateUserParams {
            username: "Éclair",
            hashed_password: "hash",
            is_admin: false,
            is_shared: false,
            enabled: true,
            permissions: &Permissions::new(),
            group_id: None,
            revokes: &[],
            bandwidth_weight: None,
        })
        .await
        .unwrap();

        let lower = db.get_user_by_username("éclair").await.unwrap().unwrap();
        let upper = db.get_user_by_username("ÉCLAIR").await.unwrap().unwrap();
        assert_eq!(lower.id, upper.id);
        // Original case preserved in the stored row.
        assert_eq!(lower.username, "Éclair");

        assert!(db.username_exists("éclair").await.unwrap());
        assert!(db.username_exists("ÉCLAIR").await.unwrap());

        // A differently-cased duplicate violates the folded unique index.
        let result = db
            .create_user(CreateUserParams {
                username: "éclair",
                hashed_password: "hash",
                is_admin: false,
                is_shared: false,
                enabled: true,
                permissions: &Permissions::new(),
                group_id: None,
                revokes: &[],
                bandwidth_weight: None,
            })
            .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_get_user_by_id() {
        let pool = create_test_db().await;
        let db = UserDb::new(pool.clone());

        let created = db
            .create_user(CreateUserParams {
                username: "alice",
                hashed_password: "hash123",
                is_admin: false,
                is_shared: false,
                enabled: true,
                permissions: &Permissions::new(),
                group_id: None,
                revokes: &[],
                bandwidth_weight: None,
            })
            .await
            .unwrap();

        let retrieved = db.get_user_by_id(created.id).await.unwrap().unwrap();

        assert_eq!(retrieved.id, created.id);
        assert_eq!(retrieved.username, "alice");
    }

    #[tokio::test]
    async fn test_get_user_by_id_not_found() {
        let pool = create_test_db().await;
        let db = UserDb::new(pool.clone());

        let result = db.get_user_by_id(99999).await.unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_create_user_with_permissions() {
        let pool = create_test_db().await;
        let db = UserDb::new(pool.clone());

        let perms = Permissions::from(&[Permission::UserList, Permission::ChatSend]);

        let user = db
            .create_user(CreateUserParams {
                username: "alice",
                hashed_password: "hash123",
                is_admin: false,
                is_shared: false,
                enabled: true,
                permissions: &perms,
                group_id: None,
                revokes: &[],
                bandwidth_weight: None,
            })
            .await
            .unwrap();

        let (count,): (i64,) = sqlx::query_as(sql::test_sql::SQL_COUNT_USER_PERMISSIONS)
            .bind(user.id)
            .fetch_one(&pool)
            .await
            .unwrap();

        assert_eq!(count, 2, "Should have 2 permissions stored");
    }

    #[tokio::test]
    async fn test_create_admin_does_not_store_permissions() {
        let pool = create_test_db().await;
        let db = UserDb::new(pool.clone());

        let perms = Permissions::from(&[Permission::UserList]);

        let admin = db
            .create_user(CreateUserParams {
                username: "admin",
                hashed_password: "hash123",
                is_admin: true,
                is_shared: false,
                enabled: true,
                permissions: &perms,
                group_id: None,
                revokes: &[],
                bandwidth_weight: None,
            })
            .await
            .unwrap();

        let (count,): (i64,) = sqlx::query_as(sql::test_sql::SQL_COUNT_USER_PERMISSIONS)
            .bind(admin.id)
            .fetch_one(&pool)
            .await
            .unwrap();

        assert_eq!(count, 0, "Admin should have no stored permissions");
    }

    #[tokio::test]
    async fn test_admin_has_all_permissions() {
        let pool = create_test_db().await;
        let db = UserDb::new(pool.clone());

        let admin = db
            .create_user(CreateUserParams {
                username: "admin",
                hashed_password: "hash",
                is_admin: true,
                is_shared: false,
                enabled: true,
                permissions: &Permissions::new(),
                group_id: None,
                revokes: &[],
                bandwidth_weight: None,
            })
            .await
            .unwrap();

        assert!(
            db.has_permission(admin.id, Permission::UserList)
                .await
                .unwrap()
        );
        assert!(
            db.has_permission(admin.id, Permission::UserInfo)
                .await
                .unwrap()
        );
        assert!(
            db.has_permission(admin.id, Permission::ChatSend)
                .await
                .unwrap()
        );
        assert!(
            db.has_permission(admin.id, Permission::ChatReceive)
                .await
                .unwrap()
        );
        assert!(
            db.has_permission(admin.id, Permission::UserDelete)
                .await
                .unwrap()
        );
    }

    #[tokio::test]
    async fn test_non_admin_respects_permissions() {
        let pool = create_test_db().await;
        let db = UserDb::new(pool.clone());

        // Create non-admin with specific permissions
        let perms = Permissions::from(&[Permission::UserList, Permission::ChatReceive]);

        let user = db
            .create_user(CreateUserParams {
                username: "bob",
                hashed_password: "hash",
                is_admin: false,
                is_shared: false,
                enabled: true,
                permissions: &perms,
                group_id: None,
                revokes: &[],
                bandwidth_weight: None,
            })
            .await
            .unwrap();

        assert!(
            db.has_permission(user.id, Permission::UserList)
                .await
                .unwrap()
        );
        assert!(
            db.has_permission(user.id, Permission::ChatReceive)
                .await
                .unwrap()
        );

        assert!(
            !db.has_permission(user.id, Permission::ChatSend)
                .await
                .unwrap()
        );
        assert!(
            !db.has_permission(user.id, Permission::UserDelete)
                .await
                .unwrap()
        );
    }

    #[tokio::test]
    async fn test_has_permission_nonexistent_user() {
        let pool = create_test_db().await;
        let db = UserDb::new(pool.clone());

        let result = db
            .has_permission(99999, Permission::UserList)
            .await
            .unwrap();
        assert!(!result);
    }

    #[tokio::test]
    async fn test_update_user_permissions_replaces_existing() {
        let pool = create_test_db().await;
        let db = UserDb::new(pool.clone());

        let initial_perms = Permissions::from(&[Permission::UserList, Permission::ChatSend]);

        let _user = db
            .create_user(CreateUserParams {
                username: "alice",
                hashed_password: "hash",
                is_admin: false,
                is_shared: false,
                enabled: true,
                permissions: &initial_perms,
                group_id: None,
                revokes: &[],
                bandwidth_weight: None,
            })
            .await
            .unwrap();

        let user = db.get_user_by_username("alice").await.unwrap().unwrap();

        assert!(
            db.has_permission(user.id, Permission::UserList)
                .await
                .unwrap()
        );
        assert!(
            db.has_permission(user.id, Permission::ChatSend)
                .await
                .unwrap()
        );

        // Update user with new permissions (should replace, not merge)
        let new_perms = Permissions::from(&[Permission::ChatReceive]);

        let updated = db
            .update_user(UpdateUserParams {
                username: "alice",
                new_username: None,
                new_password_hash: None,
                is_admin: None,
                enabled: None,
                permissions: Some(&new_perms),
                revokes: None,
                remove_group: false,
                group_id: None,
                bandwidth_weight: None,
                inherit_bandwidth_weight: false,
                requester_is_admin: true,
                permission_write_scope: PermissionWriteScope::ReplaceAll,
                requester_bandwidth_max: None,
            })
            .await
            .unwrap();
        assert!(matches!(updated, UpdateUserResult::Updated { .. }));

        assert!(
            db.has_permission(user.id, Permission::ChatReceive)
                .await
                .unwrap()
        );

        assert!(
            !db.has_permission(user.id, Permission::UserList)
                .await
                .unwrap()
        );
        assert!(
            !db.has_permission(user.id, Permission::ChatSend)
                .await
                .unwrap()
        );
    }

    #[tokio::test]
    async fn test_delete_user_cascades_permissions() {
        let pool = create_test_db().await;
        let db = UserDb::new(pool.clone());

        db.create_user(CreateUserParams {
            username: "admin",
            hashed_password: "hash0",
            is_admin: true,
            is_shared: false,
            enabled: true,
            permissions: &Permissions::new(),
            group_id: None,
            revokes: &[],
            bandwidth_weight: None,
        })
        .await
        .unwrap();

        let perms = Permissions::from(&[Permission::UserList, Permission::ChatSend]);

        let user = db
            .create_user(CreateUserParams {
                username: "bob",
                hashed_password: "hash",
                is_admin: false,
                is_shared: false,
                enabled: true,
                permissions: &perms,
                group_id: None,
                revokes: &[],
                bandwidth_weight: None,
            })
            .await
            .unwrap();

        let (perm_count,): (i64,) = sqlx::query_as(sql::test_sql::SQL_COUNT_USER_PERMISSIONS)
            .bind(user.id)
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(perm_count, 2);

        let deleted = db.delete_user(user.id, true).await.unwrap();
        assert!(deleted, "User should be deleted");

        let (perm_count_after,): (i64,) = sqlx::query_as(sql::test_sql::SQL_COUNT_USER_PERMISSIONS)
            .bind(user.id)
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(
            perm_count_after, 0,
            "Permissions should be deleted via CASCADE"
        );
    }

    #[tokio::test]
    async fn test_delete_nonexistent_user() {
        let pool = create_test_db().await;
        let db = UserDb::new(pool.clone());

        let deleted = db.delete_user(99999, true).await.unwrap();
        assert!(!deleted, "Should return false for non-existent user");
    }

    /// Atomic SQL guard for non-admin → admin delete. Pins the race
    /// protection in isolation from the handler's pre-check (since the
    /// handler can be bypassed by a target promotion between the
    /// pre-check and the DELETE).
    #[tokio::test]
    async fn test_delete_user_non_admin_cannot_delete_admin_target() {
        let pool = create_test_db().await;
        let db = UserDb::new(pool);

        // Two admins so the last-admin guard doesn't conflate with the new one.
        db.create_user(CreateUserParams {
            username: "alice",
            hashed_password: "hash",
            is_admin: true,
            is_shared: false,
            enabled: true,
            permissions: &Permissions::new(),
            group_id: None,
            revokes: &[],
            bandwidth_weight: None,
        })
        .await
        .unwrap();
        let bob = db
            .create_user(CreateUserParams {
                username: "bob",
                hashed_password: "hash",
                is_admin: true,
                is_shared: false,
                enabled: true,
                permissions: &Permissions::new(),
                group_id: None,
                revokes: &[],
                bandwidth_weight: None,
            })
            .await
            .unwrap();

        // Non-admin requester blocked.
        let deleted = db.delete_user(bob.id, false).await.unwrap();
        assert!(!deleted, "non-admin requester must not delete admin");
        assert!(db.get_user_by_id(bob.id).await.unwrap().is_some());

        // Admin requester proceeds.
        let deleted = db.delete_user(bob.id, true).await.unwrap();
        assert!(deleted, "admin requester deletes admin target");
        assert!(db.get_user_by_id(bob.id).await.unwrap().is_none());
    }

    #[tokio::test]
    async fn test_cannot_delete_last_admin() {
        let pool = create_test_db().await;
        let db = UserDb::new(pool.clone());

        let admin = db
            .create_user(CreateUserParams {
                username: "admin",
                hashed_password: "hash",
                is_admin: true,
                is_shared: false,
                enabled: true,
                permissions: &Permissions::new(),
                group_id: None,
                revokes: &[],
                bandwidth_weight: None,
            })
            .await
            .unwrap();

        assert_eq!(count_admins(&pool).await, 1);

        let deleted = db.delete_user(admin.id, true).await.unwrap();
        assert!(!deleted, "Should not delete the last admin");

        assert!(db.get_user_by_id(admin.id).await.unwrap().is_some());
        assert_eq!(count_admins(&pool).await, 1);
    }

    #[tokio::test]
    async fn test_can_delete_admin_when_multiple_exist() {
        let pool = create_test_db().await;
        let db = UserDb::new(pool.clone());

        let admin1 = db
            .create_user(CreateUserParams {
                username: "admin1",
                hashed_password: "hash1",
                is_admin: true,
                is_shared: false,
                enabled: true,
                permissions: &Permissions::new(),
                group_id: None,
                revokes: &[],
                bandwidth_weight: None,
            })
            .await
            .unwrap();
        let admin2 = db
            .create_user(CreateUserParams {
                username: "admin2",
                hashed_password: "hash2",
                is_admin: true,
                is_shared: false,
                enabled: true,
                permissions: &Permissions::new(),
                group_id: None,
                revokes: &[],
                bandwidth_weight: None,
            })
            .await
            .unwrap();

        assert_eq!(count_admins(&pool).await, 2);

        let deleted = db.delete_user(admin1.id, true).await.unwrap();
        assert!(deleted, "Should delete admin when multiple exist");

        assert_eq!(count_admins(&pool).await, 1);
        assert!(db.get_user_by_id(admin2.id).await.unwrap().is_some());
        assert!(db.get_user_by_id(admin1.id).await.unwrap().is_none());
    }

    #[tokio::test]
    async fn test_can_delete_non_admin_user() {
        let pool = create_test_db().await;
        let db = UserDb::new(pool.clone());

        db.create_user(CreateUserParams {
            username: "admin",
            hashed_password: "hash0",
            is_admin: true,
            is_shared: false,
            enabled: true,
            permissions: &Permissions::new(),
            group_id: None,
            revokes: &[],
            bandwidth_weight: None,
        })
        .await
        .unwrap();

        let user = db
            .create_user(CreateUserParams {
                username: "bob",
                hashed_password: "hash",
                is_admin: false,
                is_shared: false,
                enabled: true,
                permissions: &Permissions::new(),
                group_id: None,
                revokes: &[],
                bandwidth_weight: None,
            })
            .await
            .unwrap();

        let deleted = db.delete_user(user.id, true).await.unwrap();
        assert!(deleted, "Should delete non-admin user");

        assert!(db.get_user_by_id(user.id).await.unwrap().is_none());
    }

    #[tokio::test]
    async fn test_concurrent_admin_deletion_race_condition() {
        let pool = create_test_db().await;
        let db1 = UserDb::new(pool.clone());
        let db2 = UserDb::new(pool.clone());

        let admin1 = db1
            .create_user(CreateUserParams {
                username: "admin1",
                hashed_password: "hash1",
                is_admin: true,
                is_shared: false,
                enabled: true,
                permissions: &Permissions::new(),
                group_id: None,
                revokes: &[],
                bandwidth_weight: None,
            })
            .await
            .unwrap();
        let admin2 = db1
            .create_user(CreateUserParams {
                username: "admin2",
                hashed_password: "hash2",
                is_admin: true,
                is_shared: false,
                enabled: true,
                permissions: &Permissions::new(),
                group_id: None,
                revokes: &[],
                bandwidth_weight: None,
            })
            .await
            .unwrap();

        assert_eq!(count_admins(&pool).await, 2);

        // Try to delete both admins simultaneously
        let (result1, result2) = tokio::join!(
            db1.delete_user(admin2.id, true),
            db2.delete_user(admin1.id, true)
        );

        let deleted1 = result1.unwrap();
        let deleted2 = result2.unwrap();

        // At least one deletion should fail (both can't succeed)
        assert!(
            !deleted1 || !deleted2,
            "Both admins should not be deleted! Race condition protection failed."
        );

        // At least one admin should still exist
        let remaining_admins = count_admins(&pool).await;
        assert!(
            remaining_admins >= 1,
            "No admins left! Race condition protection failed. Count: {}",
            remaining_admins
        );
    }

    #[tokio::test]
    async fn test_concurrent_permission_updates() {
        let pool = create_test_db().await;
        let db1 = UserDb::new(pool.clone());
        let db2 = UserDb::new(pool.clone());

        db1.create_user(CreateUserParams {
            username: "admin",
            hashed_password: "hash0",
            is_admin: true,
            is_shared: false,
            enabled: true,
            permissions: &Permissions::new(),
            group_id: None,
            revokes: &[],
            bandwidth_weight: None,
        })
        .await
        .unwrap();

        let user = db1
            .create_user(CreateUserParams {
                username: "bob",
                hashed_password: "hash",
                is_admin: false,
                is_shared: false,
                enabled: true,
                permissions: &Permissions::new(),
                group_id: None,
                revokes: &[],
                bandwidth_weight: None,
            })
            .await
            .unwrap();

        let perms1 = Permissions::from(&[Permission::UserList]);

        let perms2 = Permissions::from(&[Permission::ChatSend]);

        let (result1, result2) = tokio::join!(
            db1.update_user(UpdateUserParams {
                username: "bob",
                new_username: None,
                new_password_hash: None,
                is_admin: None,
                enabled: None,
                permissions: Some(&perms1),
                revokes: None,
                remove_group: false,
                group_id: None,
                bandwidth_weight: None,
                inherit_bandwidth_weight: false,
                requester_is_admin: true,
                permission_write_scope: PermissionWriteScope::ReplaceAll,
                requester_bandwidth_max: None,
            }),
            db2.update_user(UpdateUserParams {
                username: "bob",
                new_username: None,
                new_password_hash: None,
                is_admin: None,
                enabled: None,
                permissions: Some(&perms2),
                revokes: None,
                remove_group: false,
                group_id: None,
                bandwidth_weight: None,
                inherit_bandwidth_weight: false,
                requester_is_admin: true,
                permission_write_scope: PermissionWriteScope::ReplaceAll,
                requester_bandwidth_max: None,
            })
        );

        // Both operations should succeed
        assert!(result1.is_ok());
        assert!(result2.is_ok());

        // User should have one of the permission sets (last write wins)
        let has_userlist = db1
            .has_permission(user.id, Permission::UserList)
            .await
            .unwrap();
        let has_chatsend = db1
            .has_permission(user.id, Permission::ChatSend)
            .await
            .unwrap();

        // Should have one or the other (XOR), but not both
        assert!(
            has_userlist ^ has_chatsend,
            "User should have one permission set, not both or neither"
        );
    }

    #[tokio::test]
    async fn test_create_first_user_if_none_exist_success() {
        let pool = create_test_db().await;
        let db = UserDb::new(pool.clone());

        let (count,): (i64,) = sqlx::query_as(sql::test_sql::SQL_COUNT_USERS)
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(count, 1, "Should only have guest account from migration");

        // Guest account is excluded from the existence check.
        let result = db
            .create_first_user_if_none_exist("admin", "hash123")
            .await
            .unwrap();

        assert!(result.is_some(), "Should create first user");
        let user = result.unwrap();
        assert_eq!(user.username, "admin");
        assert!(user.is_admin, "First user should be admin");
        assert!(user.enabled, "First user should be enabled");

        let (count_after,): (i64,) = sqlx::query_as(sql::test_sql::SQL_COUNT_USERS)
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(count_after, 2, "Should have guest and admin");
    }

    #[tokio::test]
    async fn test_create_first_user_if_none_exist_users_already_exist() {
        let pool = create_test_db().await;
        let db = UserDb::new(pool.clone());

        db.create_user(CreateUserParams {
            username: "existing",
            hashed_password: "hash",
            is_admin: false,
            is_shared: false,
            enabled: true,
            permissions: &Permissions::new(),
            group_id: None,
            revokes: &[],
            bandwidth_weight: None,
        })
        .await
        .unwrap();

        let result = db
            .create_first_user_if_none_exist("admin", "hash123")
            .await
            .unwrap();

        assert!(result.is_none(), "Should return None when users exist");

        let (count,): (i64,) = sqlx::query_as(sql::test_sql::SQL_COUNT_USERS)
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(count, 2, "Should still have only guest and existing user");
    }

    #[tokio::test]
    async fn test_get_user_permissions() {
        let pool = create_test_db().await;
        let db = UserDb::new(pool.clone());

        let perms = Permissions::from(&[
            Permission::UserList,
            Permission::ChatSend,
            Permission::UserInfo,
        ]);

        let user = db
            .create_user(CreateUserParams {
                username: "alice",
                hashed_password: "hash",
                is_admin: false,
                is_shared: false,
                enabled: true,
                permissions: &perms,
                group_id: None,
                revokes: &[],
                bandwidth_weight: None,
            })
            .await
            .unwrap();

        let retrieved_perms = db.get_user_permissions(user.id).await.unwrap();
        let retrieved_vec = retrieved_perms.to_vec();

        assert_eq!(retrieved_vec.len(), 3);
        assert!(retrieved_vec.contains(&Permission::UserList));
        assert!(retrieved_vec.contains(&Permission::ChatSend));
        assert!(retrieved_vec.contains(&Permission::UserInfo));
    }

    #[tokio::test]
    async fn test_get_user_permissions_empty() {
        let pool = create_test_db().await;
        let db = UserDb::new(pool.clone());

        let user = db
            .create_user(CreateUserParams {
                username: "bob",
                hashed_password: "hash",
                is_admin: false,
                is_shared: false,
                enabled: true,
                permissions: &Permissions::new(),
                group_id: None,
                revokes: &[],
                bandwidth_weight: None,
            })
            .await
            .unwrap();

        let perms = db.get_user_permissions(user.id).await.unwrap();
        assert_eq!(perms.to_vec().len(), 0);
    }

    #[tokio::test]
    async fn test_get_user_permissions_admin() {
        let pool = create_test_db().await;
        let db = UserDb::new(pool.clone());

        let admin = db
            .create_user(CreateUserParams {
                username: "admin",
                hashed_password: "hash",
                is_admin: true,
                is_shared: false,
                enabled: true,
                permissions: &Permissions::new(),
                group_id: None,
                revokes: &[],
                bandwidth_weight: None,
            })
            .await
            .unwrap();

        let perms = db.get_user_permissions(admin.id).await.unwrap();
        assert_eq!(
            perms.to_vec().len(),
            0,
            "Admin permissions not stored in DB"
        );
    }

    #[tokio::test]
    async fn test_username_exists_found() {
        let pool = create_test_db().await;
        let db = UserDb::new(pool.clone());

        db.create_user(CreateUserParams {
            username: "alice",
            hashed_password: "hash",
            is_admin: false,
            is_shared: false,
            enabled: true,
            permissions: &Permissions::new(),
            group_id: None,
            revokes: &[],
            bandwidth_weight: None,
        })
        .await
        .unwrap();

        assert!(db.username_exists("alice").await.unwrap());

        // Case-insensitive matches.
        assert!(db.username_exists("Alice").await.unwrap());
        assert!(db.username_exists("ALICE").await.unwrap());
    }

    #[tokio::test]
    async fn test_username_exists_not_found() {
        let pool = create_test_db().await;
        let db = UserDb::new(pool.clone());

        assert!(!db.username_exists("alice").await.unwrap());

        db.create_user(CreateUserParams {
            username: "bob",
            hashed_password: "hash",
            is_admin: false,
            is_shared: false,
            enabled: true,
            permissions: &Permissions::new(),
            group_id: None,
            revokes: &[],
            bandwidth_weight: None,
        })
        .await
        .unwrap();

        // alice still doesn't exist
        assert!(!db.username_exists("alice").await.unwrap());
    }

    #[tokio::test]
    async fn test_create_shared_account() {
        let pool = create_test_db().await;
        let db = UserDb::new(pool.clone());

        let shared = db
            .create_user(CreateUserParams {
                username: "shared_acct",
                hashed_password: "hash",
                is_admin: false,
                is_shared: true,
                enabled: true,
                permissions: &Permissions::new(),
                group_id: None,
                revokes: &[],
                bandwidth_weight: None,
            })
            .await
            .unwrap();

        assert_eq!(shared.username, "shared_acct");
        assert!(!shared.is_admin);
        assert!(shared.is_shared);
        assert!(shared.enabled);

        let retrieved = db
            .get_user_by_username("shared_acct")
            .await
            .unwrap()
            .unwrap();
        assert!(retrieved.is_shared);
        assert!(!retrieved.is_admin);
    }

    #[tokio::test]
    async fn test_shared_account_in_get_all_users() {
        let pool = create_test_db().await;
        let db = UserDb::new(pool.clone());

        // Create mix of regular and shared accounts
        db.create_user(CreateUserParams {
            username: "alice",
            hashed_password: "hash",
            is_admin: false,
            is_shared: false,
            enabled: true,
            permissions: &Permissions::new(),
            group_id: None,
            revokes: &[],
            bandwidth_weight: None,
        })
        .await
        .unwrap();
        db.create_user(CreateUserParams {
            username: "shared_acct",
            hashed_password: "hash",
            is_admin: false,
            is_shared: true,
            enabled: true,
            permissions: &Permissions::new(),
            group_id: None,
            revokes: &[],
            bandwidth_weight: None,
        })
        .await
        .unwrap();
        db.create_user(CreateUserParams {
            username: "bob",
            hashed_password: "hash",
            is_admin: true,
            is_shared: false,
            enabled: true,
            permissions: &Permissions::new(),
            group_id: None,
            revokes: &[],
            bandwidth_weight: None,
        })
        .await
        .unwrap();

        let all_users = db.get_all_users().await.unwrap();
        assert_eq!(
            all_users.len(),
            4,
            "Should have guest, alice, shared_acct, bob"
        );

        let alice = all_users.iter().find(|u| u.username == "alice").unwrap();
        assert!(!alice.is_shared);
        assert!(!alice.is_admin);

        let shared = all_users
            .iter()
            .find(|u| u.username == "shared_acct")
            .unwrap();
        assert!(shared.is_shared);
        assert!(!shared.is_admin);

        let bob = all_users.iter().find(|u| u.username == "bob").unwrap();
        assert!(!bob.is_shared);
        assert!(bob.is_admin);
    }

    #[tokio::test]
    async fn test_shared_account_is_shared_immutable() {
        let pool = create_test_db().await;
        let db = UserDb::new(pool.clone());

        db.create_user(CreateUserParams {
            username: "admin",
            hashed_password: "hash",
            is_admin: true,
            is_shared: false,
            enabled: true,
            permissions: &Permissions::new(),
            group_id: None,
            revokes: &[],
            bandwidth_weight: None,
        })
        .await
        .unwrap();

        let shared = db
            .create_user(CreateUserParams {
                username: "shared_acct",
                hashed_password: "hash",
                is_admin: false,
                is_shared: true,
                enabled: true,
                permissions: &Permissions::new(),
                group_id: None,
                revokes: &[],
                bandwidth_weight: None,
            })
            .await
            .unwrap();

        assert!(shared.is_shared);

        // Update the user (change password, etc.) - is_shared should remain unchanged
        // Note: update_user doesn't have is_shared parameter because it's immutable
        let UpdateUserResult::Updated { account, .. } = db
            .update_user(UpdateUserParams {
                username: "shared_acct",
                new_username: None,
                new_password_hash: Some("new_hash"),
                is_admin: None,
                enabled: None,
                permissions: None,
                revokes: None,
                remove_group: false,
                group_id: None,
                bandwidth_weight: None,
                inherit_bandwidth_weight: false,
                requester_is_admin: true,
                permission_write_scope: PermissionWriteScope::ReplaceAll,
                requester_bandwidth_max: None,
            })
            .await
            .unwrap()
        else {
            panic!("update should have succeeded");
        };
        assert!(
            account.is_shared,
            "is_shared should remain true after update"
        );
        assert_eq!(account.hashed_password, "new_hash");
    }

    #[tokio::test]
    async fn test_create_user_with_group() {
        let pool = create_test_db().await;
        let db = UserDb::new(pool.clone());
        let group_db = crate::db::GroupDb::new(pool.clone());

        let group = group_db
            .create_group(
                "Mods",
                false,
                &Permissions::from(&[Permission::ChatSend, Permission::UserKick]),
                1,
            )
            .await
            .unwrap();

        let user = db
            .create_user(CreateUserParams {
                username: "alice",
                hashed_password: "hash",
                is_admin: false,
                is_shared: false,
                enabled: true,
                permissions: &Permissions::new(),
                group_id: Some(group.id),
                revokes: &[],
                bandwidth_weight: None,
            })
            .await
            .unwrap();

        assert_eq!(user.group_id, Some(group.id));

        let fetched = db.get_user_by_username("alice").await.unwrap().unwrap();
        assert_eq!(fetched.group_id, Some(group.id));
    }

    #[tokio::test]
    async fn test_create_user_without_group() {
        let pool = create_test_db().await;
        let db = UserDb::new(pool.clone());

        let user = db
            .create_user(CreateUserParams {
                username: "alice",
                hashed_password: "hash",
                is_admin: false,
                is_shared: false,
                enabled: true,
                permissions: &Permissions::new(),
                group_id: None,
                revokes: &[],
                bandwidth_weight: None,
            })
            .await
            .unwrap();

        assert_eq!(user.group_id, None);
    }

    #[tokio::test]
    async fn test_get_user_permissions_no_group_legacy() {
        let pool = create_test_db().await;
        let db = UserDb::new(pool.clone());

        // User with no group — legacy behavior (individual grants only)
        let perms = Permissions::from(&[Permission::ChatSend, Permission::UserList]);

        let user = db
            .create_user(CreateUserParams {
                username: "alice",
                hashed_password: "hash",
                is_admin: false,
                is_shared: false,
                enabled: true,
                permissions: &perms,
                group_id: None,
                revokes: &[],
                bandwidth_weight: None,
            })
            .await
            .unwrap();

        let effective = db.get_user_permissions(user.id).await.unwrap();
        let vec = effective.to_vec();
        assert_eq!(vec.len(), 2);
        assert!(vec.contains(&Permission::ChatSend));
        assert!(vec.contains(&Permission::UserList));
    }

    #[tokio::test]
    async fn test_get_user_permissions_with_group_base() {
        let pool = create_test_db().await;
        let db = UserDb::new(pool.clone());
        let group_db = crate::db::GroupDb::new(pool.clone());

        let group = group_db
            .create_group(
                "Mods",
                false,
                &Permissions::from(&[Permission::ChatSend, Permission::UserKick]),
                1,
            )
            .await
            .unwrap();

        let user = db
            .create_user(CreateUserParams {
                username: "alice",
                hashed_password: "hash",
                is_admin: false,
                is_shared: false,
                enabled: true,
                permissions: &Permissions::new(),
                group_id: Some(group.id),
                revokes: &[],
                bandwidth_weight: None,
            })
            .await
            .unwrap();

        let effective = db.get_user_permissions(user.id).await.unwrap();
        let vec = effective.to_vec();
        assert_eq!(vec.len(), 2);
        assert!(vec.contains(&Permission::ChatSend));
        assert!(vec.contains(&Permission::UserKick));
    }

    #[tokio::test]
    async fn test_get_user_permissions_group_plus_grant_override() {
        let pool = create_test_db().await;
        let db = UserDb::new(pool.clone());
        let group_db = crate::db::GroupDb::new(pool.clone());

        let group = group_db
            .create_group(
                "Basic",
                false,
                &Permissions::from(&[Permission::ChatSend]),
                1,
            )
            .await
            .unwrap();

        let user = db
            .create_user(CreateUserParams {
                username: "alice",
                hashed_password: "hash",
                is_admin: false,
                is_shared: false,
                enabled: true,
                permissions: &Permissions::new(),
                group_id: Some(group.id),
                revokes: &[],
                bandwidth_weight: None,
            })
            .await
            .unwrap();

        // Add a grant override for user_list (not in group)
        sqlx::query(sql::SQL_INSERT_PERMISSION_OVERRIDE)
            .bind(user.id)
            .bind("user_list")
            .bind("grant")
            .execute(&pool)
            .await
            .unwrap();

        let effective = db.get_user_permissions(user.id).await.unwrap();
        let vec = effective.to_vec();
        assert_eq!(vec.len(), 2);
        assert!(vec.contains(&Permission::ChatSend)); // from group
        assert!(vec.contains(&Permission::UserList)); // from grant override
    }

    #[tokio::test]
    async fn test_get_user_permissions_group_with_revoke_override() {
        let pool = create_test_db().await;
        let db = UserDb::new(pool.clone());
        let group_db = crate::db::GroupDb::new(pool.clone());

        let group = group_db
            .create_group(
                "Mods",
                false,
                &Permissions::from(&[Permission::ChatSend, Permission::UserKick]),
                1,
            )
            .await
            .unwrap();

        let user = db
            .create_user(CreateUserParams {
                username: "alice",
                hashed_password: "hash",
                is_admin: false,
                is_shared: false,
                enabled: true,
                permissions: &Permissions::new(),
                group_id: Some(group.id),
                revokes: &[],
                bandwidth_weight: None,
            })
            .await
            .unwrap();

        sqlx::query(sql::SQL_INSERT_PERMISSION_OVERRIDE)
            .bind(user.id)
            .bind("user_kick")
            .bind("revoke")
            .execute(&pool)
            .await
            .unwrap();

        let effective = db.get_user_permissions(user.id).await.unwrap();
        let vec = effective.to_vec();
        assert_eq!(vec.len(), 1);
        assert!(vec.contains(&Permission::ChatSend)); // from group
        assert!(!vec.contains(&Permission::UserKick)); // revoked
    }

    #[tokio::test]
    async fn test_get_user_permissions_group_grant_and_revoke_overrides() {
        let pool = create_test_db().await;
        let db = UserDb::new(pool.clone());
        let group_db = crate::db::GroupDb::new(pool.clone());

        let group = group_db
            .create_group(
                "Mods",
                false,
                &Permissions::from(&[
                    Permission::ChatSend,
                    Permission::UserKick,
                    Permission::BanCreate,
                ]),
                1,
            )
            .await
            .unwrap();

        let user = db
            .create_user(CreateUserParams {
                username: "alice",
                hashed_password: "hash",
                is_admin: false,
                is_shared: false,
                enabled: true,
                permissions: &Permissions::new(),
                group_id: Some(group.id),
                revokes: &[],
                bandwidth_weight: None,
            })
            .await
            .unwrap();

        // Grant override: user_list (not in group)
        sqlx::query(sql::SQL_INSERT_PERMISSION_OVERRIDE)
            .bind(user.id)
            .bind("user_list")
            .bind("grant")
            .execute(&pool)
            .await
            .unwrap();

        // Revoke override: ban_create (in group but revoked for this user)
        sqlx::query(sql::SQL_INSERT_PERMISSION_OVERRIDE)
            .bind(user.id)
            .bind("ban_create")
            .bind("revoke")
            .execute(&pool)
            .await
            .unwrap();

        // Effective = (chat_send, user_kick, ban_create) ∪ (user_list) - (ban_create)
        //           = chat_send, user_kick, user_list
        let effective = db.get_user_permissions(user.id).await.unwrap();
        let vec = effective.to_vec();
        assert_eq!(vec.len(), 3);
        assert!(vec.contains(&Permission::ChatSend));
        assert!(vec.contains(&Permission::UserKick));
        assert!(vec.contains(&Permission::UserList));
        assert!(!vec.contains(&Permission::BanCreate));
    }

    #[tokio::test]
    async fn test_get_user_permissions_revoke_nonexistent_is_noop() {
        let pool = create_test_db().await;
        let db = UserDb::new(pool.clone());
        let group_db = crate::db::GroupDb::new(pool.clone());

        let group = group_db
            .create_group(
                "Basic",
                false,
                &Permissions::from(&[Permission::ChatSend]),
                1,
            )
            .await
            .unwrap();

        let user = db
            .create_user(CreateUserParams {
                username: "alice",
                hashed_password: "hash",
                is_admin: false,
                is_shared: false,
                enabled: true,
                permissions: &Permissions::new(),
                group_id: Some(group.id),
                revokes: &[],
                bandwidth_weight: None,
            })
            .await
            .unwrap();

        // Revoke user_kick — not in group, so this is a no-op
        sqlx::query(sql::SQL_INSERT_PERMISSION_OVERRIDE)
            .bind(user.id)
            .bind("user_kick")
            .bind("revoke")
            .execute(&pool)
            .await
            .unwrap();

        let effective = db.get_user_permissions(user.id).await.unwrap();
        let vec = effective.to_vec();
        assert_eq!(vec.len(), 1);
        assert!(vec.contains(&Permission::ChatSend));
    }

    #[tokio::test]
    async fn test_get_user_permissions_no_group_ignores_revokes() {
        let pool = create_test_db().await;
        let db = UserDb::new(pool.clone());

        // User with no group, but has both grant and revoke overrides
        let perms = Permissions::from(&[Permission::ChatSend]);

        let user = db
            .create_user(CreateUserParams {
                username: "alice",
                hashed_password: "hash",
                is_admin: false,
                is_shared: false,
                enabled: true,
                permissions: &perms,
                group_id: None,
                revokes: &[],
                bandwidth_weight: None,
            })
            .await
            .unwrap();

        // Manually insert a revoke override (shouldn't happen in practice
        // without a group, but test defense-in-depth)
        sqlx::query(sql::SQL_INSERT_PERMISSION_OVERRIDE)
            .bind(user.id)
            .bind("user_kick")
            .bind("revoke")
            .execute(&pool)
            .await
            .unwrap();

        // Legacy path: only grants are considered
        let effective = db.get_user_permissions(user.id).await.unwrap();
        let vec = effective.to_vec();
        assert_eq!(vec.len(), 1);
        assert!(vec.contains(&Permission::ChatSend));
    }

    #[tokio::test]
    async fn test_get_user_permissions_nonexistent_user() {
        let pool = create_test_db().await;
        let db = UserDb::new(pool.clone());

        let effective = db.get_user_permissions(99999).await.unwrap();
        assert_eq!(effective.to_vec().len(), 0);
    }

    #[tokio::test]
    async fn test_get_user_permissions_empty_group() {
        let pool = create_test_db().await;
        let db = UserDb::new(pool.clone());
        let group_db = crate::db::GroupDb::new(pool.clone());

        let group = group_db
            .create_group("Empty", false, &Permissions::new(), 1)
            .await
            .unwrap();

        let user = db
            .create_user(CreateUserParams {
                username: "alice",
                hashed_password: "hash",
                is_admin: false,
                is_shared: false,
                enabled: true,
                permissions: &Permissions::new(),
                group_id: Some(group.id),
                revokes: &[],
                bandwidth_weight: None,
            })
            .await
            .unwrap();

        let effective = db.get_user_permissions(user.id).await.unwrap();
        assert_eq!(effective.to_vec().len(), 0);
    }

    #[tokio::test]
    async fn test_get_user_permissions_empty_group_with_grant_override() {
        let pool = create_test_db().await;
        let db = UserDb::new(pool.clone());
        let group_db = crate::db::GroupDb::new(pool.clone());

        // Empty group, but the user has a grant override.
        let group = group_db
            .create_group("Empty", false, &Permissions::new(), 1)
            .await
            .unwrap();

        let user = db
            .create_user(CreateUserParams {
                username: "alice",
                hashed_password: "hash",
                is_admin: false,
                is_shared: false,
                enabled: true,
                permissions: &Permissions::new(),
                group_id: Some(group.id),
                revokes: &[],
                bandwidth_weight: None,
            })
            .await
            .unwrap();

        sqlx::query(sql::SQL_INSERT_PERMISSION_OVERRIDE)
            .bind(user.id)
            .bind("chat_send")
            .bind("grant")
            .execute(&pool)
            .await
            .unwrap();

        let effective = db.get_user_permissions(user.id).await.unwrap();
        let vec = effective.to_vec();
        assert_eq!(vec.len(), 1);
        assert!(vec.contains(&Permission::ChatSend));
    }

    #[tokio::test]
    async fn test_get_all_users_includes_group_id() {
        let pool = create_test_db().await;
        let db = UserDb::new(pool.clone());
        let group_db = crate::db::GroupDb::new(pool.clone());

        let group = group_db
            .create_group("Team", false, &Permissions::new(), 1)
            .await
            .unwrap();

        db.create_user(CreateUserParams {
            username: "alice",
            hashed_password: "hash",
            is_admin: false,
            is_shared: false,
            enabled: true,
            permissions: &Permissions::new(),
            group_id: Some(group.id),
            revokes: &[],
            bandwidth_weight: None,
        })
        .await
        .unwrap();
        db.create_user(CreateUserParams {
            username: "bob",
            hashed_password: "hash",
            is_admin: false,
            is_shared: false,
            enabled: true,
            permissions: &Permissions::new(),
            group_id: None,
            revokes: &[],
            bandwidth_weight: None,
        })
        .await
        .unwrap();

        let all = db.get_all_users().await.unwrap();
        let alice = all.iter().find(|u| u.username == "alice").unwrap();
        let bob = all.iter().find(|u| u.username == "bob").unwrap();

        assert_eq!(alice.group_id, Some(group.id));
        assert_eq!(bob.group_id, None);
    }

    #[tokio::test]
    async fn test_update_user_assign_group() {
        let pool = create_test_db().await;
        let db = UserDb::new(pool.clone());
        let group_db = crate::db::GroupDb::new(pool.clone());

        let group1 = group_db
            .create_group(
                "Group1",
                false,
                &Permissions::from(&[Permission::ChatSend]),
                1,
            )
            .await
            .unwrap();
        let group2 = group_db
            .create_group(
                "Group2",
                false,
                &Permissions::from(&[Permission::UserKick]),
                1,
            )
            .await
            .unwrap();

        let user = db
            .create_user(CreateUserParams {
                username: "alice",
                hashed_password: "hash",
                is_admin: false,
                is_shared: false,
                enabled: true,
                permissions: &Permissions::new(),
                group_id: Some(group1.id),
                revokes: &[],
                bandwidth_weight: None,
            })
            .await
            .unwrap();

        assert_eq!(user.group_id, Some(group1.id));

        let UpdateUserResult::Updated { account, .. } = db
            .update_user(UpdateUserParams {
                username: "alice",
                new_username: None,
                new_password_hash: None,
                is_admin: None,
                enabled: None,
                permissions: None,
                revokes: None,
                remove_group: false,
                group_id: Some(group2.id),
                bandwidth_weight: None,
                inherit_bandwidth_weight: false,
                requester_is_admin: true,
                permission_write_scope: PermissionWriteScope::ReplaceAll,
                requester_bandwidth_max: None,
            })
            .await
            .unwrap()
        else {
            panic!("update should have succeeded");
        };
        assert_eq!(account.group_id, Some(group2.id), "group reassigned");
        assert_eq!(account.id, user.id, "same user");
    }

    #[tokio::test]
    async fn test_update_user_remove_group() {
        let pool = create_test_db().await;
        let db = UserDb::new(pool.clone());
        let group_db = crate::db::GroupDb::new(pool.clone());

        let group = group_db
            .create_group(
                "Team",
                false,
                &Permissions::from(&[Permission::ChatSend]),
                1,
            )
            .await
            .unwrap();

        db.create_user(CreateUserParams {
            username: "alice",
            hashed_password: "hash",
            is_admin: false,
            is_shared: false,
            enabled: true,
            permissions: &Permissions::new(),
            group_id: Some(group.id),
            revokes: &[],
            bandwidth_weight: None,
        })
        .await
        .unwrap();

        let UpdateUserResult::Updated { account, .. } = db
            .update_user(UpdateUserParams {
                username: "alice",
                new_username: None,
                new_password_hash: None,
                is_admin: None,
                enabled: None,
                permissions: None,
                revokes: None,
                remove_group: true,
                group_id: None,
                bandwidth_weight: None,
                inherit_bandwidth_weight: false,
                requester_is_admin: true,
                permission_write_scope: PermissionWriteScope::ReplaceAll,
                requester_bandwidth_max: None,
            })
            .await
            .unwrap()
        else {
            panic!("update should have succeeded");
        };
        assert_eq!(account.group_id, None);
    }

    #[tokio::test]
    async fn test_update_user_remove_group_takes_precedence() {
        let pool = create_test_db().await;
        let db = UserDb::new(pool.clone());
        let group_db = crate::db::GroupDb::new(pool.clone());

        let group1 = group_db
            .create_group(
                "Group1",
                false,
                &Permissions::from(&[Permission::ChatSend]),
                1,
            )
            .await
            .unwrap();
        let group2 = group_db
            .create_group(
                "Group2",
                false,
                &Permissions::from(&[Permission::UserKick]),
                1,
            )
            .await
            .unwrap();

        db.create_user(CreateUserParams {
            username: "alice",
            hashed_password: "hash",
            is_admin: false,
            is_shared: false,
            enabled: true,
            permissions: &Permissions::new(),
            group_id: Some(group1.id),
            revokes: &[],
            bandwidth_weight: None,
        })
        .await
        .unwrap();

        // Both remove_group and group_id set — remove wins
        let UpdateUserResult::Updated { account, .. } = db
            .update_user(UpdateUserParams {
                username: "alice",
                new_username: None,
                new_password_hash: None,
                is_admin: None,
                enabled: None,
                permissions: None,
                revokes: None,
                remove_group: true,
                group_id: Some(group2.id),
                bandwidth_weight: None,
                inherit_bandwidth_weight: false,
                requester_is_admin: true,
                permission_write_scope: PermissionWriteScope::ReplaceAll,
                requester_bandwidth_max: None,
            })
            .await
            .unwrap()
        else {
            panic!("update should have succeeded");
        };
        assert_eq!(account.group_id, None, "remove_group wins over group_id");
    }

    #[tokio::test]
    async fn test_get_revoke_permissions_empty() {
        let pool = create_test_db().await;
        let db = UserDb::new(pool.clone());

        let user = db
            .create_user(CreateUserParams {
                username: "alice",
                hashed_password: "hash",
                is_admin: false,
                is_shared: false,
                enabled: true,
                permissions: &Permissions::new(),
                group_id: None,
                revokes: &[],
                bandwidth_weight: None,
            })
            .await
            .unwrap();

        let revokes = db.get_revoke_permissions(user.id).await.unwrap();
        assert!(revokes.is_empty());
    }

    #[tokio::test]
    async fn test_get_revoke_permissions_returns_revokes() {
        let pool = create_test_db().await;
        let db = UserDb::new(pool.clone());
        let group_db = crate::db::GroupDb::new(pool.clone());

        let group = group_db
            .create_group(
                "Mods",
                false,
                &Permissions::from(&[Permission::ChatSend, Permission::UserKick]),
                1,
            )
            .await
            .unwrap();

        let user = db
            .create_user(CreateUserParams {
                username: "alice",
                hashed_password: "hash",
                is_admin: false,
                is_shared: false,
                enabled: true,
                permissions: &Permissions::new(),
                group_id: Some(group.id),
                revokes: &[Permission::UserKick],
                bandwidth_weight: None,
            })
            .await
            .unwrap();

        let revokes = db.get_revoke_permissions(user.id).await.unwrap();
        assert_eq!(revokes.len(), 1);
        assert!(revokes.contains(&Permission::UserKick));
    }

    #[tokio::test]
    async fn test_set_revoke_permissions_replaces_existing() {
        let pool = create_test_db().await;
        let db = UserDb::new(pool.clone());
        let group_db = crate::db::GroupDb::new(pool.clone());

        let group = group_db
            .create_group(
                "Mods",
                false,
                &Permissions::from(&[
                    Permission::ChatSend,
                    Permission::UserKick,
                    Permission::BanCreate,
                ]),
                1,
            )
            .await
            .unwrap();

        let user = db
            .create_user(CreateUserParams {
                username: "alice",
                hashed_password: "hash",
                is_admin: false,
                is_shared: false,
                enabled: true,
                permissions: &Permissions::new(),
                group_id: Some(group.id),
                revokes: &[],
                bandwidth_weight: None,
            })
            .await
            .unwrap();

        db.set_revoke_permissions(user.id, &[Permission::UserKick])
            .await
            .unwrap();
        let revokes = db.get_revoke_permissions(user.id).await.unwrap();
        assert_eq!(revokes.len(), 1);
        assert!(revokes.contains(&Permission::UserKick));

        db.set_revoke_permissions(user.id, &[Permission::BanCreate, Permission::ChatSend])
            .await
            .unwrap();
        let revokes = db.get_revoke_permissions(user.id).await.unwrap();
        assert_eq!(revokes.len(), 2);
        assert!(revokes.contains(&Permission::BanCreate));
        assert!(revokes.contains(&Permission::ChatSend));
        // Old revoke should be gone
        assert!(!revokes.contains(&Permission::UserKick));
    }

    #[tokio::test]
    async fn test_set_revoke_permissions_empty_clears() {
        let pool = create_test_db().await;
        let db = UserDb::new(pool.clone());
        let group_db = crate::db::GroupDb::new(pool.clone());

        let group = group_db
            .create_group(
                "Mods",
                false,
                &Permissions::from(&[Permission::ChatSend, Permission::UserKick]),
                1,
            )
            .await
            .unwrap();

        let user = db
            .create_user(CreateUserParams {
                username: "alice",
                hashed_password: "hash",
                is_admin: false,
                is_shared: false,
                enabled: true,
                permissions: &Permissions::new(),
                group_id: Some(group.id),
                revokes: &[Permission::UserKick],
                bandwidth_weight: None,
            })
            .await
            .unwrap();

        let revokes = db.get_revoke_permissions(user.id).await.unwrap();
        assert_eq!(revokes.len(), 1);

        db.set_revoke_permissions(user.id, &[]).await.unwrap();
        let revokes = db.get_revoke_permissions(user.id).await.unwrap();
        assert!(revokes.is_empty());
    }

    #[tokio::test]
    async fn test_update_user_assign_group_removes_duplicate_grants() {
        let pool = create_test_db().await;
        let db = UserDb::new(pool.clone());
        let group_db = crate::db::GroupDb::new(pool.clone());

        let group = group_db
            .create_group(
                "Mods",
                false,
                &Permissions::from(&[Permission::ChatSend, Permission::UserKick]),
                1,
            )
            .await
            .unwrap();

        let user = db
            .create_user(CreateUserParams {
                username: "alice",
                hashed_password: "hash",
                is_admin: false,
                is_shared: false,
                enabled: true,
                permissions: &Permissions::new(),
                group_id: None,
                revokes: &[],
                bandwidth_weight: None,
            })
            .await
            .unwrap();

        // Add grant overrides: chat_send (overlaps group) and user_list (does not)
        sqlx::query(sql::SQL_INSERT_PERMISSION_OVERRIDE)
            .bind(user.id)
            .bind("chat_send")
            .bind("grant")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query(sql::SQL_INSERT_PERMISSION_OVERRIDE)
            .bind(user.id)
            .bind("user_list")
            .bind("grant")
            .execute(&pool)
            .await
            .unwrap();

        // Assign to group via update_user — should atomically clean up duplicate grants
        let UpdateUserResult::Updated { account, .. } = db
            .update_user(UpdateUserParams {
                username: "alice",
                new_username: None,
                new_password_hash: None,
                is_admin: None,
                enabled: None,
                permissions: None,
                revokes: None,
                remove_group: false,
                group_id: Some(group.id),
                bandwidth_weight: None,
                inherit_bandwidth_weight: false,
                requester_is_admin: true,
                permission_write_scope: PermissionWriteScope::ReplaceAll,
                requester_bandwidth_max: None,
            })
            .await
            .unwrap()
        else {
            panic!("update should have succeeded");
        };

        // chat_send grant should be removed (duplicate with group)
        // user_list grant should be kept (not in group)
        let grant_rows: Vec<(String,)> = sqlx::query_as(sql::SQL_SELECT_GRANT_PERMISSIONS)
            .bind(user.id)
            .fetch_all(&pool)
            .await
            .unwrap();

        let grant_perms: Vec<String> = grant_rows.into_iter().map(|(p,)| p).collect();
        assert_eq!(grant_perms.len(), 1);
        assert!(grant_perms.contains(&"user_list".to_string()));
        assert!(!grant_perms.contains(&"chat_send".to_string()));

        // Group assignment reflected in the returned account.
        assert_eq!(account.group_id, Some(group.id));
    }

    #[tokio::test]
    async fn test_update_user_remove_group_clears_revokes_keeps_grants() {
        let pool = create_test_db().await;
        let db = UserDb::new(pool.clone());
        let group_db = crate::db::GroupDb::new(pool.clone());

        let group = group_db
            .create_group(
                "Mods",
                false,
                &Permissions::from(&[Permission::ChatSend, Permission::UserKick]),
                1,
            )
            .await
            .unwrap();

        // Create user in the group with a revoke override
        let user = db
            .create_user(CreateUserParams {
                username: "alice",
                hashed_password: "hash",
                is_admin: false,
                is_shared: false,
                enabled: true,
                permissions: &Permissions::new(),
                group_id: Some(group.id),
                revokes: &[Permission::UserKick],
                bandwidth_weight: None,
            })
            .await
            .unwrap();

        sqlx::query(sql::SQL_INSERT_PERMISSION_OVERRIDE)
            .bind(user.id)
            .bind("user_list")
            .bind("grant")
            .execute(&pool)
            .await
            .unwrap();

        let revokes = db.get_revoke_permissions(user.id).await.unwrap();
        assert_eq!(revokes.len(), 1);

        // Remove from group via update_user — should atomically clear revokes, keep grants
        let UpdateUserResult::Updated { account, .. } = db
            .update_user(UpdateUserParams {
                username: "alice",
                new_username: None,
                new_password_hash: None,
                is_admin: None,
                enabled: None,
                permissions: None,
                revokes: None,
                remove_group: true,
                group_id: None,
                bandwidth_weight: None,
                inherit_bandwidth_weight: false,
                requester_is_admin: true,
                permission_write_scope: PermissionWriteScope::ReplaceAll,
                requester_bandwidth_max: None,
            })
            .await
            .unwrap()
        else {
            panic!("update should have succeeded");
        };

        // Grants should be kept (become individual permissions)
        let grant_rows: Vec<(String,)> = sqlx::query_as(sql::SQL_SELECT_GRANT_PERMISSIONS)
            .bind(user.id)
            .fetch_all(&pool)
            .await
            .unwrap();
        let grant_perms: Vec<String> = grant_rows.into_iter().map(|(p,)| p).collect();
        assert_eq!(grant_perms.len(), 1);
        assert!(grant_perms.contains(&"user_list".to_string()));

        // Revokes should be cleared (no group to revoke against)
        let revokes = db.get_revoke_permissions(user.id).await.unwrap();
        assert!(revokes.is_empty());

        // Group removal reflected in the returned account.
        assert_eq!(account.group_id, None);
    }

    #[tokio::test]
    async fn test_update_user_permissions_revokes_and_group_change_simultaneously() {
        let pool = create_test_db().await;
        let db = UserDb::new(pool.clone());
        let group_db = crate::db::GroupDb::new(pool.clone());

        let group1 = group_db
            .create_group(
                "OldGroup",
                false,
                &Permissions::from(&[Permission::ChatSend, Permission::ChatReceive]),
                1,
            )
            .await
            .unwrap();
        let group2 = group_db
            .create_group(
                "NewGroup",
                false,
                &Permissions::from(&[Permission::UserList, Permission::UserKick]),
                1,
            )
            .await
            .unwrap();

        let user = db
            .create_user(CreateUserParams {
                username: "alice",
                hashed_password: "hash",
                is_admin: false,
                is_shared: false,
                enabled: true,
                permissions: &Permissions::from(&[Permission::VoiceListen]),
                group_id: Some(group1.id),
                revokes: &[Permission::ChatReceive],
                bandwidth_weight: None,
            })
            .await
            .unwrap();

        // Verify initial state: group1 base (chat_send, chat_receive)
        //   + grant override (voice_listen) - revoke (chat_receive)
        //   = chat_send, voice_listen
        let initial_perms = db.get_user_permissions(user.id).await.unwrap();
        assert!(initial_perms.permissions.contains(&Permission::ChatSend));
        assert!(initial_perms.permissions.contains(&Permission::VoiceListen));
        assert!(!initial_perms.permissions.contains(&Permission::ChatReceive));
        assert_eq!(initial_perms.permissions.len(), 2);

        // Now do the combo update: change permissions + revokes + group all at once
        // - New permissions: voice_listen (grant override on new group)
        // - New revokes: user_kick (revoke from new group)
        // - New group: group2 (user_list, user_kick)
        let UpdateUserResult::Updated { account, .. } = db
            .update_user(UpdateUserParams {
                username: "alice",
                new_username: None,
                new_password_hash: None,
                is_admin: None,
                enabled: None,
                permissions: Some(&Permissions::from(&[Permission::VoiceListen])),
                revokes: Some(&[Permission::UserKick]),
                remove_group: false,
                group_id: Some(group2.id),
                bandwidth_weight: None,
                inherit_bandwidth_weight: false,
                requester_is_admin: true,
                permission_write_scope: PermissionWriteScope::ReplaceAll,
                requester_bandwidth_max: None,
            })
            .await
            .unwrap()
        else {
            panic!("update should have succeeded");
        };

        // Group reassignment reflected in the returned account.
        assert_eq!(account.group_id, Some(group2.id));

        // Verify effective permissions:
        // group2 base (user_list, user_kick)
        //   + grant override (voice_listen) - revoke (user_kick)
        //   = user_list, voice_listen
        let final_perms = db.get_user_permissions(user.id).await.unwrap();
        assert!(
            final_perms.permissions.contains(&Permission::UserList),
            "Should have user_list from new group"
        );
        assert!(
            final_perms.permissions.contains(&Permission::VoiceListen),
            "Should have voice_listen from grant override"
        );
        assert!(
            !final_perms.permissions.contains(&Permission::UserKick),
            "user_kick should be revoked"
        );
        assert!(
            !final_perms.permissions.contains(&Permission::ChatSend),
            "chat_send was from old group, should be gone"
        );
        assert!(
            !final_perms.permissions.contains(&Permission::ChatReceive),
            "chat_receive was from old group, should be gone"
        );
        assert_eq!(final_perms.permissions.len(), 2);

        let revokes = db.get_revoke_permissions(user.id).await.unwrap();
        assert_eq!(revokes.len(), 1);
        assert!(revokes.contains(&Permission::UserKick));

        // Raw override rows: voice_listen should remain as a grant
        // (it doesn't overlap with group2's permissions, so cleanup shouldn't remove it)
        let override_rows: Vec<(String, String)> = sqlx::query_as(sql::SQL_SELECT_PERMISSIONS)
            .bind(user.id)
            .fetch_all(&pool)
            .await
            .unwrap();
        // Should have 2 rows: voice_listen (grant) + user_kick (revoke)
        assert_eq!(
            override_rows.len(),
            2,
            "Should have exactly 2 override rows"
        );
        assert!(
            override_rows
                .iter()
                .any(|(p, t)| p == "voice_listen" && t == "grant"),
            "voice_listen should be stored as a grant override"
        );
        assert!(
            override_rows
                .iter()
                .any(|(p, t)| p == "user_kick" && t == "revoke"),
            "user_kick should be stored as a revoke override"
        );
    }

    /// Helper: build a minimal CreateUserParams with a given bandwidth_weight override.
    /// Lives next to its test users via a `static` so the returned reference satisfies
    /// the `'a` lifetime without each caller allocating its own `Permissions::new()`.
    fn create_user_params<'a>(
        username: &'a str,
        hashed_password: &'a str,
        permissions: &'a Permissions,
        group_id: Option<i64>,
        bandwidth_weight: Option<u16>,
    ) -> CreateUserParams<'a> {
        CreateUserParams {
            username,
            hashed_password,
            is_admin: false,
            is_shared: false,
            enabled: true,
            permissions,
            group_id,
            revokes: &[],
            bandwidth_weight,
        }
    }

    #[tokio::test]
    async fn test_create_user_stores_bandwidth_weight_override() {
        let pool = create_test_db().await;
        let db = UserDb::new(pool);

        let perms = Permissions::new();
        let user = db
            .create_user(create_user_params("alice", "hash", &perms, None, Some(42)))
            .await
            .unwrap();

        let fetched = db.get_user_by_id(user.id).await.unwrap().unwrap();
        assert_eq!(fetched.bandwidth_weight, Some(42));
    }

    #[tokio::test]
    async fn test_create_user_with_none_stores_null() {
        let pool = create_test_db().await;
        let db = UserDb::new(pool);

        let perms = Permissions::new();
        let user = db
            .create_user(create_user_params("alice", "hash", &perms, None, None))
            .await
            .unwrap();

        let fetched = db.get_user_by_id(user.id).await.unwrap().unwrap();
        assert!(fetched.bandwidth_weight.is_none());
    }

    #[tokio::test]
    async fn test_update_user_sets_explicit_weight() {
        let pool = create_test_db().await;
        let db = UserDb::new(pool);

        let perms = Permissions::new();
        db.create_user(create_user_params("alice", "hash", &perms, None, None))
            .await
            .unwrap();

        let UpdateUserResult::Updated { account, .. } = db
            .update_user(UpdateUserParams {
                username: "alice",
                new_username: None,
                new_password_hash: None,
                is_admin: None,
                enabled: None,
                permissions: None,
                revokes: None,
                remove_group: false,
                group_id: None,
                bandwidth_weight: Some(99),
                inherit_bandwidth_weight: false,
                requester_is_admin: true,
                permission_write_scope: PermissionWriteScope::ReplaceAll,
                requester_bandwidth_max: None,
            })
            .await
            .unwrap()
        else {
            panic!("update should have succeeded");
        };
        assert_eq!(account.bandwidth_weight, Some(99));
    }

    #[tokio::test]
    async fn test_update_user_clear_sets_null() {
        let pool = create_test_db().await;
        let db = UserDb::new(pool);

        let perms = Permissions::new();
        db.create_user(create_user_params("alice", "hash", &perms, None, Some(50)))
            .await
            .unwrap();

        let UpdateUserResult::Updated { account, .. } = db
            .update_user(UpdateUserParams {
                username: "alice",
                new_username: None,
                new_password_hash: None,
                is_admin: None,
                enabled: None,
                permissions: None,
                revokes: None,
                remove_group: false,
                group_id: None,
                bandwidth_weight: None,
                inherit_bandwidth_weight: true,
                requester_is_admin: true,
                permission_write_scope: PermissionWriteScope::ReplaceAll,
                requester_bandwidth_max: None,
            })
            .await
            .unwrap()
        else {
            panic!("update should have succeeded");
        };
        assert!(account.bandwidth_weight.is_none());
    }

    #[tokio::test]
    async fn test_update_user_clear_takes_precedence_over_value() {
        let pool = create_test_db().await;
        let db = UserDb::new(pool);

        let perms = Permissions::new();
        db.create_user(create_user_params("alice", "hash", &perms, None, Some(50)))
            .await
            .unwrap();

        let UpdateUserResult::Updated { account, .. } = db
            .update_user(UpdateUserParams {
                username: "alice",
                new_username: None,
                new_password_hash: None,
                is_admin: None,
                enabled: None,
                permissions: None,
                revokes: None,
                remove_group: false,
                group_id: None,
                // Flag wins: stored value should be NULL despite explicit Some(77).
                bandwidth_weight: Some(77),
                inherit_bandwidth_weight: true,
                requester_is_admin: true,
                permission_write_scope: PermissionWriteScope::ReplaceAll,
                requester_bandwidth_max: None,
            })
            .await
            .unwrap()
        else {
            panic!("update should have succeeded");
        };
        assert!(account.bandwidth_weight.is_none());
    }

    #[tokio::test]
    async fn test_update_user_no_bandwidth_change_preserves_existing() {
        let pool = create_test_db().await;
        let db = UserDb::new(pool);

        let perms = Permissions::new();
        db.create_user(create_user_params("alice", "hash", &perms, None, Some(123)))
            .await
            .unwrap();

        // Change username only; leave bandwidth_weight untouched.
        let UpdateUserResult::Updated { account, .. } = db
            .update_user(UpdateUserParams {
                username: "alice",
                new_username: Some("alice2"),
                new_password_hash: None,
                is_admin: None,
                enabled: None,
                permissions: None,
                revokes: None,
                remove_group: false,
                group_id: None,
                bandwidth_weight: None,
                inherit_bandwidth_weight: false,
                requester_is_admin: true,
                permission_write_scope: PermissionWriteScope::ReplaceAll,
                requester_bandwidth_max: None,
            })
            .await
            .unwrap()
        else {
            panic!("update should have succeeded");
        };
        assert_eq!(account.username, "alice2", "rename reflected");
        assert_eq!(account.bandwidth_weight, Some(123), "override preserved");
    }

    /// Regression: the DB-layer's defense-in-depth zero-check on
    /// `bandwidth_weight` must respect the same `inherit_bandwidth_weight`
    /// precedence as the write logic ten lines below it. A request that
    /// carries `bandwidth_weight: Some(0)` alongside
    /// `inherit_bandwidth_weight: true` is logically valid — the zero will
    /// be discarded by the precedence and the override cleared to NULL.
    /// The defense must not reject the request as if the zero were going
    /// to be stored.
    #[tokio::test]
    async fn test_update_user_inherit_with_zero_bandwidth_does_not_reject() {
        let pool = create_test_db().await;
        let db = UserDb::new(pool);

        let perms = Permissions::new();
        db.create_user(create_user_params("alice", "hash", &perms, None, Some(50)))
            .await
            .unwrap();

        // Both fields present: bandwidth_weight=0 (would be invalid on its own)
        // but inherit=true makes the zero get discarded. The DB-layer
        // defense-in-depth must mirror that precedence and accept the call.
        let result = db
            .update_user(UpdateUserParams {
                username: "alice",
                new_username: None,
                new_password_hash: None,
                is_admin: None,
                enabled: None,
                permissions: None,
                revokes: None,
                remove_group: false,
                group_id: None,
                bandwidth_weight: Some(0),
                inherit_bandwidth_weight: true,
                requester_is_admin: true,
                permission_write_scope: PermissionWriteScope::ReplaceAll,
                requester_bandwidth_max: None,
            })
            .await;
        assert!(
            result.is_ok(),
            "inherit=true must short-circuit the zero defense; got {:?}",
            result.err()
        );
        let UpdateUserResult::Updated { account, .. } = result.unwrap() else {
            panic!("row should report as updated");
        };
        // The precedence applied: stored override is now NULL.
        assert!(
            account.bandwidth_weight.is_none(),
            "inherit_bandwidth_weight: true must clear the override regardless of the discarded bandwidth_weight value"
        );
    }

    #[tokio::test]
    async fn test_get_resolved_bandwidth_weight_uses_user_override() {
        let pool = create_test_db().await;
        let user_db = UserDb::new(pool.clone());
        let group_db = crate::db::GroupDb::new(pool);

        let group = group_db
            .create_group("Mods", false, &Permissions::new(), 10)
            .await
            .unwrap();
        let perms = Permissions::new();
        let user = user_db
            .create_user(create_user_params(
                "alice",
                "hash",
                &perms,
                Some(group.id),
                Some(50),
            ))
            .await
            .unwrap();

        // User override (50) wins over group weight (10).
        let resolved = user_db
            .get_resolved_bandwidth_weight(user.id)
            .await
            .unwrap();
        assert_eq!(resolved, 50);
    }

    #[tokio::test]
    async fn test_get_resolved_bandwidth_weight_inherits_from_group() {
        let pool = create_test_db().await;
        let user_db = UserDb::new(pool.clone());
        let group_db = crate::db::GroupDb::new(pool);

        let group = group_db
            .create_group("Mods", false, &Permissions::new(), 25)
            .await
            .unwrap();
        let perms = Permissions::new();
        let user = user_db
            .create_user(create_user_params(
                "alice",
                "hash",
                &perms,
                Some(group.id),
                None,
            ))
            .await
            .unwrap();

        // No user override → group's weight wins.
        let resolved = user_db
            .get_resolved_bandwidth_weight(user.id)
            .await
            .unwrap();
        assert_eq!(resolved, 25);
    }

    #[tokio::test]
    async fn test_get_resolved_bandwidth_weight_falls_back_to_default() {
        let pool = create_test_db().await;
        let db = UserDb::new(pool);

        let perms = Permissions::new();
        let user = db
            .create_user(create_user_params("alice", "hash", &perms, None, None))
            .await
            .unwrap();

        // No user override, no group → server default.
        let resolved = db.get_resolved_bandwidth_weight(user.id).await.unwrap();
        assert_eq!(resolved, DEFAULT_BANDWIDTH_WEIGHT);
    }

    #[tokio::test]
    async fn test_get_resolved_bandwidth_weight_admin_default() {
        let pool = create_test_db().await;
        let db = UserDb::new(pool);

        let perms = Permissions::new();
        let user = db
            .create_user(CreateUserParams {
                username: "admin",
                hashed_password: "hash",
                is_admin: true,
                is_shared: false,
                enabled: true,
                permissions: &perms,
                group_id: None,
                revokes: &[],
                bandwidth_weight: None,
            })
            .await
            .unwrap();

        // Admin with no override → admin default (skips group lookup).
        let resolved = db.get_resolved_bandwidth_weight(user.id).await.unwrap();
        assert_eq!(resolved, DEFAULT_ADMIN_BANDWIDTH_WEIGHT);
    }

    #[tokio::test]
    async fn test_get_resolved_bandwidth_weight_admin_override_wins() {
        let pool = create_test_db().await;
        let db = UserDb::new(pool);

        let perms = Permissions::new();
        let user = db
            .create_user(CreateUserParams {
                username: "admin",
                hashed_password: "hash",
                is_admin: true,
                is_shared: false,
                enabled: true,
                permissions: &perms,
                group_id: None,
                revokes: &[],
                bandwidth_weight: Some(7),
            })
            .await
            .unwrap();

        // Explicit per-user override beats the admin implicit default.
        let resolved = db.get_resolved_bandwidth_weight(user.id).await.unwrap();
        assert_eq!(resolved, 7);
    }

    #[tokio::test]
    async fn test_create_user_admin_with_group_rejected() {
        let pool = create_test_db().await;
        let user_db = UserDb::new(pool.clone());
        let group_db = crate::db::GroupDb::new(pool);

        let group = group_db
            .create_group("Mods", false, &Permissions::new(), 25)
            .await
            .unwrap();
        let perms = Permissions::new();
        let result = user_db
            .create_user(CreateUserParams {
                username: "admin",
                hashed_password: "hash",
                is_admin: true,
                is_shared: false,
                enabled: true,
                permissions: &perms,
                group_id: Some(group.id),
                revokes: &[],
                bandwidth_weight: None,
            })
            .await;

        // Admin XOR group invariant: create_user must reject admin + group_id.
        assert!(
            result.is_err(),
            "create_user should reject admin user with group_id"
        );
    }

    #[tokio::test]
    async fn test_update_user_promotion_clears_group_and_permissions() {
        let pool = create_test_db().await;
        let db = UserDb::new(pool.clone());
        let group_db = crate::db::GroupDb::new(pool.clone());

        let group = group_db
            .create_group(
                "Staff",
                false,
                &Permissions::from(&[Permission::ChatSend]),
                1,
            )
            .await
            .unwrap();

        // Non-admin user with group, grants, and revokes.
        let mut grants = Permissions::new();
        grants.permissions.insert(Permission::NewsList);
        let user = db
            .create_user(CreateUserParams {
                username: "alice",
                hashed_password: "hash",
                is_admin: false,
                is_shared: false,
                enabled: true,
                permissions: &grants,
                group_id: Some(group.id),
                revokes: &[Permission::ChatSend],
                bandwidth_weight: None,
            })
            .await
            .unwrap();
        assert_eq!(user.group_id, Some(group.id));
        let count_perm_rows = async |id: i64| -> i64 {
            let (n,): (i64,) =
                sqlx::query_as("SELECT COUNT(*) FROM user_permissions WHERE user_id = ?")
                    .bind(id)
                    .fetch_one(&pool)
                    .await
                    .unwrap();
            n
        };
        assert!(
            count_perm_rows(user.id).await > 0,
            "fixture should have permission rows"
        );

        // Promote to admin without touching group / permissions in the request.
        // The promotion auto-clean must NULL group_id and wipe all override rows
        // so the schema CHECK passes and no stale state survives.
        let UpdateUserResult::Updated { account, .. } = db
            .update_user(UpdateUserParams {
                username: "alice",
                new_username: None,
                new_password_hash: None,
                is_admin: Some(true),
                enabled: None,
                permissions: None,
                revokes: None,
                remove_group: false,
                group_id: None,
                bandwidth_weight: None,
                inherit_bandwidth_weight: false,
                requester_is_admin: true,
                permission_write_scope: PermissionWriteScope::ReplaceAll,
                requester_bandwidth_max: None,
            })
            .await
            .unwrap()
        else {
            panic!("update should have succeeded");
        };

        assert!(account.is_admin, "user should now be admin");
        assert_eq!(
            account.group_id, None,
            "promotion must clear group_id (CHECK constraint forbids admin + group)"
        );
        // Permission rows are persistent state that's not on the returned
        // account; verify directly via DB to confirm the auto-clean.
        assert_eq!(
            count_perm_rows(user.id).await,
            0,
            "promotion must wipe all permission override rows (admins resolve to all)"
        );
    }

    /// Atomic guard: a non-admin requester must not be able to update an
    /// admin target, even if they slipped past a handler-level pre-check
    /// because the target was promoted between the pre-check and this
    /// UPDATE. Pin the SQL clause directly so a future refactor of the
    /// handler's pre-check can't accidentally remove this race protection.
    #[tokio::test]
    async fn test_update_user_non_admin_cannot_edit_admin_target() {
        let pool = create_test_db().await;
        let db = UserDb::new(pool);

        // Two admins so the per-row CHECK doesn't conflate this with the
        // last-admin guard (we're testing the new clause in isolation).
        db.create_user(CreateUserParams {
            username: "alice",
            hashed_password: "alice_original_hash",
            is_admin: true,
            is_shared: false,
            enabled: true,
            permissions: &Permissions::new(),
            group_id: None,
            revokes: &[],
            bandwidth_weight: None,
        })
        .await
        .unwrap();
        db.create_user(CreateUserParams {
            username: "carol",
            hashed_password: "hash",
            is_admin: true,
            is_shared: false,
            enabled: true,
            permissions: &Permissions::new(),
            group_id: None,
            revokes: &[],
            bandwidth_weight: None,
        })
        .await
        .unwrap();

        // Non-admin requester tries to change alice's password (the
        // race-case attack: handler pre-check saw alice as non-admin,
        // then someone promoted alice, then this UPDATE landed).
        // Non-admin must satisfy the API invariant (OwnedSubset +
        // bandwidth ceiling); empty owned + default ceiling here since
        // the request doesn't touch permissions or group.
        let no_owned: [Permission; 0] = [];
        let result = db
            .update_user(UpdateUserParams {
                username: "alice",
                new_username: None,
                new_password_hash: Some("compromised_hash"),
                is_admin: None,
                enabled: None,
                permissions: None,
                revokes: None,
                remove_group: false,
                group_id: None,
                bandwidth_weight: None,
                inherit_bandwidth_weight: false,
                requester_is_admin: false,
                permission_write_scope: PermissionWriteScope::OwnedSubset(&no_owned),
                requester_bandwidth_max: Some(DEFAULT_BANDWIDTH_WEIGHT),
            })
            .await
            .unwrap();

        assert!(
            matches!(result, UpdateUserResult::Blocked),
            "non-admin requester must not be able to update an admin target"
        );

        // Password must be untouched.
        let alice_after = db.get_user_by_username("alice").await.unwrap().unwrap();
        assert_eq!(
            alice_after.hashed_password, "alice_original_hash",
            "blocked update must not have touched any field"
        );

        // Admin requester editing the same target succeeds — verifying
        // the clause distinguishes by requester, not by target alone.
        let result = db
            .update_user(UpdateUserParams {
                username: "alice",
                new_username: None,
                new_password_hash: Some("admin_set_hash"),
                is_admin: None,
                enabled: None,
                permissions: None,
                revokes: None,
                remove_group: false,
                group_id: None,
                bandwidth_weight: None,
                inherit_bandwidth_weight: false,
                requester_is_admin: true,
                permission_write_scope: PermissionWriteScope::ReplaceAll,
                requester_bandwidth_max: None,
            })
            .await
            .unwrap();
        assert!(matches!(result, UpdateUserResult::Updated { .. }));
        let alice_after = db.get_user_by_username("alice").await.unwrap().unwrap();
        assert_eq!(alice_after.hashed_password, "admin_set_hash");

        // Non-admin editing a non-admin target succeeds — the clause
        // doesn't accidentally block legitimate non-admin → non-admin
        // edits (e.g. a moderator with UserEdit editing a regular user).
        db.create_user(CreateUserParams {
            username: "bob",
            hashed_password: "bob_hash",
            is_admin: false,
            is_shared: false,
            enabled: true,
            permissions: &Permissions::new(),
            group_id: None,
            revokes: &[],
            bandwidth_weight: None,
        })
        .await
        .unwrap();
        let result = db
            .update_user(UpdateUserParams {
                username: "bob",
                new_username: None,
                new_password_hash: Some("mod_set_hash"),
                is_admin: None,
                enabled: None,
                permissions: None,
                revokes: None,
                remove_group: false,
                group_id: None,
                bandwidth_weight: None,
                inherit_bandwidth_weight: false,
                requester_is_admin: false,
                permission_write_scope: PermissionWriteScope::OwnedSubset(&no_owned),
                requester_bandwidth_max: Some(DEFAULT_BANDWIDTH_WEIGHT),
            })
            .await
            .unwrap();
        assert!(matches!(result, UpdateUserResult::Updated { .. }));
    }

    /// Helper: snapshot a user's permission rows as `(permission, override_type)`
    /// pairs, sorted, for deterministic assertions.
    async fn snapshot_perm_rows(pool: &SqlitePool, user_id: i64) -> Vec<(String, String)> {
        let mut rows: Vec<(String, String)> = sqlx::query_as(
            "SELECT permission, override_type FROM user_permissions WHERE user_id = ?",
        )
        .bind(user_id)
        .fetch_all(pool)
        .await
        .unwrap();
        rows.sort();
        rows
    }

    /// Seed a target user with explicit grant + revoke rows for the surgery
    /// tests. Bypasses `create_user`'s "no group" cleanup path (no group
    /// assigned, so grants stay as-is).
    async fn seed_target_user(
        db: &UserDb,
        username: &str,
        grants: &[Permission],
        revokes: &[Permission],
    ) -> i64 {
        let mut grant_perms = Permissions::new();
        for p in grants {
            grant_perms.permissions.insert(*p);
        }
        let user = db
            .create_user(CreateUserParams {
                username,
                hashed_password: "hash",
                is_admin: false,
                is_shared: false,
                enabled: true,
                permissions: &grant_perms,
                group_id: None,
                revokes,
                bandwidth_weight: None,
            })
            .await
            .unwrap();
        user.id
    }

    /// `OwnedSubset` grant surgery must leave rows for permissions
    /// outside `owned` completely untouched. Models the race against a
    /// concurrent admin grant: an admin (or any owner of the unowned
    /// permission) put a row in place; a non-admin's edit must not
    /// clobber it.
    #[tokio::test]
    async fn test_owned_subset_preserves_unowned_grants() {
        let pool = create_test_db().await;
        let db = UserDb::new(pool.clone());

        // Target has two grants. Requester only owns one of them
        // (ChatSend); UserKick is unowned (e.g. just granted by an admin).
        let user_id = seed_target_user(
            &db,
            "target",
            &[Permission::ChatSend, Permission::UserKick],
            &[],
        )
        .await;

        // Non-admin requester re-asserts the grant for ChatSend (their owned
        // permission), implicitly leaving UserKick alone.
        let mut requested_grants = Permissions::new();
        requested_grants.permissions.insert(Permission::ChatSend);
        let owned = [Permission::ChatSend];

        db.update_user(UpdateUserParams {
            username: "target",
            new_username: None,
            new_password_hash: None,
            is_admin: None,
            enabled: None,
            permissions: Some(&requested_grants),
            revokes: None,
            remove_group: false,
            group_id: None,
            bandwidth_weight: None,
            inherit_bandwidth_weight: false,
            requester_is_admin: false,
            permission_write_scope: PermissionWriteScope::OwnedSubset(&owned),
            requester_bandwidth_max: Some(DEFAULT_BANDWIDTH_WEIGHT),
        })
        .await
        .unwrap();

        assert_eq!(
            snapshot_perm_rows(&pool, user_id).await,
            vec![
                ("chat_send".to_string(), "grant".to_string()),
                ("user_kick".to_string(), "grant".to_string()),
            ],
            "unowned grant must survive non-admin surgical update"
        );
    }

    /// Symmetric to the grants test: `OwnedSubset` revoke surgery must
    /// leave revoke rows for unowned permissions in place.
    #[tokio::test]
    async fn test_owned_subset_preserves_unowned_revokes() {
        let pool = create_test_db().await;
        let db = UserDb::new(pool.clone());

        // Target has two revokes. Requester only owns ChatSend.
        let user_id = seed_target_user(
            &db,
            "target",
            &[],
            &[Permission::ChatSend, Permission::UserKick],
        )
        .await;

        // Non-admin requester re-asserts the revoke for ChatSend only.
        let requested_revokes = vec![Permission::ChatSend];
        let owned = [Permission::ChatSend];

        db.update_user(UpdateUserParams {
            username: "target",
            new_username: None,
            new_password_hash: None,
            is_admin: None,
            enabled: None,
            permissions: None,
            revokes: Some(&requested_revokes),
            remove_group: false,
            group_id: None,
            bandwidth_weight: None,
            inherit_bandwidth_weight: false,
            requester_is_admin: false,
            permission_write_scope: PermissionWriteScope::OwnedSubset(&owned),
            requester_bandwidth_max: Some(DEFAULT_BANDWIDTH_WEIGHT),
        })
        .await
        .unwrap();

        assert_eq!(
            snapshot_perm_rows(&pool, user_id).await,
            vec![
                ("chat_send".to_string(), "revoke".to_string()),
                ("user_kick".to_string(), "revoke".to_string()),
            ],
            "unowned revoke must survive non-admin surgical revoke-only update"
        );
    }

    /// Upsert semantics: requesting a grant for a permission the
    /// requester owns AND the target currently has revoked must flip
    /// the row to a grant. Without the `ON CONFLICT` upsert this would
    /// either be a no-op (if we only inserted) or a unique-constraint
    /// failure (if the table key wasn't `(user_id, permission)`).
    #[tokio::test]
    async fn test_owned_grant_upsert_flips_owned_revoke() {
        let pool = create_test_db().await;
        let db = UserDb::new(pool.clone());

        let user_id = seed_target_user(&db, "target", &[], &[Permission::ChatSend]).await;

        let mut requested_grants = Permissions::new();
        requested_grants.permissions.insert(Permission::ChatSend);
        let owned = [Permission::ChatSend];

        db.update_user(UpdateUserParams {
            username: "target",
            new_username: None,
            new_password_hash: None,
            is_admin: None,
            enabled: None,
            permissions: Some(&requested_grants),
            revokes: None,
            remove_group: false,
            group_id: None,
            bandwidth_weight: None,
            inherit_bandwidth_weight: false,
            requester_is_admin: false,
            permission_write_scope: PermissionWriteScope::OwnedSubset(&owned),
            requester_bandwidth_max: Some(DEFAULT_BANDWIDTH_WEIGHT),
        })
        .await
        .unwrap();

        assert_eq!(
            snapshot_perm_rows(&pool, user_id).await,
            vec![("chat_send".to_string(), "grant".to_string())],
            "grant request on owned-but-currently-revoked permission must flip to grant"
        );
    }

    /// Symmetric upsert: requesting a revoke for a permission the
    /// requester owns AND the target currently has granted must flip
    /// the row to a revoke.
    #[tokio::test]
    async fn test_owned_revoke_upsert_flips_owned_grant() {
        let pool = create_test_db().await;
        let db = UserDb::new(pool.clone());

        let user_id = seed_target_user(&db, "target", &[Permission::ChatSend], &[]).await;

        let requested_revokes = vec![Permission::ChatSend];
        let owned = [Permission::ChatSend];

        db.update_user(UpdateUserParams {
            username: "target",
            new_username: None,
            new_password_hash: None,
            is_admin: None,
            enabled: None,
            permissions: None,
            revokes: Some(&requested_revokes),
            remove_group: false,
            group_id: None,
            bandwidth_weight: None,
            inherit_bandwidth_weight: false,
            requester_is_admin: false,
            permission_write_scope: PermissionWriteScope::OwnedSubset(&owned),
            requester_bandwidth_max: Some(DEFAULT_BANDWIDTH_WEIGHT),
        })
        .await
        .unwrap();

        assert_eq!(
            snapshot_perm_rows(&pool, user_id).await,
            vec![("chat_send".to_string(), "revoke".to_string())],
            "revoke request on owned-but-currently-granted permission must flip to revoke"
        );
    }

    /// Per-side separation: a non-admin grant write that re-asserts
    /// an owned permission must NOT touch a revoke row for a
    /// different owned permission. Pins the contract that grant
    /// surgery's per-row DELETE is scoped to `override_type = 'grant'`,
    /// not "any row for this permission".
    ///
    /// Setup: target has `revoke(user_message)`; requester owns both
    /// `chat_send` and `user_message`; request asks to grant only
    /// `chat_send`. The revoke must survive because the grant
    /// surgery's owned-and-not-requested branch deletes only grant
    /// rows (and `user_message` doesn't have one).
    #[tokio::test]
    async fn test_owned_subset_does_not_touch_unrelated_revokes_during_grant_write() {
        let pool = create_test_db().await;
        let db = UserDb::new(pool.clone());

        let user_id = seed_target_user(&db, "target", &[], &[Permission::UserMessage]).await;

        let mut requested_grants = Permissions::new();
        requested_grants.permissions.insert(Permission::ChatSend);
        let owned = [Permission::ChatSend, Permission::UserMessage];

        db.update_user(UpdateUserParams {
            username: "target",
            new_username: None,
            new_password_hash: None,
            is_admin: None,
            enabled: None,
            permissions: Some(&requested_grants),
            // Re-assert the seeded revoke. With `revokes: None` the
            // `OwnedSubset` dispatch would also run revoke surgery
            // with an empty requested set and delete the
            // owned-but-not-requested `user_message` revoke — but
            // that's the revoke-side behavior, not what this test is
            // pinning. Keeping the revoke in `requested_revokes`
            // isolates the assertion to "grant surgery doesn't touch
            // unrelated revokes".
            revokes: Some(&[Permission::UserMessage]),
            remove_group: false,
            group_id: None,
            bandwidth_weight: None,
            inherit_bandwidth_weight: false,
            requester_is_admin: false,
            permission_write_scope: PermissionWriteScope::OwnedSubset(&owned),
            requester_bandwidth_max: Some(DEFAULT_BANDWIDTH_WEIGHT),
        })
        .await
        .unwrap();

        assert_eq!(
            snapshot_perm_rows(&pool, user_id).await,
            vec![
                ("chat_send".to_string(), "grant".to_string()),
                ("user_message".to_string(), "revoke".to_string()),
            ],
            "grant surgery must not touch revoke rows for other permissions"
        );
    }

    /// `ReplaceAll` revoke-only path: an admin-issued revoke-only
    /// update for a permission that currently has a grant row must
    /// succeed — the upsert flips the grant to a revoke. Pre-fix the
    /// `DELETE WHERE override_type = 'revoke'` left the grant row in
    /// place, the subsequent plain `INSERT` collided with the
    /// `(user_id, permission)` primary key, and the whole tx aborted
    /// with a UNIQUE constraint error on what is a perfectly valid
    /// admin request.
    #[tokio::test]
    async fn test_replace_all_revoke_only_flips_existing_grant() {
        let pool = create_test_db().await;
        let db = UserDb::new(pool.clone());

        // Target has a single grant for ChatSend (no group, no revokes).
        let user_id = seed_target_user(&db, "target", &[Permission::ChatSend], &[]).await;

        // Admin (`requester_is_admin: true`, `ReplaceAll`) sends a
        // revoke-only update naming the same permission.
        let requested_revokes = vec![Permission::ChatSend];
        db.update_user(UpdateUserParams {
            username: "target",
            new_username: None,
            new_password_hash: None,
            is_admin: None,
            enabled: None,
            permissions: None,
            revokes: Some(&requested_revokes),
            remove_group: false,
            group_id: None,
            bandwidth_weight: None,
            inherit_bandwidth_weight: false,
            requester_is_admin: true,
            permission_write_scope: PermissionWriteScope::ReplaceAll,
            requester_bandwidth_max: None,
        })
        .await
        .unwrap();

        assert_eq!(
            snapshot_perm_rows(&pool, user_id).await,
            vec![("chat_send".to_string(), "revoke".to_string())],
            "ReplaceAll revoke-only update must flip an existing grant row to a revoke"
        );
    }

    /// API invariant: a non-admin caller must supply both an
    /// `OwnedSubset` scope and a `requester_bandwidth_max`. Violations
    /// return `Err(sqlx::Error::Protocol(...))` rather than silently
    /// bypassing the in-tx ownership/bandwidth checks. Belt-and-suspenders
    /// — the production handler always sets both for non-admins, but
    /// this guard catches a future refactor that drops one wire.
    #[tokio::test]
    async fn test_update_user_non_admin_must_have_owned_subset_and_bandwidth_max() {
        let pool = create_test_db().await;
        let db = UserDb::new(pool);

        db.create_user(CreateUserParams {
            username: "target",
            hashed_password: "hash",
            is_admin: false,
            is_shared: false,
            enabled: true,
            permissions: &Permissions::new(),
            group_id: None,
            revokes: &[],
            bandwidth_weight: None,
        })
        .await
        .unwrap();

        // Missing OwnedSubset (passes ReplaceAll with non-admin).
        let result = db
            .update_user(UpdateUserParams {
                username: "target",
                new_username: None,
                new_password_hash: None,
                is_admin: None,
                enabled: None,
                permissions: None,
                revokes: None,
                remove_group: false,
                group_id: None,
                bandwidth_weight: None,
                inherit_bandwidth_weight: false,
                requester_is_admin: false,
                permission_write_scope: PermissionWriteScope::ReplaceAll,
                requester_bandwidth_max: Some(DEFAULT_BANDWIDTH_WEIGHT),
            })
            .await;
        assert!(matches!(result, Err(sqlx::Error::Protocol(_))));

        // Missing bandwidth ceiling (OwnedSubset but None).
        let no_owned: [Permission; 0] = [];
        let result = db
            .update_user(UpdateUserParams {
                username: "target",
                new_username: None,
                new_password_hash: None,
                is_admin: None,
                enabled: None,
                permissions: None,
                revokes: None,
                remove_group: false,
                group_id: None,
                bandwidth_weight: None,
                inherit_bandwidth_weight: false,
                requester_is_admin: false,
                permission_write_scope: PermissionWriteScope::OwnedSubset(&no_owned),
                requester_bandwidth_max: None,
            })
            .await;
        assert!(matches!(result, Err(sqlx::Error::Protocol(_))));
    }

    /// In-tx race protection: a non-admin assigning a target to a
    /// group whose perms include one the requester doesn't own must be
    /// rejected, even if the handler's pre-tx check would have passed
    /// (admin added the unowned perm after the snapshot). Re-fetched
    /// inside the tx so the check sees the row state the
    /// `SQL_UPDATE_USER_GROUP` write will land on.
    #[tokio::test]
    async fn test_update_user_non_admin_group_assign_blocked_on_unowned_perm() {
        let pool = create_test_db().await;
        let db = UserDb::new(pool.clone());
        let group_db = crate::db::GroupDb::new(pool);

        // Group with two perms.
        let group = group_db
            .create_group(
                "Staff",
                false,
                &Permissions::from(&[Permission::ChatSend, Permission::UserKick]),
                1,
            )
            .await
            .unwrap();

        db.create_user(CreateUserParams {
            username: "target",
            hashed_password: "hash",
            is_admin: false,
            is_shared: false,
            enabled: true,
            permissions: &Permissions::new(),
            group_id: None,
            revokes: &[],
            bandwidth_weight: None,
        })
        .await
        .unwrap();

        // Requester owns only ChatSend; group requires UserKick too.
        let owned = [Permission::ChatSend];
        let result = db
            .update_user(UpdateUserParams {
                username: "target",
                new_username: None,
                new_password_hash: None,
                is_admin: None,
                enabled: None,
                permissions: None,
                revokes: None,
                remove_group: false,
                group_id: Some(group.id),
                bandwidth_weight: None,
                inherit_bandwidth_weight: false,
                requester_is_admin: false,
                permission_write_scope: PermissionWriteScope::OwnedSubset(&owned),
                requester_bandwidth_max: Some(DEFAULT_BANDWIDTH_WEIGHT),
            })
            .await
            .unwrap();
        assert!(matches!(result, UpdateUserResult::BlockedForGroupAuth));

        // Target's group_id must still be None — tx rolled back.
        let target_after = db.get_user_by_username("target").await.unwrap().unwrap();
        assert_eq!(target_after.group_id, None);
    }

    /// In-tx race protection: bandwidth ceiling re-checked against the
    /// group's current weight. An admin's mid-flight weight bump can't
    /// slip a target into an over-ceiling group.
    #[tokio::test]
    async fn test_update_user_non_admin_group_assign_blocked_on_bandwidth_ceiling() {
        let pool = create_test_db().await;
        let db = UserDb::new(pool.clone());
        let group_db = crate::db::GroupDb::new(pool);

        // Group's weight is 100; requester's ceiling is 50.
        let group = group_db
            .create_group("Staff", false, &Permissions::new(), 100)
            .await
            .unwrap();

        db.create_user(CreateUserParams {
            username: "target",
            hashed_password: "hash",
            is_admin: false,
            is_shared: false,
            enabled: true,
            permissions: &Permissions::new(),
            group_id: None,
            revokes: &[],
            bandwidth_weight: None,
        })
        .await
        .unwrap();

        let no_owned: [Permission; 0] = [];
        let result = db
            .update_user(UpdateUserParams {
                username: "target",
                new_username: None,
                new_password_hash: None,
                is_admin: None,
                enabled: None,
                permissions: None,
                revokes: None,
                remove_group: false,
                group_id: Some(group.id),
                bandwidth_weight: None,
                inherit_bandwidth_weight: false,
                requester_is_admin: false,
                permission_write_scope: PermissionWriteScope::OwnedSubset(&no_owned),
                requester_bandwidth_max: Some(50),
            })
            .await
            .unwrap();
        assert!(matches!(result, UpdateUserResult::BlockedForGroupAuth));

        let target_after = db.get_user_by_username("target").await.unwrap().unwrap();
        assert_eq!(target_after.group_id, None);
    }

    /// In-tx race protection: removing a target from a group with
    /// perms the requester doesn't own is rejected.
    #[tokio::test]
    async fn test_update_user_non_admin_group_remove_blocked_on_unowned_perm() {
        let pool = create_test_db().await;
        let db = UserDb::new(pool.clone());
        let group_db = crate::db::GroupDb::new(pool);

        let group = group_db
            .create_group(
                "Staff",
                false,
                &Permissions::from(&[Permission::UserKick]),
                1,
            )
            .await
            .unwrap();

        db.create_user(CreateUserParams {
            username: "target",
            hashed_password: "hash",
            is_admin: false,
            is_shared: false,
            enabled: true,
            permissions: &Permissions::new(),
            group_id: Some(group.id),
            revokes: &[],
            bandwidth_weight: None,
        })
        .await
        .unwrap();

        // Requester owns ChatSend, NOT UserKick (which the group has).
        let owned = [Permission::ChatSend];
        let result = db
            .update_user(UpdateUserParams {
                username: "target",
                new_username: None,
                new_password_hash: None,
                is_admin: None,
                enabled: None,
                permissions: None,
                revokes: None,
                remove_group: true,
                group_id: None,
                bandwidth_weight: None,
                inherit_bandwidth_weight: false,
                requester_is_admin: false,
                permission_write_scope: PermissionWriteScope::OwnedSubset(&owned),
                requester_bandwidth_max: Some(DEFAULT_BANDWIDTH_WEIGHT),
            })
            .await
            .unwrap();
        assert!(matches!(result, UpdateUserResult::BlockedForGroupAuth));

        // Target still in the group — tx rolled back.
        let target_after = db.get_user_by_username("target").await.unwrap().unwrap();
        assert_eq!(target_after.group_id, Some(group.id));
    }

    /// In-tx race protection: `is_shared` mismatch re-checked against
    /// the group's current row. Covers the empty-group → flip →
    /// first-incompatible-member-assignment race the GroupUpdate
    /// pre-check doesn't cover (it only blocks the flip when the
    /// group already has members).
    #[tokio::test]
    async fn test_update_user_non_admin_group_assign_blocked_on_shared_mismatch() {
        let pool = create_test_db().await;
        let db = UserDb::new(pool.clone());
        let group_db = crate::db::GroupDb::new(pool);

        // Shared group, but target is a non-shared user.
        let group = group_db
            .create_group("Shared", true, &Permissions::new(), 1)
            .await
            .unwrap();

        db.create_user(CreateUserParams {
            username: "target",
            hashed_password: "hash",
            is_admin: false,
            is_shared: false,
            enabled: true,
            permissions: &Permissions::new(),
            group_id: None,
            revokes: &[],
            bandwidth_weight: None,
        })
        .await
        .unwrap();

        let no_owned: [Permission; 0] = [];
        let result = db
            .update_user(UpdateUserParams {
                username: "target",
                new_username: None,
                new_password_hash: None,
                is_admin: None,
                enabled: None,
                permissions: None,
                revokes: None,
                remove_group: false,
                group_id: Some(group.id),
                bandwidth_weight: None,
                inherit_bandwidth_weight: false,
                requester_is_admin: false,
                permission_write_scope: PermissionWriteScope::OwnedSubset(&no_owned),
                requester_bandwidth_max: Some(DEFAULT_BANDWIDTH_WEIGHT),
            })
            .await
            .unwrap();
        assert!(matches!(result, UpdateUserResult::BlockedForGroupAuth));
    }

    /// Companion: admin callers bypass all in-tx group-auth checks.
    /// Same setup as the unowned-perm test above; admin succeeds where
    /// non-admin was blocked.
    #[tokio::test]
    async fn test_update_user_admin_bypasses_in_tx_group_auth() {
        let pool = create_test_db().await;
        let db = UserDb::new(pool.clone());
        let group_db = crate::db::GroupDb::new(pool);

        let group = group_db
            .create_group(
                "Staff",
                false,
                &Permissions::from(&[Permission::ChatSend, Permission::UserKick]),
                100,
            )
            .await
            .unwrap();

        db.create_user(CreateUserParams {
            username: "target",
            hashed_password: "hash",
            is_admin: false,
            is_shared: false,
            enabled: true,
            permissions: &Permissions::new(),
            group_id: None,
            revokes: &[],
            bandwidth_weight: None,
        })
        .await
        .unwrap();

        let result = db
            .update_user(UpdateUserParams {
                username: "target",
                new_username: None,
                new_password_hash: None,
                is_admin: None,
                enabled: None,
                permissions: None,
                revokes: None,
                remove_group: false,
                group_id: Some(group.id),
                bandwidth_weight: None,
                inherit_bandwidth_weight: false,
                requester_is_admin: true,
                permission_write_scope: PermissionWriteScope::ReplaceAll,
                requester_bandwidth_max: None,
            })
            .await
            .unwrap();
        assert!(matches!(result, UpdateUserResult::Updated { .. }));

        let target_after = db.get_user_by_username("target").await.unwrap().unwrap();
        assert_eq!(target_after.group_id, Some(group.id));
    }

    /// Schema-level pin: the CHECK constraint must reject any user row with
    /// both `is_admin = 1` and `is_shared = 1`. The handler layer also blocks
    /// this combination (UserCreate rejects it, UserUpdate rejects promoting
    /// a shared user to admin), but the CHECK is the storage-layer safety net
    /// that catches any path that bypasses the handlers.
    #[tokio::test]
    async fn test_schema_rejects_admin_and_shared() {
        let pool = create_test_db().await;
        let db = UserDb::new(pool);

        // Going through create_user would route the combination through the
        // DB layer's INSERT. The CHECK constraint fires inside SQLite and
        // surfaces as an sqlx error — create_user propagates it.
        let result = db
            .create_user(CreateUserParams {
                username: "evil",
                hashed_password: "hash",
                is_admin: true,
                is_shared: true,
                enabled: true,
                permissions: &Permissions::new(),
                group_id: None,
                revokes: &[],
                bandwidth_weight: None,
            })
            .await;
        assert!(
            result.is_err(),
            "admin+shared insert must be rejected by the schema CHECK"
        );
    }

    #[tokio::test]
    async fn test_get_resolved_bandwidth_weight_unknown_user_returns_default() {
        let pool = create_test_db().await;
        let db = UserDb::new(pool);

        // No user with this id exists; resolver returns default rather than erroring.
        let resolved = db.get_resolved_bandwidth_weight(99_999).await.unwrap();
        assert_eq!(resolved, DEFAULT_BANDWIDTH_WEIGHT);
    }

    #[tokio::test]
    async fn test_get_inherited_bandwidth_weight_ignores_user_override() {
        let pool = create_test_db().await;
        let user_db = UserDb::new(pool.clone());
        let group_db = crate::db::GroupDb::new(pool);

        let group = group_db
            .create_group("Mods", false, &Permissions::new(), 25)
            .await
            .unwrap();
        let perms = Permissions::new();
        let user = user_db
            .create_user(create_user_params(
                "alice",
                "hash",
                &perms,
                Some(group.id),
                Some(7),
            ))
            .await
            .unwrap();

        // User override is 7, but inherited weight ignores it → group's 25.
        // Pass the user's current group_id as the "proposed" group: the
        // caller's contract is to ask "what would the inherited weight be
        // if the user were in group X" — when no group change is requested,
        // X is the current group_id.
        let inherited = user_db
            .get_inherited_bandwidth_weight(user.id, Some(group.id))
            .await
            .unwrap();
        assert_eq!(inherited, 25);
    }

    #[tokio::test]
    async fn test_get_inherited_bandwidth_weight_admin() {
        let pool = create_test_db().await;
        let db = UserDb::new(pool);

        let perms = Permissions::new();
        let user = db
            .create_user(CreateUserParams {
                username: "admin",
                hashed_password: "hash",
                is_admin: true,
                is_shared: false,
                enabled: true,
                permissions: &perms,
                group_id: None,
                revokes: &[],
                bandwidth_weight: None,
            })
            .await
            .unwrap();

        // Admin → admin default, regardless of override or proposed group.
        let inherited = db
            .get_inherited_bandwidth_weight(user.id, None)
            .await
            .unwrap();
        assert_eq!(inherited, DEFAULT_ADMIN_BANDWIDTH_WEIGHT);
    }

    #[tokio::test]
    async fn test_get_inherited_bandwidth_weight_no_group_no_override() {
        let pool = create_test_db().await;
        let db = UserDb::new(pool);

        let perms = Permissions::new();
        let user = db
            .create_user(create_user_params("alice", "hash", &perms, None, None))
            .await
            .unwrap();

        // No proposed group, no admin → system default.
        let inherited = db
            .get_inherited_bandwidth_weight(user.id, None)
            .await
            .unwrap();
        assert_eq!(inherited, DEFAULT_BANDWIDTH_WEIGHT);
    }

    #[tokio::test]
    async fn test_get_inherited_bandwidth_weight_unknown_user_returns_default() {
        let pool = create_test_db().await;
        let db = UserDb::new(pool);

        let inherited = db
            .get_inherited_bandwidth_weight(99_999, None)
            .await
            .unwrap();
        assert_eq!(inherited, DEFAULT_BANDWIDTH_WEIGHT);
    }

    /// Regression: `proposed_group_id` must drive the join — using the
    /// user's current group_id from the DB would defeat the purpose of the
    /// parameter (which exists so the inherit-delegation check can ask
    /// "what WILL the inherited weight be after this pending group change?").
    #[tokio::test]
    async fn test_get_inherited_bandwidth_weight_uses_proposed_group_not_current() {
        let pool = create_test_db().await;
        let user_db = UserDb::new(pool.clone());
        let group_db = crate::db::GroupDb::new(pool);

        let high_group = group_db
            .create_group("High", false, &Permissions::new(), 100)
            .await
            .unwrap();
        let low_group = group_db
            .create_group("Low", false, &Permissions::new(), 5)
            .await
            .unwrap();
        let perms = Permissions::new();
        // User currently in high-weight group.
        let user = user_db
            .create_user(create_user_params(
                "alice",
                "hash",
                &perms,
                Some(high_group.id),
                None,
            ))
            .await
            .unwrap();

        // Ask: "what would alice's inherited weight be in the low group?"
        let in_low = user_db
            .get_inherited_bandwidth_weight(user.id, Some(low_group.id))
            .await
            .unwrap();
        assert_eq!(
            in_low, 5,
            "proposed group must drive resolution, ignoring the user's current group_id"
        );

        // And "in no group" → system default, not high group's 100.
        let in_none = user_db
            .get_inherited_bandwidth_weight(user.id, None)
            .await
            .unwrap();
        assert_eq!(in_none, DEFAULT_BANDWIDTH_WEIGHT);
    }

    // In-transaction contract: update_user returns the post-update
    // effective weight resolved inside the same transaction as the write.
    // These tests pin the invariant that the value reported by
    // `UpdateUserResult::Updated.resolved_bandwidth_weight` matches what
    // `get_resolved_bandwidth_weight` would return immediately after the
    // commit — no torn states, no follow-up DB read needed for callers.

    /// Setting a new override → resolved equals the override.
    #[tokio::test]
    async fn test_update_user_returns_resolved_for_override_set() {
        let pool = create_test_db().await;
        let db = UserDb::new(pool);

        let perms = Permissions::new();
        db.create_user(create_user_params("alice", "hash", &perms, None, None))
            .await
            .unwrap();

        let result = db
            .update_user(UpdateUserParams {
                username: "alice",
                new_username: None,
                new_password_hash: None,
                is_admin: None,
                enabled: None,
                permissions: None,
                revokes: None,
                remove_group: false,
                group_id: None,
                bandwidth_weight: Some(77),
                inherit_bandwidth_weight: false,
                requester_is_admin: true,
                permission_write_scope: PermissionWriteScope::ReplaceAll,
                requester_bandwidth_max: None,
            })
            .await
            .unwrap();

        match result {
            UpdateUserResult::Updated {
                account,
                resolved_bandwidth_weight,
                ..
            } => {
                assert_eq!(resolved_bandwidth_weight, 77);
                assert_eq!(account.bandwidth_weight, Some(77), "stored override");
            }
            UpdateUserResult::Blocked | UpdateUserResult::BlockedForGroupAuth => {
                panic!("update should have succeeded")
            }
        }
    }

    /// Clearing an override on a user with a group → resolved equals the
    /// group's weight (override removed, group inheritance takes over).
    #[tokio::test]
    async fn test_update_user_returns_resolved_for_inherit_with_group() {
        let pool = create_test_db().await;
        let user_db = UserDb::new(pool.clone());
        let group_db = crate::db::GroupDb::new(pool);

        let group = group_db
            .create_group("Tier1", false, &Permissions::new(), 30)
            .await
            .unwrap();
        let perms = Permissions::new();
        user_db
            .create_user(create_user_params(
                "alice",
                "hash",
                &perms,
                Some(group.id),
                Some(99),
            ))
            .await
            .unwrap();

        let result = user_db
            .update_user(UpdateUserParams {
                username: "alice",
                new_username: None,
                new_password_hash: None,
                is_admin: None,
                enabled: None,
                permissions: None,
                revokes: None,
                remove_group: false,
                group_id: None,
                bandwidth_weight: None,
                inherit_bandwidth_weight: true,
                requester_is_admin: true,
                permission_write_scope: PermissionWriteScope::ReplaceAll,
                requester_bandwidth_max: None,
            })
            .await
            .unwrap();

        match result {
            UpdateUserResult::Updated {
                account,
                resolved_bandwidth_weight,
                ..
            } => {
                assert_eq!(
                    resolved_bandwidth_weight, 30,
                    "inherit should resolve to group weight"
                );
                assert!(
                    account.bandwidth_weight.is_none(),
                    "inherit clears the stored override"
                );
            }
            UpdateUserResult::Blocked | UpdateUserResult::BlockedForGroupAuth => {
                panic!("update should have succeeded")
            }
        }
    }

    /// Clearing an override on a non-admin user with no group → resolved
    /// falls back to the system default.
    #[tokio::test]
    async fn test_update_user_returns_resolved_for_inherit_no_group() {
        let pool = create_test_db().await;
        let db = UserDb::new(pool);

        let perms = Permissions::new();
        db.create_user(create_user_params("alice", "hash", &perms, None, Some(55)))
            .await
            .unwrap();

        let result = db
            .update_user(UpdateUserParams {
                username: "alice",
                new_username: None,
                new_password_hash: None,
                is_admin: None,
                enabled: None,
                permissions: None,
                revokes: None,
                remove_group: false,
                group_id: None,
                bandwidth_weight: None,
                inherit_bandwidth_weight: true,
                requester_is_admin: true,
                permission_write_scope: PermissionWriteScope::ReplaceAll,
                requester_bandwidth_max: None,
            })
            .await
            .unwrap();

        match result {
            UpdateUserResult::Updated {
                account,
                resolved_bandwidth_weight,
                ..
            } => {
                assert_eq!(resolved_bandwidth_weight, DEFAULT_BANDWIDTH_WEIGHT);
                assert!(account.bandwidth_weight.is_none(), "override cleared");
            }
            UpdateUserResult::Blocked | UpdateUserResult::BlockedForGroupAuth => {
                panic!("update should have succeeded")
            }
        }
    }

    /// Promoting a non-admin to admin → resolved jumps to the admin
    /// default, regardless of any prior override or group. The promotion
    /// also auto-clears group_id (admin XOR group invariant), so this
    /// covers the cross-side-effect path.
    #[tokio::test]
    async fn test_update_user_returns_resolved_for_admin_promotion() {
        let pool = create_test_db().await;
        let user_db = UserDb::new(pool.clone());
        let group_db = crate::db::GroupDb::new(pool);

        let group = group_db
            .create_group("Tier1", false, &Permissions::new(), 30)
            .await
            .unwrap();
        let perms = Permissions::new();
        user_db
            .create_user(create_user_params(
                "alice",
                "hash",
                &perms,
                Some(group.id),
                None,
            ))
            .await
            .unwrap();

        let result = user_db
            .update_user(UpdateUserParams {
                username: "alice",
                new_username: None,
                new_password_hash: None,
                is_admin: Some(true),
                enabled: None,
                permissions: None,
                revokes: None,
                remove_group: false,
                group_id: None,
                bandwidth_weight: None,
                inherit_bandwidth_weight: false,
                requester_is_admin: true,
                permission_write_scope: PermissionWriteScope::ReplaceAll,
                requester_bandwidth_max: None,
            })
            .await
            .unwrap();

        match result {
            UpdateUserResult::Updated {
                account,
                resolved_bandwidth_weight,
                ..
            } => {
                assert_eq!(
                    resolved_bandwidth_weight, DEFAULT_ADMIN_BANDWIDTH_WEIGHT,
                    "promotion should resolve to admin default"
                );
                assert!(account.is_admin, "promotion should flip is_admin");
                assert!(
                    account.group_id.is_none(),
                    "promotion auto-clean should NULL group_id"
                );
            }
            UpdateUserResult::Blocked | UpdateUserResult::BlockedForGroupAuth => {
                panic!("update should have succeeded")
            }
        }
    }

    /// A request that doesn't touch any bandwidth-relevant field still
    /// returns the user's current effective weight — the resolution runs
    /// unconditionally inside the tx so callers don't have to branch on
    /// "did the bandwidth-relevant fields change?" before consuming the
    /// value.
    #[tokio::test]
    async fn test_update_user_returns_resolved_for_unchanged_bandwidth() {
        let pool = create_test_db().await;
        let db = UserDb::new(pool);

        let perms = Permissions::new();
        db.create_user(create_user_params("alice", "hash", &perms, None, Some(88)))
            .await
            .unwrap();

        // Username change only — bandwidth_weight untouched.
        let result = db
            .update_user(UpdateUserParams {
                username: "alice",
                new_username: Some("alice2"),
                new_password_hash: None,
                is_admin: None,
                enabled: None,
                permissions: None,
                revokes: None,
                remove_group: false,
                group_id: None,
                bandwidth_weight: None,
                inherit_bandwidth_weight: false,
                requester_is_admin: true,
                permission_write_scope: PermissionWriteScope::ReplaceAll,
                requester_bandwidth_max: None,
            })
            .await
            .unwrap();

        match result {
            UpdateUserResult::Updated {
                account,
                resolved_bandwidth_weight,
                ..
            } => {
                assert_eq!(
                    resolved_bandwidth_weight, 88,
                    "unchanged bandwidth → resolved still equals the existing override"
                );
                assert_eq!(account.username, "alice2", "rename should be reflected");
                assert_eq!(account.bandwidth_weight, Some(88), "override preserved");
            }
            UpdateUserResult::Blocked | UpdateUserResult::BlockedForGroupAuth => {
                panic!("update should have succeeded")
            }
        }
    }

    /// The post-write in-tx resolution must agree with what
    /// `get_resolved_bandwidth_weight` would return immediately after
    /// commit. Pins the contract that the two resolution paths are
    /// equivalent — no future drift can split them.
    #[tokio::test]
    async fn test_update_user_resolved_matches_standalone_resolver() {
        let pool = create_test_db().await;
        let user_db = UserDb::new(pool.clone());
        let group_db = crate::db::GroupDb::new(pool);

        let group = group_db
            .create_group("Tier1", false, &Permissions::new(), 30)
            .await
            .unwrap();
        let perms = Permissions::new();
        let user = user_db
            .create_user(create_user_params(
                "alice",
                "hash",
                &perms,
                Some(group.id),
                None,
            ))
            .await
            .unwrap();

        let result = user_db
            .update_user(UpdateUserParams {
                username: "alice",
                new_username: None,
                new_password_hash: None,
                is_admin: None,
                enabled: None,
                permissions: None,
                revokes: None,
                remove_group: false,
                group_id: None,
                bandwidth_weight: Some(150),
                inherit_bandwidth_weight: false,
                requester_is_admin: true,
                permission_write_scope: PermissionWriteScope::ReplaceAll,
                requester_bandwidth_max: None,
            })
            .await
            .unwrap();

        let in_tx = match result {
            UpdateUserResult::Updated {
                account: _,
                resolved_bandwidth_weight,
                ..
            } => resolved_bandwidth_weight,
            UpdateUserResult::Blocked | UpdateUserResult::BlockedForGroupAuth => {
                panic!("update should have succeeded")
            }
        };
        let standalone = user_db
            .get_resolved_bandwidth_weight(user.id)
            .await
            .unwrap();
        assert_eq!(
            in_tx, standalone,
            "in-tx resolution and standalone resolver must agree"
        );
    }

    /// Trust-anchor for the `account` field of `UpdateUserResult::Updated`:
    /// the assembled `UserAccount` (built from in-scope values) must
    /// equal what a fresh `get_user_by_id` returns for the same row.
    /// If the assembly ever diverges from the actual SQL writes, this
    /// test catches it — and callers can stop double-fetching to verify.
    #[tokio::test]
    async fn test_update_user_returned_account_matches_db_row() {
        let pool = create_test_db().await;
        let user_db = UserDb::new(pool.clone());
        let group_db = crate::db::GroupDb::new(pool);

        let group = group_db
            .create_group("Tier1", false, &Permissions::new(), 30)
            .await
            .unwrap();
        let perms = Permissions::new();
        let user = user_db
            .create_user(create_user_params(
                "alice",
                "hash",
                &perms,
                Some(group.id),
                Some(99),
            ))
            .await
            .unwrap();

        // Touch every assembly-relevant field at once: rename, password,
        // enabled, override, remove_group. Pins the assembly contract
        // across the whole shape, not just one field at a time.
        let result = user_db
            .update_user(UpdateUserParams {
                username: "alice",
                new_username: Some("alice2"),
                new_password_hash: Some("newhash"),
                is_admin: None,
                enabled: Some(false),
                permissions: None,
                revokes: None,
                remove_group: true,
                group_id: None,
                bandwidth_weight: Some(42),
                inherit_bandwidth_weight: false,
                requester_is_admin: true,
                permission_write_scope: PermissionWriteScope::ReplaceAll,
                requester_bandwidth_max: None,
            })
            .await
            .unwrap();

        let returned_account = match result {
            UpdateUserResult::Updated { account, .. } => account,
            UpdateUserResult::Blocked | UpdateUserResult::BlockedForGroupAuth => {
                panic!("update should have succeeded")
            }
        };
        let row = user_db
            .get_user_by_id(user.id)
            .await
            .unwrap()
            .expect("user should still exist after update");

        assert_eq!(returned_account.id, row.id);
        assert_eq!(returned_account.username, row.username);
        assert_eq!(returned_account.hashed_password, row.hashed_password);
        assert_eq!(returned_account.is_admin, row.is_admin);
        assert_eq!(returned_account.is_shared, row.is_shared);
        assert_eq!(returned_account.enabled, row.enabled);
        assert_eq!(returned_account.created_at, row.created_at);
        assert_eq!(returned_account.group_id, row.group_id);
        assert_eq!(returned_account.bandwidth_weight, row.bandwidth_weight);
    }
}
