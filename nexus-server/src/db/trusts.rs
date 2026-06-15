//! IP trusted database operations

use std::time::{SystemTime, UNIX_EPOCH};

use ipnet::IpNet;
use nexus_common::names::fold_name;
use sqlx::sqlite::SqlitePool;

use crate::constants::ERR_SYSTEM_TIME_BEFORE_EPOCH_CHECK_CLOCK;
use crate::db::sql;
use crate::db::util::{begin_immediate, target_is_contained_by_range};
use crate::ip_rule_cache::assert_canonical_target;

#[derive(Debug, Clone)]
pub struct TrustRecord {
    /// Canonical lowercase IP address or CIDR (network address with host bits
    /// zeroed for ranges). Produced by `canonicalize_target` at the handler
    /// boundary before storage.
    pub ip_address: String,
    /// Optional nickname annotation, stored in display case (the true nickname
    /// of the resolved session). Case-insensitive delete matches the folded
    /// `nickname_lower` column, not this one.
    pub nickname: Option<String>,
    pub reason: Option<String>,
    /// Admin username, preserved as-typed. Display-only; never used in
    /// `WHERE` predicates.
    pub created_by: String,
    pub created_at: i64,
    pub expires_at: Option<i64>,
}

type TrustRow = (
    String,
    Option<String>,
    Option<String>,
    String,
    i64,
    Option<i64>,
);

impl From<TrustRow> for TrustRecord {
    fn from(row: TrustRow) -> Self {
        Self {
            ip_address: row.0,
            nickname: row.1,
            reason: row.2,
            created_by: row.3,
            created_at: row.4,
            expires_at: row.5,
        }
    }
}

#[derive(Clone)]
pub struct TrustDb {
    pool: SqlitePool,
}

