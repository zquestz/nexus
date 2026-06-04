//! SQL query constants for the database layer.

pub const SQL_GET_CONFIG: &str = "SELECT value FROM config WHERE key = ?";

/// Only updates existing keys; does not insert new ones.
pub const SQL_SET_CONFIG: &str = "UPDATE config SET value = ? WHERE key = ?";

pub const SQL_GET_ALL_CONFIG: &str = "SELECT key, value FROM config";

/// Single-query bundle (rather than three separate `SQL_GET_CONFIG` calls) —
/// small efficiency win on the tracker task's per-refresh hot path. Missing
/// keys are simply absent from the result set.
pub const SQL_GET_TRACKER_CONFIG_FIELDS: &str = "SELECT key, value FROM config WHERE key IN ('server_name', 'server_description', 'public_address')";

/// Reserved username; cannot be used for other accounts.
pub const GUEST_USERNAME: &str = "guest";

/// Excludes the guest account so the first non-guest user becomes admin.
/// Matches on the folded `username_lower` (the literal is already folded).
pub const SQL_COUNT_NON_GUEST_USERS: &str =
    "SELECT COUNT(*) FROM users WHERE username_lower != 'guest'";

/// Case-insensitive matching via the folded `username_lower` column (written
/// as `fold_name(username)`); the original case is preserved in `username`.
/// Bind the folded form (`fold_name(input)`), not the raw input.
pub const SQL_SELECT_USER_BY_USERNAME: &str = "SELECT id, username, password_hash, is_admin, is_shared, enabled, created_at, group_id, bandwidth_weight FROM users WHERE username_lower = ?";

pub const SQL_GET_GUEST_ENABLED: &str = "SELECT enabled FROM users WHERE username_lower = 'guest'";

/// Results are sorted alphabetically by the folded `username_lower`
/// (case-insensitive); the original-case `username` is returned for display.
pub const SQL_SELECT_ALL_USERS: &str = "SELECT id, username, password_hash, is_admin, is_shared, enabled, created_at, group_id, bandwidth_weight FROM users ORDER BY username_lower";

/// Case-insensitive via the folded `username_lower` column; bind
/// `fold_name(input)`.
pub const SQL_CHECK_USERNAME_EXISTS: &str = "SELECT COUNT(*) FROM users WHERE username_lower = ?";

/// `override_type` is either `'grant'` or `'revoke'`.
pub const SQL_SELECT_PERMISSIONS: &str =
    "SELECT permission, override_type FROM user_permissions WHERE user_id = ?";

pub const SQL_SELECT_GRANT_PERMISSIONS: &str =
    "SELECT permission FROM user_permissions WHERE user_id = ? AND override_type = 'grant'";

pub const SQL_SELECT_REVOKE_PERMISSIONS: &str =
    "SELECT permission FROM user_permissions WHERE user_id = ? AND override_type = 'revoke'";

pub const SQL_DELETE_REVOKE_PERMISSIONS: &str =
    "DELETE FROM user_permissions WHERE user_id = ? AND override_type = 'revoke'";

pub const SQL_DELETE_GRANT_PERMISSION: &str =
    "DELETE FROM user_permissions WHERE user_id = ? AND permission = ? AND override_type = 'grant'";

/// Lightweight query used by `get_user_permissions()` to check if group-based
/// permission resolution is needed.
pub const SQL_SELECT_USER_GROUP_ID: &str = "SELECT group_id FROM users WHERE id = ?";

/// Used by `update_user`'s in-tx group-auth re-read for non-admin callers.
pub const SQL_SELECT_USER_GROUP_AND_SHARED: &str =
    "SELECT group_id, is_shared FROM users WHERE id = ?";

/// Used by `update_user`'s in-tx new-group re-check (clamp the weight at the
/// call site).
pub const SQL_SELECT_GROUP_SHARED_AND_BANDWIDTH: &str =
    "SELECT is_shared, bandwidth_weight FROM groups WHERE id = ?";

