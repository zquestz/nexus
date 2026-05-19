//! Shared test utilities for database tests

use sqlx::SqlitePool;

use crate::db::sql;

/// Create an in-memory test database with migrations applied
pub async fn create_test_db() -> SqlitePool {
    let pool = SqlitePool::connect(":memory:")
        .await
        .expect("Failed to create in-memory database");

    sqlx::migrate!("./migrations")
        .run(&pool)
        .await
        .expect("Failed to run migrations");

    pool
}

/// Count admin users in the database.
pub async fn count_admins(pool: &SqlitePool) -> i64 {
    let (count,): (i64,) = sqlx::query_as(sql::SQL_COUNT_ADMINS)
        .fetch_one(pool)
        .await
        .unwrap();
    count
}
