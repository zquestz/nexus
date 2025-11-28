//! Shared test utilities for database tests

use sqlx::SqlitePool;

// ========================================================================
// Test-only SQL Constants
// ========================================================================

/// Count admin users in the database
///
/// **Parameters:** None
///
/// **Returns:** `(count: i64)` - Number of admin users
///
/// **Note:** Used in test utilities to verify admin count in race condition tests.
const SQL_COUNT_ADMINS: &str = "SELECT COUNT(*) FROM users WHERE is_admin = 1";

/// Create an in-memory test database with migrations applied
pub async fn create_test_db() -> SqlitePool {
    let pool = SqlitePool::connect(":memory:")
        .await
        .expect("Failed to create in-memory database");

    // Run migrations
    sqlx::migrate!("./migrations")
        .run(&pool)
        .await
        .expect("Failed to run migrations");

    pool
}

/// Count the number of admin users in the database
///
/// This is a test helper function used to verify admin count in race condition
/// tests and last-admin protection tests. It performs a direct SQL query to
/// count users where `is_admin = 1`.
///
/// # Arguments
///
/// * `pool` - Reference to the SQLite connection pool
///
/// # Returns
///
/// The number of admin users currently in the database.
///
/// # Usage
///
/// Used primarily in tests that verify:
/// - Last admin cannot be deleted
/// - Last admin cannot be demoted
/// - Race conditions don't leave zero admins
pub async fn count_admins(pool: &SqlitePool) -> i64 {
    let (count,): (i64,) = sqlx::query_as(SQL_COUNT_ADMINS)
        .fetch_one(pool)
        .await
        .unwrap();
    count
}