impl TrustDb {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    fn now() -> i64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect(ERR_SYSTEM_TIME_BEFORE_EPOCH_CHECK_CLOCK)
            .as_secs() as i64
    }

    /// Create or update multiple trusted targets atomically.
    ///
    /// A target is a canonical single IP or CIDR range. Used when a nickname
    /// resolves to several connected IPs: either every resolved target is
    /// persisted, or none are.
    pub async fn create_or_update_trust_targets<'a, I>(
        &self,
        targets: I,
        nickname: Option<&str>,
        reason: Option<&str>,
        created_by: &str,
        expires_at: Option<i64>,
    ) -> Result<Vec<TrustRecord>, sqlx::Error>
    where
        I: IntoIterator<Item = &'a str>,
    {
        let targets: Vec<&str> = targets.into_iter().collect();
        for target in &targets {
            assert_canonical_target(target);
        }

        let now = Self::now();
        let nickname_lower = nickname.map(fold_name);
        let mut records = Vec::with_capacity(targets.len());
        let mut tx = self.pool.begin().await?;

        for target in targets {
            sqlx::query(sql::SQL_UPSERT_TRUST)
                .bind(target)
                .bind(nickname)
                .bind(nickname_lower.as_deref())
                .bind(reason)
                .bind(created_by)
                .bind(now)
                .bind(expires_at)
                .execute(&mut *tx)
                .await?;

            records.push(TrustRecord {
                ip_address: target.to_string(),
                nickname: nickname.map(str::to_owned),
                reason: reason.map(str::to_owned),
                created_by: created_by.to_string(),
                created_at: now,
                expires_at,
            });
        }

        tx.commit().await?;
        Ok(records)
    }

    /// Returns true if a trust was deleted, false if no trust existed.
    pub async fn delete_trust_by_ip(&self, ip_address: &str) -> Result<bool, sqlx::Error> {
        let result = sqlx::query(sql::SQL_DELETE_TRUST_BY_IP)
            .bind(ip_address)
            .execute(&self.pool)
            .await?;

        Ok(result.rows_affected() > 0)
    }

    /// Delete all trusts with a given nickname annotation.
    ///
    /// Lookup is case-insensitive: the supplied nickname is folded and matched
    /// against the `nickname_lower` column. Returns the list of IP addresses
    /// that were untrusted.
    pub async fn delete_trusts_by_nickname(
        &self,
        nickname: &str,
    ) -> Result<Vec<String>, sqlx::Error> {
        let nickname_lower = fold_name(nickname);

        let rows: Vec<(String,)> = sqlx::query_as(sql::SQL_DELETE_TRUSTS_BY_NICKNAME_RETURNING)
            .bind(&nickname_lower)
            .fetch_all(&self.pool)
            .await?;

        Ok(rows.into_iter().map(|(ip,)| ip).collect())
    }

    /// List all active (non-expired) trusts, sorted by creation time (newest first).
    pub async fn list_active_trusts(&self) -> Result<Vec<TrustRecord>, sqlx::Error> {
        let now = Self::now();

        let rows: Vec<TrustRow> = sqlx::query_as(sql::SQL_SELECT_ACTIVE_TRUSTS)
            .bind(now)
            .fetch_all(&self.pool)
            .await?;

        Ok(rows.into_iter().map(TrustRecord::from).collect())
    }

    pub async fn cleanup_expired_trusts(&self) -> Result<u64, sqlx::Error> {
        let now = Self::now();

        let result = sqlx::query(sql::SQL_DELETE_EXPIRED_TRUSTS)
            .bind(now)
            .execute(&self.pool)
            .await?;

        Ok(result.rows_affected())
    }

    pub async fn load_all_active_trusts(&self) -> Result<Vec<TrustRecord>, sqlx::Error> {
        self.list_active_trusts().await
    }

    /// Delete every trust whose IP/CIDR is contained within `range`.
    ///
    /// Cascades a CIDR untrust to the single IPs and smaller ranges nested
    /// inside it. Returns the IP/CIDR strings that were deleted.
    pub async fn delete_trusts_in_range(&self, range: &IpNet) -> Result<Vec<String>, sqlx::Error> {
        let mut tx = begin_immediate(&self.pool).await?;
        let rows: Vec<(String,)> = sqlx::query_as(sql::SQL_SELECT_ALL_TRUST_TARGETS)
            .fetch_all(&mut *tx)
            .await?;

        let targets: Vec<String> = rows
            .into_iter()
            .map(|(target,)| target)
            .filter(|target| target_is_contained_by_range(target, range))
            .collect();

        let mut deleted = Vec::with_capacity(targets.len());
        for target in targets {
            let result = sqlx::query(sql::SQL_DELETE_TRUST_BY_IP)
                .bind(&target)
                .execute(&mut *tx)
                .await?;

            if result.rows_affected() > 0 {
                deleted.push(target);
            }
        }

        tx.commit().await?;
        Ok(deleted)
    }
}

#[cfg(test)]
impl TrustDb {
    /// Create or update one trust target (upsert).
    pub async fn create_or_update_trust(
        &self,
        target: &str,
        nickname: Option<&str>,
        reason: Option<&str>,
        created_by: &str,
        expires_at: Option<i64>,
    ) -> Result<TrustRecord, sqlx::Error> {
        self.create_or_update_trust_targets(
            std::iter::once(target),
            nickname,
            reason,
            created_by,
            expires_at,
        )
        .await?
        .into_iter()
        .next()
        .ok_or(sqlx::Error::RowNotFound)
    }

    /// Get a trust entry by IP/CIDR target regardless of expiry status.
    async fn get_trust_by_ip_unfiltered(
        &self,
        ip_address: &str,
    ) -> Result<Option<TrustRecord>, sqlx::Error> {
        let row: Option<TrustRow> =
            sqlx::query_as(sql::test_sql::SQL_SELECT_TRUST_BY_IP_UNFILTERED)
                .bind(ip_address)
                .fetch_optional(&self.pool)
                .await?;

        Ok(row.map(TrustRecord::from))
    }

    /// Check if a trust entry exists for a given IP/CIDR regardless of expiry.
    pub async fn trust_exists(&self, ip_address: &str) -> Result<bool, sqlx::Error> {
        Ok(self.get_trust_by_ip_unfiltered(ip_address).await?.is_some())
    }

