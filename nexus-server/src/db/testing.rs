//! Shared test utilities for database tests

use sqlx::SqlitePool;

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
pub async fn count_admins(pool: &SqlitePool) -> i64 {
    let (count,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM users WHERE is_admin = 1")
        .fetch_one(pool)
        .await
        .unwrap();
    count
}
