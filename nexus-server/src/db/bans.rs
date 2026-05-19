//! IP ban database operations

use std::net::IpAddr;
use std::time::{SystemTime, UNIX_EPOCH};

use ipnet::IpNet;
use sqlx::sqlite::SqlitePool;

use crate::constants::{ERR_SYSTEM_TIME_BEFORE_EPOCH_CHECK_CLOCK, ERR_VALID_IP_PREFIX};
use crate::db::sql;
use crate::ip_rule_cache::assert_canonical_target;

#[derive(Debug, Clone)]
pub struct BanRecord {
    /// Canonical lowercase IP address or CIDR (network address with host bits
    /// zeroed for ranges). Produced by `canonicalize_target` at the handler
    /// boundary before storage.
    pub ip_address: String,
    /// Optional nickname annotation. Stored in canonical lowercase by
    /// `create_or_update_ban` so case-insensitive lookups
    /// (`has_bans_for_nickname`, `delete_bans_by_nickname`) round-trip
    /// regardless of admin-typed casing.
    pub nickname: Option<String>,
    pub reason: Option<String>,
    /// Admin username, preserved as-typed. Display-only; never used in
    /// `WHERE` predicates.
    pub created_by: String,
    pub created_at: i64,
    pub expires_at: Option<i64>,
}

type BanRow = (
    String,
    Option<String>,
    Option<String>,
    String,
    i64,
    Option<i64>,
);