    /// Check if an IP is currently trusted.
    pub async fn is_ip_trusted(&self, ip_address: &str) -> Result<bool, sqlx::Error> {
        let now = Self::now();

        let row: Option<TrustRow> = sqlx::query_as(sql::test_sql::SQL_SELECT_TRUST_BY_IP)
            .bind(ip_address)
            .bind(now)
            .fetch_optional(&self.pool)
            .await?;

        Ok(row.is_some())
    }

    /// Get a trust entry by IP address if it has not expired.
    pub async fn get_trust_by_ip(
        &self,
        ip_address: &str,
    ) -> Result<Option<TrustRecord>, sqlx::Error> {
        let now = Self::now();

        let row: Option<TrustRow> = sqlx::query_as(sql::test_sql::SQL_SELECT_TRUST_BY_IP)
            .bind(ip_address)
            .bind(now)
            .fetch_optional(&self.pool)
            .await?;

        Ok(row.map(TrustRecord::from))
    }

    /// Check if any trusts exist with a given nickname annotation.
    pub async fn has_trusts_for_nickname(&self, nickname: &str) -> Result<bool, sqlx::Error> {
        let row: (i64,) = sqlx::query_as(sql::test_sql::SQL_COUNT_TRUSTS_BY_NICKNAME)
            .bind(fold_name(nickname))
            .fetch_one(&self.pool)
            .await?;

        Ok(row.0 > 0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::testing::create_test_db;

    #[tokio::test]
    async fn test_create_trust() {
        let pool = create_test_db().await;
        let db = TrustDb::new(pool);

        let trust = db
            .create_or_update_trust(
                "192.168.1.100",
                Some("alice"),
                Some("office network"),
                "admin",
                None,
            )
            .await
            .expect("create trust");

        assert_eq!(trust.ip_address, "192.168.1.100");
        assert_eq!(trust.nickname, Some("alice".to_string()));
        assert_eq!(trust.reason, Some("office network".to_string()));
        assert_eq!(trust.created_by, "admin");
        assert!(trust.expires_at.is_none()); // permanent
    }

    #[tokio::test]
    async fn test_create_trust_with_expiry() {
        let pool = create_test_db().await;
        let db = TrustDb::new(pool);

        let expires = TrustDb::now() + 3600;

        let trust = db
            .create_or_update_trust("10.0.0.1", None, None, "admin", Some(expires))
            .await
            .expect("create trust");

        assert_eq!(trust.ip_address, "10.0.0.1");
        assert_eq!(trust.expires_at, Some(expires));
    }

    #[tokio::test]
    async fn test_create_or_update_trust_targets_multi_target() {
        let pool = create_test_db().await;
        let db = TrustDb::new(pool);

        let trusts = db
            .create_or_update_trust_targets(
                ["192.168.1.100", "192.168.1.0/24"],
                Some("target"),
                Some("reason"),
                "admin",
                None,
            )
            .await
            .expect("create trusts");

        assert_eq!(trusts.len(), 2);
        assert!(db.is_ip_trusted("192.168.1.100").await.unwrap());
        assert!(db.trust_exists("192.168.1.0/24").await.unwrap());
        assert!(db.has_trusts_for_nickname("TARGET").await.unwrap());
    }

    #[tokio::test]
    async fn test_upsert_trust() {
        let pool = create_test_db().await;
        let db = TrustDb::new(pool);

        db.create_or_update_trust(
            "192.168.1.100",
            Some("alice"),
            Some("reason1"),
            "admin1",
            None,
        )
        .await
        .expect("create trust");

        // Re-upsert the same IP with new field values.
        let trust = db
            .create_or_update_trust(
                "192.168.1.100",
                Some("bob"),
                Some("reason2"),
                "admin2",
                None,
            )
            .await
            .expect("update trust");

        assert_eq!(trust.ip_address, "192.168.1.100");
        assert_eq!(trust.nickname, Some("bob".to_string()));
        assert_eq!(trust.reason, Some("reason2".to_string()));
        assert_eq!(trust.created_by, "admin2");
    }

    #[tokio::test]
    async fn test_is_ip_trusted() {
        let pool = create_test_db().await;
        let db = TrustDb::new(pool);

        assert!(!db.is_ip_trusted("192.168.1.100").await.unwrap());

        db.create_or_update_trust("192.168.1.100", None, None, "admin", None)
            .await
            .expect("create trust");

        assert!(db.is_ip_trusted("192.168.1.100").await.unwrap());
    }

    #[tokio::test]
    async fn test_expired_trust_not_returned() {
        let pool = create_test_db().await;
        let db = TrustDb::new(pool);

        let expired = TrustDb::now() - 1;

        db.create_or_update_trust("192.168.1.100", None, None, "admin", Some(expired))
            .await
            .expect("create trust");

        // An expired trust is neither trusted nor returned.
        assert!(!db.is_ip_trusted("192.168.1.100").await.unwrap());
        assert!(db.get_trust_by_ip("192.168.1.100").await.unwrap().is_none());
    }

    #[tokio::test]
    async fn test_delete_trust_by_ip() {
        let pool = create_test_db().await;
        let db = TrustDb::new(pool);

        db.create_or_update_trust("192.168.1.100", None, None, "admin", None)
            .await
            .expect("create trust");

        assert!(db.is_ip_trusted("192.168.1.100").await.unwrap());

        let deleted = db.delete_trust_by_ip("192.168.1.100").await.unwrap();
        assert!(deleted);

        assert!(!db.is_ip_trusted("192.168.1.100").await.unwrap());

        // Deleting again returns false.
        let deleted = db.delete_trust_by_ip("192.168.1.100").await.unwrap();
        assert!(!deleted);
    }

    #[tokio::test]
    async fn test_delete_trusts_by_nickname() {
        let pool = create_test_db().await;
        let db = TrustDb::new(pool);

        db.create_or_update_trust("192.168.1.100", Some("alice"), None, "admin", None)
            .await
            .expect("create trust 1");
        db.create_or_update_trust("192.168.1.101", Some("alice"), None, "admin", None)
            .await
            .expect("create trust 2");
        db.create_or_update_trust("192.168.1.102", Some("other"), None, "admin", None)
            .await
            .expect("create trust 3");

        let deleted_ips = db.delete_trusts_by_nickname("alice").await.unwrap();

        assert_eq!(deleted_ips.len(), 2);
        assert!(deleted_ips.contains(&"192.168.1.100".to_string()));
        assert!(deleted_ips.contains(&"192.168.1.101".to_string()));

        // The "other" nickname's trust is untouched.
        assert!(db.is_ip_trusted("192.168.1.102").await.unwrap());
    }

    #[tokio::test]
    async fn test_has_trusts_for_nickname() {
        let pool = create_test_db().await;
        let db = TrustDb::new(pool);

        assert!(!db.has_trusts_for_nickname("alice").await.unwrap());

        db.create_or_update_trust("192.168.1.100", Some("alice"), None, "admin", None)
            .await
            .expect("create trust");

        assert!(db.has_trusts_for_nickname("alice").await.unwrap());
    }

    #[tokio::test]
    async fn test_trusts_by_nickname_is_case_insensitive() {
        let pool = create_test_db().await;
        let db = TrustDb::new(pool);

        // Display case is preserved in `nickname`; `nickname_lower` keys lookups.
        let record = db
            .create_or_update_trust("192.168.1.100", Some("Alice"), None, "Admin", None)
            .await
            .expect("create trust");
        assert_eq!(record.nickname, Some("Alice".to_string()));
        // created_by is display-only; preserved as-typed
        assert_eq!(record.created_by, "Admin");

        // Lookup / delete with arbitrary case still finds the row
        assert!(db.has_trusts_for_nickname("alice").await.unwrap());
        assert!(db.has_trusts_for_nickname("ALICE").await.unwrap());

        let deleted = db.delete_trusts_by_nickname("Alice").await.unwrap();
        assert_eq!(deleted, vec!["192.168.1.100".to_string()]);
        assert!(!db.has_trusts_for_nickname("alice").await.unwrap());
    }

    #[tokio::test]
    async fn test_trusts_by_nickname_unicode_case_insensitive() {
        let pool = create_test_db().await;
        let db = TrustDb::new(pool);

        // Display case is preserved in `nickname`; lookup/delete fold via
        // `nickname_lower` (full Unicode), so a non-ASCII case pair (é↔É) is
        // found case-insensitively.
        let record = db
            .create_or_update_trust("192.168.1.100", Some("Renée"), None, "Admin", None)
            .await
            .expect("create trust");
        assert_eq!(record.nickname, Some("Renée".to_string()));

        assert!(db.has_trusts_for_nickname("renée").await.unwrap());
        assert!(db.has_trusts_for_nickname("RENÉE").await.unwrap());

        let deleted = db.delete_trusts_by_nickname("Renée").await.unwrap();
        assert_eq!(deleted, vec!["192.168.1.100".to_string()]);
        assert!(!db.has_trusts_for_nickname("renée").await.unwrap());
    }

    #[tokio::test]
    async fn test_list_active_trusts() {
        let pool = create_test_db().await;
        let db = TrustDb::new(pool);

        db.create_or_update_trust("192.168.1.100", Some("alice"), None, "admin", None)
            .await
            .expect("create trust 1");
        db.create_or_update_trust("192.168.1.101", None, Some("office"), "admin", None)
            .await
            .expect("create trust 2");

        let expired = TrustDb::now() - 1;
        db.create_or_update_trust("192.168.1.102", None, None, "admin", Some(expired))
            .await
            .expect("create expired trust");

        let trusts = db.list_active_trusts().await.unwrap();

        // The expired trust is excluded.
        assert_eq!(trusts.len(), 2);
    }

    #[tokio::test]
    async fn test_cleanup_expired_trusts() {
        let pool = create_test_db().await;
        let db = TrustDb::new(pool);

        let expired = TrustDb::now() - 1;
        db.create_or_update_trust("192.168.1.100", None, None, "admin", Some(expired))
            .await
            .expect("create expired trust 1");
        db.create_or_update_trust("192.168.1.101", None, None, "admin", Some(expired))
            .await
            .expect("create expired trust 2");

        db.create_or_update_trust("192.168.1.102", None, None, "admin", None)
            .await
            .expect("create permanent trust");

        let future = TrustDb::now() + 3600;
        db.create_or_update_trust("192.168.1.103", None, None, "admin", Some(future))
            .await
            .expect("create future trust");

        let deleted = db.cleanup_expired_trusts().await.unwrap();
        assert_eq!(deleted, 2);

        // Permanent and future trusts survive cleanup.
        let trusts = db.list_active_trusts().await.unwrap();
        assert_eq!(trusts.len(), 2);
    }

    #[tokio::test]
    async fn test_delete_trusts_in_range_deletes_expired_contained_trusts() {
        let pool = create_test_db().await;
        let db = TrustDb::new(pool);

        let expired = TrustDb::now() - 1;
        db.create_or_update_trust("192.168.1.50", None, None, "admin", Some(expired))
            .await
            .expect("create contained expired trust");
        db.create_or_update_trust("192.168.2.1", None, None, "admin", Some(expired))
            .await
            .expect("create outside expired trust");

        let range: IpNet = "192.168.1.0/24".parse().unwrap();
        let deleted = db.delete_trusts_in_range(&range).await.unwrap();

        assert_eq!(deleted, vec!["192.168.1.50".to_string()]);
        assert!(!db.trust_exists("192.168.1.50").await.unwrap());
        assert!(db.trust_exists("192.168.2.1").await.unwrap());
    }

    #[tokio::test]
    #[should_panic(expected = "ip_or_cidr must be canonical")]
    async fn test_create_or_update_trust_panics_on_non_canonical_ip() {
        // Handler is the canonicalization funnel; the DB trusts its callers
        // to pre-canonicalize. The debug_assert catches a future caller that
        // skips that funnel.
        let pool = create_test_db().await;
        let db = TrustDb::new(pool);
        let _ = db
            .create_or_update_trust("192.168.1.5/24", None, None, "admin", None)
            .await;
    }
}