/// User's raw weight (NULL = inherit), the group's weight (NULL if no group),
/// and is_admin (admins resolve to `DEFAULT_ADMIN_BANDWIDTH_WEIGHT` when no
/// per-user override is set) — in one row, avoiding two round-trips.
pub const SQL_SELECT_USER_AND_GROUP_BANDWIDTH_WEIGHT: &str = "SELECT u.bandwidth_weight, g.bandwidth_weight, u.is_admin FROM users u LEFT JOIN groups g ON u.group_id = g.id WHERE u.id = ?";

pub const SQL_SELECT_GROUP_BANDWIDTH_INHERITOR_USER_IDS: &str =
    "SELECT id FROM users WHERE group_id = ? AND bandwidth_weight IS NULL";

/// Joins on a *parameter* group_id rather than the user's current one — used
/// by `get_inherited_bandwidth_weight` to ask "what will the user's inherited
/// weight be if they're in group X?" for the inherit-delegation check on a
/// request that also changes the target's group.
pub const SQL_SELECT_USER_ADMIN_AND_PROPOSED_GROUP_WEIGHT: &str = "SELECT u.is_admin, g.bandwidth_weight FROM users u LEFT JOIN groups g ON g.id = ? WHERE u.id = ?";

/// Used when replacing permissions or promoting user to admin.
pub const SQL_DELETE_PERMISSIONS: &str = "DELETE FROM user_permissions WHERE user_id = ?";

pub const SQL_INSERT_PERMISSION: &str =
    "INSERT INTO user_permissions (user_id, permission) VALUES (?, ?)";

pub const SQL_INSERT_PERMISSION_OVERRIDE: &str =
    "INSERT INTO user_permissions (user_id, permission, override_type) VALUES (?, ?, ?)";

/// `(user_id, permission)` is the PK so a row is either grant OR revoke — the
/// upsert flips an existing revoke to a grant in one operation.
pub const SQL_UPSERT_GRANT_PERMISSION: &str = "INSERT INTO user_permissions (user_id, permission, override_type) VALUES (?, ?, 'grant') \
     ON CONFLICT(user_id, permission) DO UPDATE SET override_type = 'grant'";

/// Symmetric to `SQL_UPSERT_GRANT_PERMISSION` with grant ↔ revoke.
pub const SQL_UPSERT_REVOKE_PERMISSION: &str = "INSERT INTO user_permissions (user_id, permission, override_type) VALUES (?, ?, 'revoke') \
     ON CONFLICT(user_id, permission) DO UPDATE SET override_type = 'revoke'";

/// Deletes a single revoke row, leaving any grant row for the same permission
/// alone. Symmetric to `SQL_DELETE_GRANT_PERMISSION`.
pub const SQL_DELETE_REVOKE_PERMISSION: &str = "DELETE FROM user_permissions WHERE user_id = ? AND permission = ? AND override_type = 'revoke'";

/// `username_lower` must be bound as `fold_name(username)` — the folded
/// uniqueness/lookup key (its `UNIQUE` index is case-insensitivity's authority).
pub const SQL_INSERT_USER: &str = "INSERT INTO users (username, username_lower, password_hash, is_admin, is_shared, enabled, created_at, group_id, bandwidth_weight) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)";

/// `is_shared` is not updated — it is immutable once set at creation.
///
/// **Atomic protection (TOCTOU prevention):** all checks happen in a single
/// statement so concurrent updates can't leave zero enabled admins or zero
/// admins, and a concurrent promotion can't let a non-admin's in-flight edit
/// land on a now-admin row. Returns 0 rows affected if blocked.
pub const SQL_UPDATE_USER: &str = "UPDATE users
    SET username = ?, username_lower = ?, password_hash = ?, is_admin = ?, enabled = ?, bandwidth_weight = ?
    WHERE id = ?
    AND (
        -- Enabled protection: allow enabling, allow non-admin disable, allow if multiple enabled admins
        ? = 1
        OR is_admin = 0
        OR (SELECT COUNT(*) FROM users WHERE is_admin = 1 AND enabled = 1) > 1
    )
    AND (
        -- is_admin protection: allow promoting, allow if currently non-admin, allow if multiple admins
        ? = 1
        OR is_admin = 0
        OR (SELECT COUNT(*) FROM users WHERE is_admin = 1) > 1
    )
    AND (
        -- Non-admin requester cannot edit an admin target. Atomic with
        -- the UPDATE so a concurrent promotion between the handler's
        -- pre-check and this write can't let a non-admin edit a
        -- now-admin row.
        ? = 1
        OR is_admin = 0
    )";

