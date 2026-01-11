//! IP trusted database operations

use std::net::IpAddr;
use std::time::{SystemTime, UNIX_EPOCH};

use ipnet::IpNet;
use sqlx::sqlite::SqlitePool;

use crate::db::sql;

/// A trust record from the database
#[derive(Debug, Clone)]
pub struct TrustRecord {
    #[allow(dead_code)]
    pub id: i64,
    pub ip_address: String,
    pub nickname: Option<String>,
    pub reason: Option<String>,
    pub created_by: String,
    pub created_at: i64,
    pub expires_at: Option<i64>,
}

/// Row type for trust queries
type TrustRow = (
    i64,
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

/// Database access for trust operations
#[derive(Clone)]
pub struct TrustDb {
    pool: SqlitePool,
}

impl TrustDb {
    /// Create a new TrustDb instance
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

    /// Create or update a trusted IP entry (upsert)
    ///
    /// If the IP already exists, all fields are updated.
    /// Returns the created/updated trust record.
    pub async fn create_or_update_trust(
        &self,
        ip_address: &str,
        nickname: Option<&str>,
        reason: Option<&str>,
        created_by: &str,
        expires_at: Option<i64>,
    ) -> Result<TrustRecord, sqlx::Error> {
        let now = Self::now();

        sqlx::query(sql::SQL_UPSERT_TRUST)
            .bind(ip_address)
            .bind(nickname)
            .bind(reason)
            .bind(created_by)
            .bind(now)
            .bind(expires_at)
            .execute(&self.pool)
            .await?;

        // Fetch the record we just created/updated (without expiry filter)
        self.get_trust_by_ip_unfiltered(ip_address)
            .await?
            .ok_or(sqlx::Error::RowNotFound)
    }

    /// Get a trust entry by IP address (regardless of expiry status)
    ///
    /// Used internally after creating/updating a trust to return the record.
    async fn get_trust_by_ip_unfiltered(
        &self,
        ip_address: &str,
    ) -> Result<Option<TrustRecord>, sqlx::Error> {
        let row: Option<TrustRow> = sqlx::query_as(sql::SQL_SELECT_TRUST_BY_IP_UNFILTERED)
            .bind(ip_address)
            .fetch_optional(&self.pool)
            .await?;

        Ok(row.map(TrustRecord::from))
    }

    /// Check if a trust entry exists for a given IP/CIDR (regardless of expiry)
    ///
    /// Used in tests to verify trusts were created/deleted.
    #[cfg(test)]
    pub async fn trust_exists(&self, ip_address: &str) -> Result<bool, sqlx::Error> {
        Ok(self.get_trust_by_ip_unfiltered(ip_address).await?.is_some())
    }

    /// Check if an IP is currently trusted (not expired)
    ///
    /// Used in tests to verify trust expiry behavior.
    #[cfg(test)]
    pub async fn is_ip_trusted(&self, ip_address: &str) -> Result<bool, sqlx::Error> {
        let now = Self::now();

        let row: Option<TrustRow> = sqlx::query_as(sql::SQL_SELECT_TRUST_BY_IP)
            .bind(ip_address)
            .bind(now)
            .fetch_optional(&self.pool)
            .await?;

        Ok(row.is_some())
    }

    /// Get a trust entry by IP address (only if not expired)
    ///
    /// Used in tests to verify trust details.
    #[cfg(test)]
    pub async fn get_trust_by_ip(
        &self,
        ip_address: &str,
    ) -> Result<Option<TrustRecord>, sqlx::Error> {
        let now = Self::now();

        let row: Option<TrustRow> = sqlx::query_as(sql::SQL_SELECT_TRUST_BY_IP)
            .bind(ip_address)
            .bind(now)
            .fetch_optional(&self.pool)
            .await?;

        Ok(row.map(TrustRecord::from))
    }

    /// Delete a trust entry by IP address
    ///
    /// Returns true if a trust was deleted, false if no trust existed.
    pub async fn delete_trust_by_ip(&self, ip_address: &str) -> Result<bool, sqlx::Error> {
        let result = sqlx::query(sql::SQL_DELETE_TRUST_BY_IP)
            .bind(ip_address)
            .execute(&self.pool)
            .await?;

        Ok(result.rows_affected() > 0)
    }

    /// Delete all trusts with a given nickname annotation
    ///
    /// Returns the list of IP addresses that were untrusted.
    pub async fn delete_trusts_by_nickname(
        &self,
        nickname: &str,
    ) -> Result<Vec<String>, sqlx::Error> {
        // First, get the IPs we're about to delete
        let rows: Vec<(String,)> = sqlx::query_as(sql::SQL_SELECT_TRUSTED_IPS_BY_NICKNAME)
            .bind(nickname)
            .fetch_all(&self.pool)
            .await?;

        let ips: Vec<String> = rows.into_iter().map(|(ip,)| ip).collect();

        if !ips.is_empty() {
            // Delete the trusts
            sqlx::query(sql::SQL_DELETE_TRUSTS_BY_NICKNAME)
                .bind(nickname)
                .execute(&self.pool)
                .await?;
        }

        Ok(ips)
    }

    /// Check if any trusts exist with a given nickname annotation
    pub async fn has_trusts_for_nickname(&self, nickname: &str) -> Result<bool, sqlx::Error> {
        let row: (i64,) = sqlx::query_as(sql::SQL_COUNT_TRUSTS_BY_NICKNAME)
            .bind(nickname)
            .fetch_one(&self.pool)
            .await?;

        Ok(row.0 > 0)
    }

    /// List all active (non-expired) trusts
    ///
    /// Results are sorted by creation time (newest first).
    pub async fn list_active_trusts(&self) -> Result<Vec<TrustRecord>, sqlx::Error> {
        let now = Self::now();

        let rows: Vec<TrustRow> = sqlx::query_as(sql::SQL_SELECT_ACTIVE_TRUSTS)
            .bind(now)
            .fetch_all(&self.pool)
            .await?;

        Ok(rows.into_iter().map(TrustRecord::from).collect())
    }

    /// Delete all expired trusts
    ///
    /// Returns the number of trusts deleted.
    /// Called on server startup to clean up stale entries.
    pub async fn cleanup_expired_trusts(&self) -> Result<u64, sqlx::Error> {
        let now = Self::now();

        let result = sqlx::query(sql::SQL_DELETE_EXPIRED_TRUSTS)
            .bind(now)
            .execute(&self.pool)
            .await?;

        Ok(result.rows_affected())
    }

    /// Load all active (non-expired) trusts for cache initialization
    ///
    /// This is used at server startup to populate the in-memory cache.
    /// Results include both single IPs and CIDR ranges.
    pub async fn load_all_active_trusts(&self) -> Result<Vec<TrustRecord>, sqlx::Error> {
        self.list_active_trusts().await
    }

    /// Delete all trusts whose IP/CIDR is contained within a given CIDR range
    ///
    /// This is used when untrusting a CIDR range to also remove any single IPs
    /// or smaller ranges that fall within the untrusted range.
    ///
    /// Returns the list of IP/CIDR strings that were deleted.
    pub async fn delete_trusts_in_range(&self, range: &IpNet) -> Result<Vec<String>, sqlx::Error> {
        // Get all active trusts
        let all_trusts = self.list_active_trusts().await?;

        let mut deleted = Vec::new();

        for trust in all_trusts {
            // Try to parse the trust's IP/CIDR
            let trust_net = if let Ok(net) = trust.ip_address.parse::<IpNet>() {
                net
            } else if let Ok(ip) = trust.ip_address.parse::<IpAddr>() {
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

            // Check if trust is contained within the range
            let is_contained = match (&trust_net, range) {
                (IpNet::V4(trust_v4), IpNet::V4(range_v4)) => {
                    range_v4.contains(&trust_v4.network())
                        && trust_v4.prefix_len() >= range_v4.prefix_len()
                }
                (IpNet::V6(trust_v6), IpNet::V6(range_v6)) => {
                    range_v6.contains(&trust_v6.network())
                        && trust_v6.prefix_len() >= range_v6.prefix_len()
                }
                _ => false, // IPv4/IPv6 mismatch
            };

            if is_contained {
                // Delete this trust
                if self.delete_trust_by_ip(&trust.ip_address).await? {
                    deleted.push(trust.ip_address);
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

        let expires = TrustDb::now() + 3600; // 1 hour from now

        let trust = db
            .create_or_update_trust("10.0.0.1", None, None, "admin", Some(expires))
            .await
            .expect("create trust");

        assert_eq!(trust.ip_address, "10.0.0.1");
        assert_eq!(trust.expires_at, Some(expires));
    }

    #[tokio::test]
    async fn test_upsert_trust() {
        let pool = create_test_db().await;
        let db = TrustDb::new(pool);

        // Create initial trust
        db.create_or_update_trust(
            "192.168.1.100",
            Some("alice"),
            Some("reason1"),
            "admin1",
            None,
        )
        .await
        .expect("create trust");

        // Update same IP
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

        // Create an already-expired trust
        let expired = TrustDb::now() - 1;

        db.create_or_update_trust("192.168.1.100", None, None, "admin", Some(expired))
            .await
            .expect("create trust");

        // Should not be considered trusted
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

        // Deleting again returns false
        let deleted = db.delete_trust_by_ip("192.168.1.100").await.unwrap();
        assert!(!deleted);
    }

    #[tokio::test]
    async fn test_delete_trusts_by_nickname() {
        let pool = create_test_db().await;
        let db = TrustDb::new(pool);

        // Create multiple trusts with same nickname
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

        // "other" trust should still exist
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
    async fn test_list_active_trusts() {
        let pool = create_test_db().await;
        let db = TrustDb::new(pool);

        // Create some trusts
        db.create_or_update_trust("192.168.1.100", Some("alice"), None, "admin", None)
            .await
            .expect("create trust 1");
        db.create_or_update_trust("192.168.1.101", None, Some("office"), "admin", None)
            .await
            .expect("create trust 2");

        // Create an expired trust
        let expired = TrustDb::now() - 1;
        db.create_or_update_trust("192.168.1.102", None, None, "admin", Some(expired))
            .await
            .expect("create expired trust");

        let trusts = db.list_active_trusts().await.unwrap();

        // Should only return 2 active trusts (expired one excluded)
        assert_eq!(trusts.len(), 2);
    }

    #[tokio::test]
    async fn test_cleanup_expired_trusts() {
        let pool = create_test_db().await;
        let db = TrustDb::new(pool);

        // Create expired trusts
        let expired = TrustDb::now() - 1;
        db.create_or_update_trust("192.168.1.100", None, None, "admin", Some(expired))
            .await
            .expect("create expired trust 1");
        db.create_or_update_trust("192.168.1.101", None, None, "admin", Some(expired))
            .await
            .expect("create expired trust 2");

        // Create a permanent trust
        db.create_or_update_trust("192.168.1.102", None, None, "admin", None)
            .await
            .expect("create permanent trust");

        // Create a future trust
        let future = TrustDb::now() + 3600;
        db.create_or_update_trust("192.168.1.103", None, None, "admin", Some(future))
            .await
            .expect("create future trust");

        let deleted = db.cleanup_expired_trusts().await.unwrap();
        assert_eq!(deleted, 2);

        // Permanent and future trusts should still exist
        let trusts = db.list_active_trusts().await.unwrap();
        assert_eq!(trusts.len(), 2);
    }
}
