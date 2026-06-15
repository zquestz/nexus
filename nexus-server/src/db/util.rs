//! Internal helpers shared across the `db` submodules.

use nexus_common::validators::MIN_BANDWIDTH_WEIGHT;
use tracing::warn;

use crate::constants::LOG_BANDWIDTH_WEIGHT_CLAMPED;
use crate::db::sql;

#[cfg(test)]
const SQLITE_BUSY_PRIMARY: i32 = 5;
#[cfg(test)]
const SQLITE_LOCKED_PRIMARY: i32 = 6;

/// Start a SQLite transaction that reserves the write slot up front.
///
/// Use this for read-filter-write sequences that must not start as deferred
/// read transactions and later fail while upgrading to a writer.
pub(super) async fn begin_immediate(
    pool: &sqlx::SqlitePool,
) -> Result<sqlx::Transaction<'static, sqlx::Sqlite>, sqlx::Error> {
    pool.begin_with(sql::SQL_BEGIN_IMMEDIATE).await
}

/// True if `err` is a UNIQUE-constraint violation.
///
/// Used to map a `username_lower` collision (the folded-username uniqueness
/// backstop) to a "username already exists" response rather than a generic DB
/// error. Handler pre-checks under `user_state_lock` make this normally
/// unreachable; it's the atomic safety net.
pub fn is_unique_violation(err: &sqlx::Error) -> bool {
    matches!(err, sqlx::Error::Database(db) if db.is_unique_violation())
}

/// Clamp a raw `i64` bandwidth weight from a DB row into the valid `u16`
/// range. Defends against corrupt rows (operator hand-edits, restored
/// backup); all writer paths validate, so this is normally the identity.
pub(super) fn clamp_db_bandwidth_weight(value: i64) -> u16 {
    let clamped = value.clamp(MIN_BANDWIDTH_WEIGHT as i64, u16::MAX as i64) as u16;
    if i64::from(clamped) != value {
        warn!(
            raw = value,
            clamped = clamped,
            "{}",
            LOG_BANDWIDTH_WEIGHT_CLAMPED
        );
    }
    clamped
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};

    const SQL_CREATE_TEST_ROWS: &str = "CREATE TABLE rows (id INTEGER PRIMARY KEY)";
    const SQL_COUNT_TEST_ROWS: &str = "SELECT COUNT(*) FROM rows";
    const SQL_INSERT_TEST_ROW: &str = "INSERT INTO rows (id) VALUES (1)";

    #[test]
    fn in_range_values_round_trip() {
        assert_eq!(clamp_db_bandwidth_weight(1), 1);
        assert_eq!(clamp_db_bandwidth_weight(100), 100);
        assert_eq!(clamp_db_bandwidth_weight(65535), u16::MAX);
    }

    #[test]
    fn zero_clamps_up_to_min() {
        assert_eq!(clamp_db_bandwidth_weight(0), MIN_BANDWIDTH_WEIGHT);
    }

    #[test]
    fn negative_clamps_up_to_min() {
        assert_eq!(clamp_db_bandwidth_weight(-1), MIN_BANDWIDTH_WEIGHT);
        assert_eq!(clamp_db_bandwidth_weight(i64::MIN), MIN_BANDWIDTH_WEIGHT);
    }

    #[test]
    fn above_max_clamps_down_to_u16_max() {
        assert_eq!(clamp_db_bandwidth_weight(65536), u16::MAX);
        assert_eq!(clamp_db_bandwidth_weight(100_000), u16::MAX);
        assert_eq!(clamp_db_bandwidth_weight(i64::MAX), u16::MAX);
    }

    #[tokio::test]
    async fn begin_immediate_reserves_write_slot_before_first_read() {
        let temp_dir = tempfile::TempDir::new().expect("create temp dir");
        let db_path = temp_dir.path().join("test.db");
        let options = SqliteConnectOptions::new()
            .filename(&db_path)
            .create_if_missing(true)
            .busy_timeout(Duration::from_millis(1));
        let pool = SqlitePoolOptions::new()
            .max_connections(2)
            .connect_with(options)
            .await
            .expect("create sqlite pool");

        sqlx::query(SQL_CREATE_TEST_ROWS)
            .execute(&pool)
            .await
            .expect("create table");

        let mut tx = begin_immediate(&pool).await.expect("begin immediate");
        let _: (i64,) = sqlx::query_as(SQL_COUNT_TEST_ROWS)
            .fetch_one(&mut *tx)
            .await
            .expect("read inside immediate tx");

        let err = sqlx::query(SQL_INSERT_TEST_ROW)
            .execute(&pool)
            .await
            .expect_err("competing writer should not acquire write slot");
        assert!(
            is_sqlite_busy_or_locked(&err),
            "expected SQLITE_BUSY/LOCKED from competing writer, got {err:?}"
        );

        tx.commit().await.expect("commit immediate tx");

        sqlx::query(SQL_INSERT_TEST_ROW)
            .execute(&pool)
            .await
            .expect("write succeeds after immediate tx commits");
    }

    fn is_sqlite_busy_or_locked(err: &sqlx::Error) -> bool {
        let sqlx::Error::Database(db_err) = err else {
            return false;
        };

        let Some(primary) = db_err
            .code()
            .as_deref()
            .and_then(|code| code.parse::<i32>().ok())
            .map(|code| code & 0xFF)
        else {
            return false;
        };

        primary == SQLITE_BUSY_PRIMARY || primary == SQLITE_LOCKED_PRIMARY
    }
}