pub const SQL_UPDATE_USER_GROUP: &str = "UPDATE users SET group_id = ? WHERE id = ?";

/// Atomic protection: prevents deleting the last admin, and prevents a
/// non-admin requester from deleting an admin target (closing the race where a
/// target is promoted between the handler's pre-check and this DELETE).
/// Returns 0 rows affected if blocked. FK constraints cascade-delete the
/// user's permissions.
pub const SQL_DELETE_USER_ATOMIC: &str = "DELETE FROM users
     WHERE id = ?
     AND (
         is_admin = 0
         OR (SELECT COUNT(*) FROM users WHERE is_admin = 1) > 1
     )
     AND (
         -- Non-admin requester cannot delete admin target.
         ? = 1
         OR is_admin = 0
     )";

/// Joins users for author info; sorted by created_at descending (newest first).
pub const SQL_SELECT_ALL_NEWS: &str = "
    SELECT 
        n.id,
        n.body,
        n.image,
        n.author_id,
        u.username as author_username,
        u.is_admin as author_is_admin,
        n.created_at,
        n.updated_at
    FROM news n
    JOIN users u ON n.author_id = u.id
    ORDER BY n.created_at DESC";

/// Joins users for author info.
pub const SQL_SELECT_NEWS_BY_ID: &str = "
    SELECT 
        n.id,
        n.body,
        n.image,
        n.author_id,
        u.username as author_username,
        u.is_admin as author_is_admin,
        n.created_at,
        n.updated_at
    FROM news n
    JOIN users u ON n.author_id = u.id
    WHERE n.id = ?";

/// At least one of body or image must be non-null (enforced by CHECK constraint).
pub const SQL_INSERT_NEWS: &str = "
    INSERT INTO news (body, image, author_id, created_at)
    VALUES (?, ?, ?, ?)";

/// At least one of body or image must be non-null (enforced by CHECK constraint).
pub const SQL_UPDATE_NEWS: &str = "
    UPDATE news
    SET body = ?, image = ?, updated_at = ?
    WHERE id = ?";

pub const SQL_DELETE_NEWS: &str = "DELETE FROM news WHERE id = ?";

/// `ORDER BY name_lower` (the folded key) sorts case-insensitively; no
/// tiebreaker needed because two rows can't have case-equivalent names. `port`
/// is stored as INTEGER but always fits in `u16` (protocol-layer validation
/// rejects out-of-range ports before insert).
pub const SQL_SELECT_ALL_TRACKERS: &str = "
    SELECT id, address, port, fingerprint, password, name, enabled,
           created_at, updated_at
    FROM trackers
    ORDER BY name_lower";

pub const SQL_SELECT_TRACKER_BY_ID: &str = "
    SELECT id, address, port, fingerprint, password, name, enabled,
           created_at, updated_at
    FROM trackers
    WHERE id = ?";

/// The cap check runs atomically with the insert via `INSERT … SELECT …
/// WHERE`, so concurrent admin requests at `count == cap - 1` can't both
/// succeed. The trailing `cap: i64` bind makes the insert a no-op (0 rows
/// affected) when the table already has `>= cap` rows; caller distinguishes
/// via `rows_affected()`. Fails with a UNIQUE violation on a duplicate
/// `(address, port)`.
pub const SQL_INSERT_TRACKER: &str = "
    INSERT INTO trackers (address, port, fingerprint, password, name, name_lower,
                          enabled, created_at, updated_at)
    SELECT ?, ?, ?, ?, ?, ?, ?, ?, ?
    WHERE (SELECT COUNT(*) FROM trackers) < ?";

