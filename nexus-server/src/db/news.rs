//! News database operations

use chrono::Utc;
use sqlx::sqlite::SqlitePool;

use crate::db::sql;

/// A news item from the database
#[derive(Debug, Clone)]
pub struct NewsRecord {
    pub id: i64,
    pub body: Option<String>,
    pub image: Option<String>,
    pub author_id: i64,
    pub author_username: String,
    pub author_is_admin: bool,
    pub created_at: String,
    pub updated_at: Option<String>,
}

/// Row type for news queries with author join
type NewsRow = (
    i64,
    Option<String>,
    Option<String>,
    i64,
    String,
    bool,
    String,
    Option<String>,
);

impl From<NewsRow> for NewsRecord {
    fn from(row: NewsRow) -> Self {
        Self {
            id: row.0,
            body: row.1,
            image: row.2,
            author_id: row.3,
            author_username: row.4,
            author_is_admin: row.5,
            created_at: row.6,
            updated_at: row.7,
        }
    }
}

/// Database access for news operations
#[derive(Clone)]
pub struct NewsDb {
    pool: SqlitePool,
}

impl NewsDb {
    /// Create a new NewsDb instance
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    /// Get all news items ordered by creation time (oldest first)
    pub async fn get_all_news(&self) -> Result<Vec<NewsRecord>, sqlx::Error> {
        let rows: Vec<NewsRow> = sqlx::query_as(sql::SQL_SELECT_ALL_NEWS)
            .fetch_all(&self.pool)
            .await?;

        Ok(rows.into_iter().map(NewsRecord::from).collect())
    }

    /// Get a single news item by ID
    pub async fn get_news_by_id(&self, id: i64) -> Result<Option<NewsRecord>, sqlx::Error> {
        let row: Option<NewsRow> = sqlx::query_as(sql::SQL_SELECT_NEWS_BY_ID)
            .bind(id)
            .fetch_optional(&self.pool)
            .await?;

        Ok(row.map(NewsRecord::from))
    }

    /// Create a new news item
    ///
    /// Returns the created news record.
    pub async fn create_news(
        &self,
        body: Option<&str>,
        image: Option<&str>,
        author_id: i64,
    ) -> Result<NewsRecord, sqlx::Error> {
        let now = Utc::now().to_rfc3339();

        // Normalize empty strings to None
        let body = body.filter(|s| !s.is_empty());
        let image = image.filter(|s| !s.is_empty());

        let result = sqlx::query(sql::SQL_INSERT_NEWS)
            .bind(body)
            .bind(image)
            .bind(author_id)
            .bind(&now)
            .execute(&self.pool)
            .await?;

        let id = result.last_insert_rowid();

        // Fetch the created record with author info
        self.get_news_by_id(id)
            .await?
            .ok_or_else(|| sqlx::Error::RowNotFound)
    }

    /// Update a news item
    ///
    /// Returns the updated news record.
    pub async fn update_news(
        &self,
        id: i64,
        body: Option<&str>,
        image: Option<&str>,
    ) -> Result<Option<NewsRecord>, sqlx::Error> {
        let now = Utc::now().to_rfc3339();

        // Normalize empty strings to None
        let body = body.filter(|s| !s.is_empty());
        let image = image.filter(|s| !s.is_empty());

        let result = sqlx::query(sql::SQL_UPDATE_NEWS)
            .bind(body)
            .bind(image)
            .bind(&now)
            .bind(id)
            .execute(&self.pool)
            .await?;

        if result.rows_affected() == 0 {
            return Ok(None);
        }

        // Fetch the updated record with author info
        self.get_news_by_id(id).await
    }