impl From<BanRow> for BanRecord {
    fn from(row: BanRow) -> Self {
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
pub struct BanDb {
    pool: SqlitePool,
}

impl BanDb {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    fn now() -> i64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect(ERR_SYSTEM_TIME_BEFORE_EPOCH_CHECK_CLOCK)
            .as_secs() as i64
    }

    /// Create or update an IP ban (upsert)
    ///
    /// If the IP already exists, all fields are updated. The `nickname`
    /// annotation is lowercased to its canonical form so case-insensitive
    /// lookups (`has_bans_for_nickname`, `delete_bans_by_nickname`)
    /// round-trip cleanly. `created_by` is preserved as-typed since it's a
    /// display-only field.
    pub async fn create_or_update_ban(
        &self,
        ip_address: &str,
        nickname: Option<&str>,
        reason: Option<&str>,
        created_by: &str,
        expires_at: Option<i64>,
    ) -> Result<BanRecord, sqlx::Error> {
        assert_canonical_target(ip_address);
        let now = Self::now();
        let nickname_lower = nickname.map(str::to_lowercase);

        sqlx::query(sql::SQL_UPSERT_BAN)
            .bind(ip_address)
            .bind(nickname_lower.as_deref())
            .bind(reason)
            .bind(created_by)
            .bind(now)
            .bind(expires_at)
            .execute(&self.pool)
            .await?;

        // Re-read without the expiry filter so the just-written row is returned.
        self.get_ban_by_ip_unfiltered(ip_address)
            .await?
            .ok_or(sqlx::Error::RowNotFound)
    }

    /// Get a ban by IP address regardless of expiry status.
    async fn get_ban_by_ip_unfiltered(
        &self,
        ip_address: &str,
    ) -> Result<Option<BanRecord>, sqlx::Error> {
        let row: Option<BanRow> = sqlx::query_as(sql::SQL_SELECT_BAN_BY_IP_UNFILTERED)
            .bind(ip_address)
            .fetch_optional(&self.pool)
            .await?;

        Ok(row.map(BanRecord::from))
    }

    /// Check if a ban exists for a given IP/CIDR (regardless of expiry).
    #[cfg(test)]
    pub async fn ban_exists(&self, ip_address: &str) -> Result<bool, sqlx::Error> {
        Ok(self.get_ban_by_ip_unfiltered(ip_address).await?.is_some())
    }

    /// Check if an IP is currently banned (not expired).
    #[cfg(test)]
    pub async fn is_ip_banned(&self, ip_address: &str) -> Result<bool, sqlx::Error> {
        use crate::db::sql;

        let now = Self::now();

        let row: Option<BanRow> = sqlx::query_as(sql::SQL_SELECT_BAN_BY_IP)
            .bind(ip_address)
            .bind(now)
            .fetch_optional(&self.pool)
            .await?;

        Ok(row.is_some())
    }

    /// Get a ban by IP address (only if not expired).
    #[cfg(test)]
    pub async fn get_ban_by_ip(&self, ip_address: &str) -> Result<Option<BanRecord>, sqlx::Error> {
        use crate::db::sql;

        let now = Self::now();

        let row: Option<BanRow> = sqlx::query_as(sql::SQL_SELECT_BAN_BY_IP)
            .bind(ip_address)
            .bind(now)
            .fetch_optional(&self.pool)
            .await?;

        Ok(row.map(BanRecord::from))
    }

    /// Returns true if a ban was deleted, false if no ban existed.
    pub async fn delete_ban_by_ip(&self, ip_address: &str) -> Result<bool, sqlx::Error> {
        let result = sqlx::query(sql::SQL_DELETE_BAN_BY_IP)
            .bind(ip_address)
            .execute(&self.pool)
            .await?;

        Ok(result.rows_affected() > 0)
    }

    /// Delete all bans with a given nickname annotation.
    ///
    /// Lookup is case-insensitive: the supplied nickname is lowercased to
    /// match the stored form. Returns the list of IP addresses that were
    /// unbanned.
    pub async fn delete_bans_by_nickname(
        &self,
        nickname: &str,
    ) -> Result<Vec<String>, sqlx::Error> {
        let nickname_lower = nickname.to_lowercase();

        // Capture the matching IPs before deleting, so we can return them.
        let rows: Vec<(String,)> = sqlx::query_as(sql::SQL_SELECT_IPS_BY_NICKNAME)
            .bind(&nickname_lower)
            .fetch_all(&self.pool)
            .await?;

        let ips: Vec<String> = rows.into_iter().map(|(ip,)| ip).collect();

        if !ips.is_empty() {
            sqlx::query(sql::SQL_DELETE_BANS_BY_NICKNAME)
                .bind(&nickname_lower)
                .execute(&self.pool)
                .await?;
        }

        Ok(ips)
    }

    /// Check if any bans exist with a given nickname annotation.
    ///
    /// Lookup is case-insensitive: the supplied nickname is lowercased to
    /// match the stored form.
    pub async fn has_bans_for_nickname(&self, nickname: &str) -> Result<bool, sqlx::Error> {
        let row: (i64,) = sqlx::query_as(sql::SQL_COUNT_BANS_BY_NICKNAME)
            .bind(nickname.to_lowercase())
            .fetch_one(&self.pool)
            .await?;

        Ok(row.0 > 0)
    }

    /// List all active (non-expired) bans, sorted by creation time (newest first).
    pub async fn list_active_bans(&self) -> Result<Vec<BanRecord>, sqlx::Error> {
        let now = Self::now();

        let rows: Vec<BanRow> = sqlx::query_as(sql::SQL_SELECT_ACTIVE_BANS)
            .bind(now)
            .fetch_all(&self.pool)
            .await?;

        Ok(rows.into_iter().map(BanRecord::from).collect())
    }

    pub async fn cleanup_expired_bans(&self) -> Result<u64, sqlx::Error> {
        let now = Self::now();

        let result = sqlx::query(sql::SQL_DELETE_EXPIRED_BANS)
            .bind(now)
            .execute(&self.pool)
            .await?;

        Ok(result.rows_affected())
    }

    pub async fn load_all_active_bans(&self) -> Result<Vec<BanRecord>, sqlx::Error> {
        self.list_active_bans().await
    }

    /// Delete every ban whose IP/CIDR is contained within `range`.
    ///
    /// Cascades a CIDR unban to the single IPs and smaller ranges nested
    /// inside it. Returns the IP/CIDR strings that were deleted.
    pub async fn delete_bans_in_range(&self, range: &IpNet) -> Result<Vec<String>, sqlx::Error> {
        let all_bans = self.list_active_bans().await?;

        let mut deleted = Vec::new();

        for ban in all_bans {
            let ban_net = if let Ok(net) = ban.ip_address.parse::<IpNet>() {
                net
            } else if let Ok(ip) = ban.ip_address.parse::<IpAddr>() {
                // A bare IP is treated as a single-host /32 or /128.
                match ip {
                    IpAddr::V4(v4) => {
                        IpNet::V4(ipnet::Ipv4Net::new(v4, 32).expect(ERR_VALID_IP_PREFIX))
                    }
                    IpAddr::V6(v6) => {
                        IpNet::V6(ipnet::Ipv6Net::new(v6, 128).expect(ERR_VALID_IP_PREFIX))
                    }
                }
            } else {
                continue;
            };

            let is_contained = match (&ban_net, range) {
                (IpNet::V4(ban_v4), IpNet::V4(range_v4)) => {
                    range_v4.contains(&ban_v4.network())
                        && ban_v4.prefix_len() >= range_v4.prefix_len()
                }
                (IpNet::V6(ban_v6), IpNet::V6(range_v6)) => {
                    range_v6.contains(&ban_v6.network())
                        && ban_v6.prefix_len() >= range_v6.prefix_len()
                }
                _ => false, // IPv4/IPv6 mismatch
            };

            if is_contained && self.delete_ban_by_ip(&ban.ip_address).await? {
                deleted.push(ban.ip_address);
            }
        }

        Ok(deleted)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::testing::create_test_db;

    #[tokio::test]
    async fn test_create_ban() {
        let pool = create_test_db().await;
        let db = BanDb::new(pool);

        let ban = db
            .create_or_update_ban(
                "192.168.1.100",
                Some("spammer"),
                Some("flooding"),
                "admin",
                None,
            )
            .await
            .expect("create ban");

        assert_eq!(ban.ip_address, "192.168.1.100");
        assert_eq!(ban.nickname, Some("spammer".to_string()));
        assert_eq!(ban.reason, Some("flooding".to_string()));
        assert_eq!(ban.created_by, "admin");
        assert!(ban.expires_at.is_none()); // permanent
    }

    #[tokio::test]
    async fn test_create_ban_with_expiry() {
        let pool = create_test_db().await;
        let db = BanDb::new(pool);

        let expires = BanDb::now() + 3600;

        let ban = db
            .create_or_update_ban("10.0.0.1", None, None, "admin", Some(expires))
            .await
            .expect("create ban");

        assert_eq!(ban.ip_address, "10.0.0.1");
        assert_eq!(ban.expires_at, Some(expires));
    }

    #[tokio::test]
    async fn test_upsert_ban() {
        let pool = create_test_db().await;
        let db = BanDb::new(pool);

        db.create_or_update_ban(
            "192.168.1.100",
            Some("alice"),
            Some("reason1"),
            "admin1",
            None,
        )
        .await
        .expect("create ban");

        // Re-upsert the same IP with new field values.
        let ban = db
            .create_or_update_ban(
                "192.168.1.100",
                Some("bob"),
                Some("reason2"),
                "admin2",
                None,
            )
            .await
            .expect("update ban");

        assert_eq!(ban.ip_address, "192.168.1.100");
        assert_eq!(ban.nickname, Some("bob".to_string()));
        assert_eq!(ban.reason, Some("reason2".to_string()));
        assert_eq!(ban.created_by, "admin2");
    }

    #[tokio::test]
    async fn test_is_ip_banned() {
        let pool = create_test_db().await;
        let db = BanDb::new(pool);

        assert!(!db.is_ip_banned("192.168.1.100").await.unwrap());

        db.create_or_update_ban("192.168.1.100", None, None, "admin", None)
            .await
            .expect("create ban");

        assert!(db.is_ip_banned("192.168.1.100").await.unwrap());
    }

    #[tokio::test]
    async fn test_expired_ban_not_returned() {
        let pool = create_test_db().await;
        let db = BanDb::new(pool);

        let expired = BanDb::now() - 1;

        db.create_or_update_ban("192.168.1.100", None, None, "admin", Some(expired))
            .await
            .expect("create ban");

        // An expired ban is neither banned nor returned.
        assert!(!db.is_ip_banned("192.168.1.100").await.unwrap());
        assert!(db.get_ban_by_ip("192.168.1.100").await.unwrap().is_none());
    }

    #[tokio::test]
    async fn test_delete_ban_by_ip() {
        let pool = create_test_db().await;
        let db = BanDb::new(pool);

        db.create_or_update_ban("192.168.1.100", None, None, "admin", None)
            .await
            .expect("create ban");

        assert!(db.is_ip_banned("192.168.1.100").await.unwrap());

        let deleted = db.delete_ban_by_ip("192.168.1.100").await.unwrap();
        assert!(deleted);

        assert!(!db.is_ip_banned("192.168.1.100").await.unwrap());

        // Deleting again returns false.
        let deleted = db.delete_ban_by_ip("192.168.1.100").await.unwrap();
        assert!(!deleted);
    }

    #[tokio::test]
    async fn test_delete_bans_by_nickname() {
        let pool = create_test_db().await;
        let db = BanDb::new(pool);

        db.create_or_update_ban("192.168.1.100", Some("spammer"), None, "admin", None)
            .await
            .expect("create ban 1");
        db.create_or_update_ban("192.168.1.101", Some("spammer"), None, "admin", None)
            .await
            .expect("create ban 2");
        db.create_or_update_ban("192.168.1.102", Some("other"), None, "admin", None)
            .await
            .expect("create ban 3");

        let deleted_ips = db.delete_bans_by_nickname("spammer").await.unwrap();

        assert_eq!(deleted_ips.len(), 2);
        assert!(deleted_ips.contains(&"192.168.1.100".to_string()));
        assert!(deleted_ips.contains(&"192.168.1.101".to_string()));

        // The "other" nickname's ban is untouched.
        assert!(db.is_ip_banned("192.168.1.102").await.unwrap());
    }

    #[tokio::test]
    async fn test_has_bans_for_nickname() {
        let pool = create_test_db().await;
        let db = BanDb::new(pool);

        assert!(!db.has_bans_for_nickname("spammer").await.unwrap());

        db.create_or_update_ban("192.168.1.100", Some("spammer"), None, "admin", None)
            .await
            .expect("create ban");

        assert!(db.has_bans_for_nickname("spammer").await.unwrap());
    }

    #[tokio::test]
    async fn test_bans_by_nickname_is_case_insensitive() {
        let pool = create_test_db().await;
        let db = BanDb::new(pool);

        // Mixed-case nickname at write time is lowered to canonical form
        let record = db
            .create_or_update_ban("192.168.1.100", Some("Spammer"), None, "Admin", None)
            .await
            .expect("create ban");
        assert_eq!(record.nickname, Some("spammer".to_string()));
        // created_by is display-only; preserved as-typed
        assert_eq!(record.created_by, "Admin");

        // Lookup / delete with arbitrary case still finds the row
        assert!(db.has_bans_for_nickname("spammer").await.unwrap());
        assert!(db.has_bans_for_nickname("SPAMMER").await.unwrap());

        let deleted = db.delete_bans_by_nickname("Spammer").await.unwrap();
        assert_eq!(deleted, vec!["192.168.1.100".to_string()]);
        assert!(!db.has_bans_for_nickname("spammer").await.unwrap());
    }

    #[tokio::test]
    async fn test_list_active_bans() {
        let pool = create_test_db().await;
        let db = BanDb::new(pool);

        db.create_or_update_ban("192.168.1.100", Some("alice"), None, "admin", None)
            .await
            .expect("create ban 1");
        db.create_or_update_ban("192.168.1.101", None, Some("flooding"), "admin", None)
            .await
            .expect("create ban 2");

        let expired = BanDb::now() - 1;
        db.create_or_update_ban("192.168.1.102", None, None, "admin", Some(expired))
            .await
            .expect("create expired ban");

        let bans = db.list_active_bans().await.unwrap();

        // The expired ban is excluded.
        assert_eq!(bans.len(), 2);
    }

    #[tokio::test]
    async fn test_cleanup_expired_bans() {
        let pool = create_test_db().await;
        let db = BanDb::new(pool);

        let expired = BanDb::now() - 1;
        db.create_or_update_ban("192.168.1.100", None, None, "admin", Some(expired))
            .await
            .expect("create expired ban 1");
        db.create_or_update_ban("192.168.1.101", None, None, "admin", Some(expired))
            .await
            .expect("create expired ban 2");

        db.create_or_update_ban("192.168.1.102", None, None, "admin", None)
            .await
            .expect("create permanent ban");

        let future = BanDb::now() + 3600;
        db.create_or_update_ban("192.168.1.103", None, None, "admin", Some(future))
            .await
            .expect("create future ban");

        let deleted = db.cleanup_expired_bans().await.unwrap();
        assert_eq!(deleted, 2);

        // Permanent and future bans survive cleanup.
        let bans = db.list_active_bans().await.unwrap();
        assert_eq!(bans.len(), 2);
    }

    #[tokio::test]
    #[should_panic(expected = "ip_or_cidr must be canonical")]
    async fn test_create_or_update_ban_panics_on_non_canonical_ip() {
        // Handler is the canonicalization funnel; the DB trusts its callers
        // to pre-canonicalize. The debug_assert catches a future caller that
        // skips that funnel.
        let pool = create_test_db().await;
        let db = BanDb::new(pool);
        let _ = db
            .create_or_update_ban("2001:DB8::1", None, None, "admin", None)
            .await;
    }
}