/// `created_at` is preserved. Fails with a UNIQUE violation if changing
/// `(address, port)` collides with another row.
pub const SQL_UPDATE_TRACKER: &str = "
    UPDATE trackers
    SET address = ?, port = ?, fingerprint = ?, password = ?, name = ?,
        name_lower = ?, enabled = ?, updated_at = ?
    WHERE id = ?";

/// Narrow update of only the fingerprint pin and `updated_at`. Used on TOFU
/// first-connect (row's fingerprint was NULL) and on admin-accept after a
/// fingerprint mismatch — neither path needs to touch other fields.
pub const SQL_UPDATE_TRACKER_FINGERPRINT: &str = "
    UPDATE trackers
    SET fingerprint = ?, updated_at = ?
    WHERE id = ?";

pub const SQL_DELETE_TRACKER: &str = "DELETE FROM trackers WHERE id = ?";

/// `ON CONFLICT(ip_address)` updates all fields if the IP already exists.
pub const SQL_UPSERT_BAN: &str = "
    INSERT INTO ip_bans (ip_address, nickname, nickname_lower, reason, created_by, created_at, expires_at)
    VALUES (?, ?, ?, ?, ?, ?, ?)
    ON CONFLICT(ip_address) DO UPDATE SET
        nickname = excluded.nickname,
        nickname_lower = excluded.nickname_lower,
        reason = excluded.reason,
        created_by = excluded.created_by,
        created_at = excluded.created_at,
        expires_at = excluded.expires_at";

pub const SQL_DELETE_BAN_BY_IP: &str = "DELETE FROM ip_bans WHERE ip_address = ?";

pub const SQL_DELETE_BANS_BY_NICKNAME_RETURNING: &str =
    "DELETE FROM ip_bans WHERE nickname_lower = ? RETURNING ip_address";

pub const SQL_SELECT_ALL_BAN_TARGETS: &str = "SELECT ip_address FROM ip_bans";

/// Sorted by creation time (newest first).
pub const SQL_SELECT_ACTIVE_BANS: &str = "
    SELECT ip_address, nickname, reason, created_by, created_at, expires_at
    FROM ip_bans
    WHERE expires_at IS NULL OR expires_at > ?
    ORDER BY created_at DESC";

/// Only deletes bans with a non-null expires_at that is <= now. Called on
/// server startup to clean up stale entries.
pub const SQL_DELETE_EXPIRED_BANS: &str = "
    DELETE FROM ip_bans
    WHERE expires_at IS NOT NULL AND expires_at <= ?";

/// `ON CONFLICT(ip_address)` updates all fields if the entry already exists.
pub const SQL_UPSERT_TRUST: &str = "
    INSERT INTO ip_trusted (ip_address, nickname, nickname_lower, reason, created_by, created_at, expires_at)
    VALUES (?, ?, ?, ?, ?, ?, ?)
    ON CONFLICT(ip_address) DO UPDATE SET
        nickname = excluded.nickname,
        nickname_lower = excluded.nickname_lower,
        reason = excluded.reason,
        created_by = excluded.created_by,
        created_at = excluded.created_at,
        expires_at = excluded.expires_at";

pub const SQL_DELETE_TRUST_BY_IP: &str = "DELETE FROM ip_trusted WHERE ip_address = ?";

pub const SQL_DELETE_TRUSTS_BY_NICKNAME_RETURNING: &str =
    "DELETE FROM ip_trusted WHERE nickname_lower = ? RETURNING ip_address";

pub const SQL_SELECT_ALL_TRUST_TARGETS: &str = "SELECT ip_address FROM ip_trusted";

/// Sorted by creation time (newest first).
pub const SQL_SELECT_ACTIVE_TRUSTS: &str = "
    SELECT ip_address, nickname, reason, created_by, created_at, expires_at
    FROM ip_trusted
    WHERE expires_at IS NULL OR expires_at > ?
    ORDER BY created_at DESC";