    /// Delete a news item
    ///
    /// Returns true if the item was deleted, false if it didn't exist.
    pub async fn delete_news(&self, id: i64) -> Result<bool, sqlx::Error> {
        let result = sqlx::query(sql::SQL_DELETE_NEWS)
            .bind(id)
            .execute(&self.pool)
            .await?;

        Ok(result.rows_affected() > 0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Permissions;
    use crate::db::testing::create_test_db;

    #[tokio::test]
    async fn test_create_news_with_body() {
        let pool = create_test_db().await;
        let news_db = NewsDb::new(pool.clone());
        let users_db = crate::db::UserDb::new(pool.clone());

        // Create a user first
        let user = users_db
            .create_user("alice", "hash", false, true, &Permissions::new())
            .await
            .unwrap();

        // Create news with body only
        let news = news_db
            .create_news(Some("# Hello\n\nThis is news!"), None, user.id)
            .await
            .unwrap();

        assert_eq!(news.body, Some("# Hello\n\nThis is news!".to_string()));
        assert!(news.image.is_none());
        assert_eq!(news.author_username, "alice");
        assert!(!news.author_is_admin);
        assert!(news.updated_at.is_none());
    }

    #[tokio::test]
    async fn test_create_news_with_image() {
        let pool = create_test_db().await;
        let news_db = NewsDb::new(pool.clone());
        let users_db = crate::db::UserDb::new(pool.clone());

        let user = users_db
            .create_user("bob", "hash", true, true, &Permissions::new())
            .await
            .unwrap();

        let news = news_db
            .create_news(None, Some("data:image/png;base64,abc123"), user.id)
            .await
            .unwrap();

        assert!(news.body.is_none());
        assert_eq!(news.image, Some("data:image/png;base64,abc123".to_string()));
        assert_eq!(news.author_username, "bob");
        assert!(news.author_is_admin);
    }

    #[tokio::test]
    async fn test_create_news_with_both() {
        let pool = create_test_db().await;
        let news_db = NewsDb::new(pool.clone());
        let users_db = crate::db::UserDb::new(pool.clone());

        let user = users_db
            .create_user("charlie", "hash", false, true, &Permissions::new())
            .await
            .unwrap();

        let news = news_db
            .create_news(
                Some("Check out this image!"),
                Some("data:image/png;base64,xyz"),
                user.id,
            )
            .await
            .unwrap();

        assert_eq!(news.body, Some("Check out this image!".to_string()));
        assert_eq!(news.image, Some("data:image/png;base64,xyz".to_string()));
    }

    #[tokio::test]
    async fn test_get_all_news_ordered() {
        let pool = create_test_db().await;
        let news_db = NewsDb::new(pool.clone());
        let users_db = crate::db::UserDb::new(pool.clone());

        let user = users_db
            .create_user("alice", "hash", false, true, &Permissions::new())
            .await
            .unwrap();

        // Create multiple news items
        let news1 = news_db
            .create_news(Some("First post"), None, user.id)
            .await
            .unwrap();
        let news2 = news_db
            .create_news(Some("Second post"), None, user.id)
            .await
            .unwrap();
        let news3 = news_db
            .create_news(Some("Third post"), None, user.id)
            .await
            .unwrap();

        let all_news = news_db.get_all_news().await.unwrap();

        assert_eq!(all_news.len(), 3);
        // Should be ordered oldest first
        assert_eq!(all_news[0].id, news1.id);
        assert_eq!(all_news[1].id, news2.id);
        assert_eq!(all_news[2].id, news3.id);
    }

    #[tokio::test]
    async fn test_get_news_by_id() {
        let pool = create_test_db().await;
        let news_db = NewsDb::new(pool.clone());
        let users_db = crate::db::UserDb::new(pool.clone());

        let user = users_db
            .create_user("alice", "hash", false, true, &Permissions::new())
            .await
            .unwrap();

        let created = news_db
            .create_news(Some("Test post"), None, user.id)
            .await
            .unwrap();

        let fetched = news_db.get_news_by_id(created.id).await.unwrap();
        assert!(fetched.is_some());
        let fetched = fetched.unwrap();
        assert_eq!(fetched.id, created.id);
        assert_eq!(fetched.body, Some("Test post".to_string()));

        // Non-existent ID
        let not_found = news_db.get_news_by_id(99999).await.unwrap();
        assert!(not_found.is_none());
    }

    #[tokio::test]
    async fn test_update_news() {
        let pool = create_test_db().await;
        let news_db = NewsDb::new(pool.clone());
        let users_db = crate::db::UserDb::new(pool.clone());

        let user = users_db
            .create_user("alice", "hash", false, true, &Permissions::new())
            .await
            .unwrap();

        let created = news_db
            .create_news(Some("Original"), None, user.id)
            .await
            .unwrap();

        assert!(created.updated_at.is_none());

        let updated = news_db
            .update_news(
                created.id,
                Some("Updated content"),
                Some("data:image/png;base64,new"),
            )
            .await
            .unwrap()
            .unwrap();

        assert_eq!(updated.body, Some("Updated content".to_string()));
        assert_eq!(updated.image, Some("data:image/png;base64,new".to_string()));
        assert!(updated.updated_at.is_some());
    }

    #[tokio::test]
    async fn test_update_nonexistent_news() {
        let pool = create_test_db().await;
        let news_db = NewsDb::new(pool.clone());

        let result = news_db
            .update_news(99999, Some("Content"), None)
            .await
            .unwrap();

        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_delete_news() {
        let pool = create_test_db().await;
        let news_db = NewsDb::new(pool.clone());
        let users_db = crate::db::UserDb::new(pool.clone());

        let user = users_db
            .create_user("alice", "hash", false, true, &Permissions::new())
            .await
            .unwrap();

        let news = news_db
            .create_news(Some("To be deleted"), None, user.id)
            .await
            .unwrap();

        let deleted = news_db.delete_news(news.id).await.unwrap();
        assert!(deleted);

        // Verify it's gone
        let fetched = news_db.get_news_by_id(news.id).await.unwrap();
        assert!(fetched.is_none());

        // Deleting again should return false
        let deleted_again = news_db.delete_news(news.id).await.unwrap();
        assert!(!deleted_again);
    }

    #[tokio::test]
    async fn test_cascade_delete_on_user_deletion() {
        let pool = create_test_db().await;
        let news_db = NewsDb::new(pool.clone());
        let users_db = crate::db::UserDb::new(pool.clone());

        let user = users_db
            .create_user("alice", "hash", false, true, &Permissions::new())
            .await
            .unwrap();

        let news = news_db
            .create_news(Some("User's post"), None, user.id)
            .await
            .unwrap();

        // Delete the user
        users_db.delete_user(user.id).await.unwrap();

        // News should be cascade deleted
        let fetched = news_db.get_news_by_id(news.id).await.unwrap();
        assert!(fetched.is_none());
    }

    #[tokio::test]
    async fn test_empty_string_normalized_to_none() {
        let pool = create_test_db().await;
        let news_db = NewsDb::new(pool.clone());
        let users_db = crate::db::UserDb::new(pool.clone());

        let user = users_db
            .create_user("alice", "hash", false, true, &Permissions::new())
            .await
            .unwrap();

        // Create with empty string body (should be normalized to None)
        // but with valid image
        let news = news_db
            .create_news(Some(""), Some("data:image/png;base64,abc"), user.id)
            .await
            .unwrap();

        assert!(news.body.is_none());
        assert!(news.image.is_some());

        // Update to clear image but set body
        let updated = news_db
            .update_news(news.id, Some("New body"), Some(""))
            .await
            .unwrap()
            .unwrap();

        assert_eq!(updated.body, Some("New body".to_string()));
        assert!(updated.image.is_none());
    }
}
