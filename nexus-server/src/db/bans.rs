//! IP ban database operations

use std::net::IpAddr;
use std::time::{SystemTime, UNIX_EPOCH};

use ipnet::IpNet;
use sqlx::sqlite::SqlitePool;

use crate::db::sql;

/// A ban record from the database
#[derive(Debug, Clone)]
pub struct BanRecord {
    #[allow(dead_code)]
    pub id: i64,
    pub ip_address: String,
    pub nickname: Option<String>,
    pub reason: Option<String>,
    pub created_by: String,
    pub created_at: i64,
    pub expires_at: Option<i64>,
}

/// Row type for ban queries
type BanRow = (
    i64,
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
            id: row.0,
            ip_address: row.1,
            nickname: row.2,
            reason: row.3,
            created_by: row.4,
            created_at: row.5,
            expires_at: row.6,
        }
    }
}

/// Database access for ban operations
#[derive(Clone)]
pub struct BanDb {
    pool: SqlitePool,
}

impl BanDb {
    /// Create a new BanDb instance
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    /// Get current Unix timestamp
    fn now() -> i64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time before Unix epoch")
            .as_secs() as i64
    }

    /// Create or update an IP ban (upsert)
    ///
    /// If the IP already exists, all fields are updated.
    /// Returns the created/updated ban record.
    pub async fn create_or_update_ban(
        &self,
        ip_address: &str,
        nickname: Option<&str>,
        reason: Option<&str>,
        created_by: &str,
        expires_at: Option<i64>,
    ) -> Result<BanRecord, sqlx::Error> {
        let now = Self::now();

        sqlx::query(sql::SQL_UPSERT_BAN)
            .bind(ip_address)
            .bind(nickname)
            .bind(reason)
            .bind(created_by)
            .bind(now)
            .bind(expires_at)
            .execute(&self.pool)
            .await?;

        // Fetch the record we just created/updated (without expiry filter)
        self.get_ban_by_ip_unfiltered(ip_address)
            .await?
            .ok_or(sqlx::Error::RowNotFound)
    }

    /// Get a ban by IP address (regardless of expiry status)
    ///
    /// Used internally after creating/updating a ban to return the record.
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

    /// Check if a ban exists for a given IP/CIDR (regardless of expiry)
    ///
    /// Used in tests to verify bans were created/deleted.
    #[cfg(test)]
    pub async fn ban_exists(&self, ip_address: &str) -> Result<bool, sqlx::Error> {
        Ok(self.get_ban_by_ip_unfiltered(ip_address).await?.is_some())
    }

    /// Check if an IP is currently banned (not expired)
    ///
    /// Used in tests to verify ban expiry behavior.
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

    /// Get a ban by IP address (only if not expired)
    ///
    /// Used in tests to verify ban details.
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

    /// Delete a ban by IP address
    ///
    /// Returns true if a ban was deleted, false if no ban existed.
    pub async fn delete_ban_by_ip(&self, ip_address: &str) -> Result<bool, sqlx::Error> {
        let result = sqlx::query(sql::SQL_DELETE_BAN_BY_IP)
            .bind(ip_address)
            .execute(&self.pool)
            .await?;

        Ok(result.rows_affected() > 0)
    }

    /// Delete all bans with a given nickname annotation
    ///
    /// Returns the list of IP addresses that were unbanned.
    pub async fn delete_bans_by_nickname(
        &self,
        nickname: &str,
    ) -> Result<Vec<String>, sqlx::Error> {
        // First, get the IPs we're about to delete
        let rows: Vec<(String,)> = sqlx::query_as(sql::SQL_SELECT_IPS_BY_NICKNAME)
            .bind(nickname)
            .fetch_all(&self.pool)
            .await?;

        let ips: Vec<String> = rows.into_iter().map(|(ip,)| ip).collect();

        if !ips.is_empty() {
            // Delete the bans
            sqlx::query(sql::SQL_DELETE_BANS_BY_NICKNAME)
                .bind(nickname)
                .execute(&self.pool)
                .await?;
        }

        Ok(ips)
    }

    /// Check if any bans exist with a given nickname annotation
    pub async fn has_bans_for_nickname(&self, nickname: &str) -> Result<bool, sqlx::Error> {
        let row: (i64,) = sqlx::query_as(sql::SQL_COUNT_BANS_BY_NICKNAME)
            .bind(nickname)
            .fetch_one(&self.pool)
            .await?;

        Ok(row.0 > 0)
    }

    /// List all active (non-expired) bans
    ///
    /// Results are sorted by creation time (newest first).
    pub async fn list_active_bans(&self) -> Result<Vec<BanRecord>, sqlx::Error> {
        let now = Self::now();

        let rows: Vec<BanRow> = sqlx::query_as(sql::SQL_SELECT_ACTIVE_BANS)
            .bind(now)
            .fetch_all(&self.pool)
            .await?;

        Ok(rows.into_iter().map(BanRecord::from).collect())
    }

    /// Delete all expired bans
    ///
    /// Returns the number of bans deleted.
    /// Called on server startup to clean up stale entries.
    pub async fn cleanup_expired_bans(&self) -> Result<u64, sqlx::Error> {
        let now = Self::now();

        let result = sqlx::query(sql::SQL_DELETE_EXPIRED_BANS)
            .bind(now)
            .execute(&self.pool)
            .await?;

        Ok(result.rows_affected())
    }

    /// Load all active (non-expired) bans for cache initialization
    ///
    /// This is used at server startup to populate the in-memory ban cache.
    /// Results include both single IPs and CIDR ranges.
    pub async fn load_all_active_bans(&self) -> Result<Vec<BanRecord>, sqlx::Error> {
        self.list_active_bans().await
    }

    /// Delete all bans whose IP/CIDR is contained within a given CIDR range
    ///
    /// This is used when unbanning a CIDR range to also remove any single IPs
    /// or smaller ranges that fall within the unbanned range.
    ///
    /// Returns the list of IP/CIDR strings that were deleted.
    pub async fn delete_bans_in_range(&self, range: &IpNet) -> Result<Vec<String>, sqlx::Error> {
        // Get all active bans
        let all_bans = self.list_active_bans().await?;

        let mut deleted = Vec::new();

        for ban in all_bans {
            // Try to parse the ban's IP/CIDR
            let ban_net = if let Ok(net) = ban.ip_address.parse::<IpNet>() {
                net
            } else if let Ok(ip) = ban.ip_address.parse::<IpAddr>() {
                // Convert single IP to /32 or /128
                match ip {
                    IpAddr::V4(v4) => IpNet::V4(ipnet::Ipv4Net::new(v4, 32).expect("valid prefix")),
                    IpAddr::V6(v6) => {
                        IpNet::V6(ipnet::Ipv6Net::new(v6, 128).expect("valid prefix"))
                    }
                }
            } else {
                // Can't parse, skip
                continue;
            };

            // Check if ban is contained within the range
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

            if is_contained {
                // Delete this ban
                if self.delete_ban_by_ip(&ban.ip_address).await? {
                    deleted.push(ban.ip_address);
                }
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

        let expires = BanDb::now() + 3600; // 1 hour from now

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

        // Create initial ban
        db.create_or_update_ban(
            "192.168.1.100",
            Some("alice"),
            Some("reason1"),
            "admin1",
            None,
        )
        .await
        .expect("create ban");

        // Update same IP
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

        // Create an already-expired ban
        let expired = BanDb::now() - 1;

        db.create_or_update_ban("192.168.1.100", None, None, "admin", Some(expired))
            .await
            .expect("create ban");

        // Should not be considered banned
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

        // Deleting again returns false
        let deleted = db.delete_ban_by_ip("192.168.1.100").await.unwrap();
        assert!(!deleted);
    }

    #[tokio::test]
    async fn test_delete_bans_by_nickname() {
        let pool = create_test_db().await;
        let db = BanDb::new(pool);

        // Create multiple bans with same nickname
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

        // "other" ban should still exist
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
    async fn test_list_active_bans() {
        let pool = create_test_db().await;
        let db = BanDb::new(pool);

        // Create some bans
        db.create_or_update_ban("192.168.1.100", Some("alice"), None, "admin", None)
            .await
            .expect("create ban 1");
        db.create_or_update_ban("192.168.1.101", None, Some("flooding"), "admin", None)
            .await
            .expect("create ban 2");

        // Create an expired ban
        let expired = BanDb::now() - 1;
        db.create_or_update_ban("192.168.1.102", None, None, "admin", Some(expired))
            .await
            .expect("create expired ban");

        let bans = db.list_active_bans().await.unwrap();

        // Should only return 2 active bans (expired one excluded)
        assert_eq!(bans.len(), 2);
    }

    #[tokio::test]
    async fn test_cleanup_expired_bans() {
        let pool = create_test_db().await;
        let db = BanDb::new(pool);

        // Create expired bans
        let expired = BanDb::now() - 1;
        db.create_or_update_ban("192.168.1.100", None, None, "admin", Some(expired))
            .await
            .expect("create expired ban 1");
        db.create_or_update_ban("192.168.1.101", None, None, "admin", Some(expired))
            .await
            .expect("create expired ban 2");

        // Create a permanent ban
        db.create_or_update_ban("192.168.1.102", None, None, "admin", None)
            .await
            .expect("create permanent ban");

        // Create a future ban
        let future = BanDb::now() + 3600;
        db.create_or_update_ban("192.168.1.103", None, None, "admin", Some(future))
            .await
            .expect("create future ban");

        let deleted = db.cleanup_expired_bans().await.unwrap();
        assert_eq!(deleted, 2);

        // Permanent and future bans should still exist
        let bans = db.list_active_bans().await.unwrap();
        assert_eq!(bans.len(), 2);
    }
}