/// Only deletes trusts with a non-null expires_at that is <= now. Called on
/// server startup to clean up stale entries.
pub const SQL_DELETE_EXPIRED_TRUSTS: &str = "
    DELETE FROM ip_trusted
    WHERE expires_at IS NOT NULL AND expires_at <= ?";

/// Member count and permissions are fetched separately per group. Ordered by
/// the folded `name_lower` for Unicode-consistent display ordering.
pub const SQL_SELECT_ALL_GROUPS: &str =
    "SELECT id, name, is_shared, bandwidth_weight FROM groups ORDER BY name_lower";

pub const SQL_SELECT_GROUP_BY_ID: &str =
    "SELECT id, name, is_shared, bandwidth_weight FROM groups WHERE id = ?";

/// Case-insensitive matching via the folded `name_lower` column (written as
/// `fold_name(name)`); the original case is preserved in `name`. Bind the
/// folded form (`fold_name(input)`), not the raw input.
pub const SQL_SELECT_GROUP_BY_NAME: &str =
    "SELECT id, name, is_shared, bandwidth_weight FROM groups WHERE name_lower = ?";

pub const SQL_COUNT_GROUP_MEMBERS: &str = "SELECT COUNT(*) FROM users WHERE group_id = ?";

pub const SQL_COUNT_ALL_GROUP_MEMBERS: &str =
    "SELECT group_id, COUNT(*) FROM users WHERE group_id IS NOT NULL GROUP BY group_id";

pub const SQL_SELECT_GROUP_PERMISSIONS: &str =
    "SELECT permission FROM group_permissions WHERE group_id = ? ORDER BY permission";

pub const SQL_SELECT_ALL_GROUP_PERMISSIONS: &str =
    "SELECT group_id, permission FROM group_permissions ORDER BY group_id, permission";

/// Used when replacing group permissions during an update.
pub const SQL_DELETE_GROUP_PERMISSIONS: &str = "DELETE FROM group_permissions WHERE group_id = ?";

pub const SQL_INSERT_GROUP_PERMISSION: &str =
    "INSERT INTO group_permissions (group_id, permission) VALUES (?, ?)";

pub const SQL_INSERT_GROUP_PERMISSION_OR_IGNORE: &str =
    "INSERT OR IGNORE INTO group_permissions (group_id, permission) VALUES (?, ?)";

pub const SQL_DELETE_GROUP_PERMISSION_BY_NAME: &str =
    "DELETE FROM group_permissions WHERE group_id = ? AND permission = ?";

/// `name_lower` must be bound as `fold_name(name)` — the folded uniqueness key.
pub const SQL_INSERT_GROUP: &str =
    "INSERT INTO groups (name, name_lower, is_shared, bandwidth_weight) VALUES (?, ?, ?, ?)";

/// Atomic protection: when `is_shared` is unchanged the `is_shared = ?`
/// condition passes and the member-count subquery is skipped; when it IS
/// changing the update only proceeds if the group has zero members,
/// preventing a TOCTOU race against the handler's pre-check. Returns 0 rows
/// affected if blocked.
pub const SQL_UPDATE_GROUP: &str =
    "UPDATE groups SET name = ?, name_lower = ?, is_shared = ?, bandwidth_weight = ?
    WHERE id = ?
    AND (
        is_shared = ?
        OR (SELECT COUNT(*) FROM users WHERE group_id = ?) = 0
    )";

/// Deletes only if the group has no assigned members. Returns rows_affected =
/// 1 on success, 0 if the group is not found OR has members; caller can
/// distinguish with a follow-up SELECT if needed.
pub const SQL_DELETE_GROUP: &str =
    "DELETE FROM groups WHERE id = ? AND (SELECT COUNT(*) FROM users WHERE group_id = ?) = 0";

/// Case-insensitive matching via the folded `name_lower` column (written as
/// `fold_name(name)`); the original case is preserved in `name`. Bind the
/// folded form (`fold_name(input)`), not the raw input.
pub const SQL_SELECT_CHANNEL_SETTINGS: &str =
    "SELECT name, topic, topic_set_by, secret FROM channel_settings WHERE name_lower = ?";

pub const SQL_SELECT_ALL_CHANNEL_SETTINGS: &str =
    "SELECT name, topic, topic_set_by, secret FROM channel_settings";

/// `ON CONFLICT` upsert — creates if the channel doesn't exist, updates if it
/// does. Conflicts resolve on the folded `name_lower` (bind it as
/// `fold_name(name)`); `name` itself is never updated, so the original stored
/// case is preserved across upserts.
pub const SQL_UPSERT_CHANNEL_SETTINGS: &str = "INSERT INTO channel_settings (name, name_lower, topic, topic_set_by, secret) VALUES (?, ?, ?, ?, ?) ON CONFLICT(name_lower) DO UPDATE SET topic = excluded.topic, topic_set_by = excluded.topic_set_by, secret = excluded.secret";

/// Matches on the folded `name_lower`; bind `fold_name(name)`.
pub const SQL_UPDATE_CHANNEL_TOPIC: &str =
    "UPDATE channel_settings SET topic = ?, topic_set_by = ? WHERE name_lower = ?";

/// Matches on the folded `name_lower`; bind `fold_name(name)`.
pub const SQL_UPDATE_CHANNEL_SECRET: &str =
    "UPDATE channel_settings SET secret = ? WHERE name_lower = ?";

/// Matches on the folded `name_lower`; bind `fold_name(name)`.
pub const SQL_DELETE_CHANNEL_SETTINGS: &str = "DELETE FROM channel_settings WHERE name_lower = ?";

pub const SQL_SELECT_USER_BY_ID: &str = "SELECT id, username, password_hash, is_admin, is_shared, enabled, created_at, group_id, bandwidth_weight FROM users WHERE id = ?";

#[cfg(test)]
pub mod test_sql {
    /// Returns the ban even if expired.
    pub const SQL_SELECT_BAN_BY_IP_UNFILTERED: &str = "
        SELECT ip_address, nickname, reason, created_by, created_at, expires_at
        FROM ip_bans
        WHERE ip_address = ?";

    pub const SQL_COUNT_BANS_BY_NICKNAME: &str =
        "SELECT COUNT(*) FROM ip_bans WHERE nickname_lower = ?";

    /// Returns the trust entry even if expired.
    pub const SQL_SELECT_TRUST_BY_IP_UNFILTERED: &str = "
        SELECT ip_address, nickname, reason, created_by, created_at, expires_at
        FROM ip_trusted
        WHERE ip_address = ?";

    pub const SQL_COUNT_TRUSTS_BY_NICKNAME: &str =
        "SELECT COUNT(*) FROM ip_trusted WHERE nickname_lower = ?";

    /// Production code uses cached permissions.
    pub const SQL_CHECK_IS_ADMIN: &str = "SELECT is_admin FROM users WHERE id = ?";

    /// Returns only permanent or not-yet-expired bans.
    pub const SQL_SELECT_BAN_BY_IP: &str = "
        SELECT ip_address, nickname, reason, created_by, created_at, expires_at
        FROM ip_bans
        WHERE ip_address = ?
        AND (expires_at IS NULL OR expires_at > ?)";

    /// Returns only permanent or not-yet-expired trusts.
    pub const SQL_SELECT_TRUST_BY_IP: &str = "
        SELECT ip_address, nickname, reason, created_by, created_at, expires_at
        FROM ip_trusted
        WHERE ip_address = ?
        AND (expires_at IS NULL OR expires_at > ?)";

    pub const SQL_COUNT_ADMINS: &str = "SELECT COUNT(*) FROM users WHERE is_admin = 1";

    pub const SQL_COUNT_USER_PERMISSIONS: &str =
        "SELECT COUNT(*) FROM user_permissions WHERE user_id = ?";

    pub const SQL_COUNT_USERS: &str = "SELECT COUNT(*) FROM users";
}
