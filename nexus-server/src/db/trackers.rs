//! Tracker database operations.
//!
//! Wraps the `trackers` table — durable configuration for trackers the
//! server's tracker tasks should register with. Connection status,
//! last-error, pending-fingerprint, and tracker-supplied refresh
//! interval are runtime state and live in memory in the tracker task
//! manager, not here.

use chrono::Utc;
use sqlx::sqlite::SqlitePool;

use crate::db::sql;

/// SQLite extended result code for `SQLITE_CONSTRAINT_UNIQUE`.
///
/// sqlx surfaces this via `DatabaseError::code()` as the string
/// `"2067"`. We compare against this rather than substring-matching
/// on the error message so the check is robust to sqlx / sqlite
/// version changes in error formatting.
const SQLITE_CONSTRAINT_UNIQUE: &str = "2067";

/// SQLite primary result code `SQLITE_BUSY` — another writer holds
/// the lock; the operation should be retried after a short delay.
/// Always transient.
const SQLITE_BUSY_PRIMARY: i32 = 5;

/// SQLite primary result code `SQLITE_LOCKED` — the write was
/// blocked by a different connection's pending transaction. Transient
/// for the same reason as `SQLITE_BUSY`.
const SQLITE_LOCKED_PRIMARY: i32 = 6;

/// Whether `err` represents a transient (retryable) DB failure.
///
/// Used by callers that need to choose between "back off and retry"
/// (lock contention, rare on SQLite with our single-pool setup) and
/// "give up and surface to admin" (corruption, missing schema, disk
/// I/O). Anything that isn't a recognized transient code is treated
/// as structural.
///
/// sqlx-sqlite surfaces the *extended* result code (e.g. `"261"` for
/// `SQLITE_BUSY_RECOVERY` or `"773"` for `SQLITE_BUSY_TIMEOUT`). The
/// primary code lives in the low byte, so every `SQLITE_BUSY_*` /
/// `SQLITE_LOCKED_*` extended variant is recognized via the mask
/// rather than enumerated.
#[must_use]
pub fn is_transient_db_error(err: &sqlx::Error) -> bool {
    let sqlx::Error::Database(db_err) = err else {
        return false;
    };
    // Bias: when we can't classify the error (no code, unparseable
    // code, or code that doesn't match a known transient primary), we
    // return `false` so the caller treats it as structural. The
    // opposite bias would let a permanently-broken DB drive an
    // infinite retry loop, where the operator never sees the failure.
    // Loud-failure-on-unknown is the safer operational default.
    let Some(primary) = db_err
        .code()
        .as_deref()
        .and_then(|s| s.parse::<i32>().ok())
        .map(|code| code & 0xFF)
    else {
        return false;
    };
    primary == SQLITE_BUSY_PRIMARY || primary == SQLITE_LOCKED_PRIMARY
}

// SQLite reports UNIQUE constraint violations on expression-based
// indexes by index name:
//   `"UNIQUE constraint failed: index '<index_name>'"`
// Both the endpoint and name indexes are expression-based
// (`LOWER(address), port` and `LOWER(name)` respectively), so we
// probe by index name in either case.
const ENDPOINT_FAILURE_MARKER: &str = "idx_trackers_endpoint";
const NAME_LOWER_FAILURE_MARKER: &str = "idx_trackers_name_lower";

/// Errors specific to the tracker DB layer.
///
/// `create` and `update` translate sqlx's generic
/// `sqlx::Error::Database(...)` variants into typed states the handler
/// layer can match on directly, so handlers don't have to inspect
/// error message strings. Anything that isn't a recognized UNIQUE
/// violation falls through to `Other(sqlx::Error)`.
#[derive(Debug)]
pub enum TrackerDbError {
    /// Another row already owns the `(address, port)` endpoint.
    EndpointDuplicate,
    /// Another row already uses this name (case-insensitive collision
    /// on `LOWER(name)`).
    NameDuplicate,
    /// The configured-trackers row cap was already reached. Returned
    /// by `create()` when the atomic-insert WHERE clause matched zero
    /// rows.
    TooMany,
    /// Any other DB error — pool exhaustion, schema corruption,
    /// unrecognized constraint, etc. The handler logs it and sends a
    /// generic translated database-error response.
    Other(sqlx::Error),
}

impl std::fmt::Display for TrackerDbError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::EndpointDuplicate => {
                f.write_str("another tracker is already configured at this address and port")
            }
            Self::NameDuplicate => {
                f.write_str("another tracker is already configured with this name")
            }
            Self::TooMany => f.write_str("tracker row cap reached"),
            Self::Other(e) => write!(f, "tracker database error: {e}"),
        }
    }
}

impl std::error::Error for TrackerDbError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Other(e) => Some(e),
            _ => None,
        }
    }
}

impl From<sqlx::Error> for TrackerDbError {
    fn from(err: sqlx::Error) -> Self {
        if let sqlx::Error::Database(ref db_err) = err
            && db_err.code().as_deref() == Some(SQLITE_CONSTRAINT_UNIQUE)
        {
            let msg = db_err.message();
            if msg.contains(ENDPOINT_FAILURE_MARKER) {
                return Self::EndpointDuplicate;
            }
            if msg.contains(NAME_LOWER_FAILURE_MARKER) {
                return Self::NameDuplicate;
            }
        }
        Self::Other(err)
    }
}

/// A tracker configuration row.
///
/// `port` is stored as INTEGER in SQLite but mapped to `u16` in Rust
/// because protocol-layer validation rejects out-of-range values
/// before insert. A stored value outside `u16` range would indicate
/// database corruption (manual edit, broken migration); the row
/// mapping panics with a descriptive message in that case.
#[derive(Clone, PartialEq, Eq)]
pub struct TrackerRecord {
    pub id: i64,
    pub address: String,
    pub port: u16,
    /// TOFU-pinned cert fingerprint in canonical form. `None` until
    /// the tracker task accepts a fingerprint on first connect (or
    /// the admin accepts a pending fingerprint after rotation).
    pub fingerprint: Option<String>,
    /// Registration password. `None` when the tracker is open.
    pub password: Option<String>,
    pub name: String,
    pub enabled: bool,
    /// Unix epoch seconds.
    pub created_at: i64,
    /// Unix epoch seconds.
    pub updated_at: i64,
}

impl std::fmt::Debug for TrackerRecord {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Redact the registration password — defense in depth so a
        // future `?record` log line can't accidentally leak it. `None`
        // is shown verbatim so a reader can distinguish "open tracker"
        // from "password set but redacted".
        let password: &dyn std::fmt::Debug = match &self.password {
            Some(_) => &"<REDACTED>",
            None => &Option::<String>::None,
        };
        f.debug_struct("TrackerRecord")
            .field("id", &self.id)
            .field("address", &self.address)
            .field("port", &self.port)
            .field("fingerprint", &self.fingerprint)
            .field("password", password)
            .field("name", &self.name)
            .field("enabled", &self.enabled)
            .field("created_at", &self.created_at)
            .field("updated_at", &self.updated_at)
            .finish()
    }
}

/// Row tuple matching `(id, address, port, fingerprint, password,
/// name, enabled, created_at, updated_at)`.
type TrackerRow = (
    i64,
    String,
    i64,
    Option<String>,
    Option<String>,
    String,
    bool,
    i64,
    i64,
);

impl From<TrackerRow> for TrackerRecord {
    fn from(row: TrackerRow) -> Self {
        let port: u16 = row
            .2
            .try_into()
            .expect("trackers.port out of u16 range — database corruption");
        Self {
            id: row.0,
            address: row.1,
            port,
            fingerprint: row.3,
            password: row.4,
            name: row.5,
            enabled: row.6,
            created_at: row.7,
            updated_at: row.8,
        }
    }
}

/// Parameters for [`TrackerDb::create`]. Borrowed `&str` so call sites
/// don't need to allocate; values move into the row at insert time.
pub struct CreateTrackerParams<'a> {
    pub address: &'a str,
    pub port: u16,
    pub fingerprint: Option<&'a str>,
    pub password: Option<&'a str>,
    pub name: &'a str,
    pub enabled: bool,
}

/// Parameters for [`TrackerDb::update`]. Same shape as
/// [`CreateTrackerParams`] because the admin UI replaces the whole
/// configuration row on save (no partial-update protocol). Kept as a
/// distinct type for symmetry with `CreateUserParams` /
/// `UpdateUserParams` and to leave room for divergence later.
pub struct UpdateTrackerParams<'a> {
    pub address: &'a str,
    pub port: u16,
    pub fingerprint: Option<&'a str>,
    pub password: Option<&'a str>,
    pub name: &'a str,
    pub enabled: bool,
}

/// Database access for tracker configuration.
#[derive(Clone)]
pub struct TrackerDb {
    pool: SqlitePool,
}

impl TrackerDb {
    /// Construct a new wrapper around the shared connection pool.
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    /// All tracker rows in insertion order (by `id`).
    pub async fn list_all(&self) -> Result<Vec<TrackerRecord>, sqlx::Error> {
        let rows: Vec<TrackerRow> = sqlx::query_as(sql::SQL_SELECT_ALL_TRACKERS)
            .fetch_all(&self.pool)
            .await?;
        Ok(rows.into_iter().map(TrackerRecord::from).collect())
    }

    /// Fetch a single tracker by id.
    pub async fn get_by_id(&self, id: i64) -> Result<Option<TrackerRecord>, sqlx::Error> {
        let row: Option<TrackerRow> = sqlx::query_as(sql::SQL_SELECT_TRACKER_BY_ID)
            .bind(id)
            .fetch_optional(&self.pool)
            .await?;
        Ok(row.map(TrackerRecord::from))
    }

    /// Insert a new tracker row. Returns the created record.
    ///
    /// On UNIQUE-constraint violation, returns
    /// [`TrackerDbError::EndpointDuplicate`] (same `(address, port)`)
    /// or [`TrackerDbError::NameDuplicate`] (same case-insensitive
    /// name). When the table already has
    /// [`MAX_TRACKERS_PER_SERVER`](nexus_common::framing::MAX_TRACKERS_PER_SERVER)
    /// rows, returns [`TrackerDbError::TooMany`] (the cap check is
    /// atomic with the insert via `INSERT … SELECT … WHERE`). Other
    /// failures bubble up as [`TrackerDbError::Other`].
    pub async fn create(
        &self,
        params: CreateTrackerParams<'_>,
    ) -> Result<TrackerRecord, TrackerDbError> {
        let now = Utc::now().timestamp();
        // `MAX_TRACKERS_PER_SERVER` is a small `usize` const (64);
        // casts to i64 cleanly, no realistic overflow path.
        let cap = nexus_common::framing::MAX_TRACKERS_PER_SERVER as i64;
        let result = sqlx::query(sql::SQL_INSERT_TRACKER)
            .bind(params.address)
            .bind(i64::from(params.port))
            .bind(params.fingerprint)
            .bind(params.password)
            .bind(params.name)
            .bind(params.enabled)
            .bind(now)
            .bind(now)
            .bind(cap)
            .execute(&self.pool)
            .await?;
        // SAFETY: `SQL_INSERT_TRACKER`'s only WHERE clause is the cap
        // check. Input-value problems (length, format, character set)
        // are caught upstream by `validate_tracker_inputs`; UNIQUE
        // collisions surface as `Err(...)` via `From<sqlx::Error>`,
        // not as zero rows. So `rows_affected == 0` unambiguously
        // means the cap was reached. Any future WHERE addition would
        // make this interpretation ambiguous and must be reflected
        // here.
        if result.rows_affected() == 0 {
            return Err(TrackerDbError::TooMany);
        }
        let id = result.last_insert_rowid();
        Ok(self.get_by_id(id).await?.ok_or(sqlx::Error::RowNotFound)?)
    }

    /// Replace a tracker row's mutable fields by id. Returns the
    /// updated record, or `None` if the id wasn't found.
    ///
    /// On UNIQUE-constraint violation, returns
    /// [`TrackerDbError::EndpointDuplicate`] or
    /// [`TrackerDbError::NameDuplicate`] depending on which constraint
    /// fired. Other failures bubble up as [`TrackerDbError::Other`].
    pub async fn update(
        &self,
        id: i64,
        params: UpdateTrackerParams<'_>,
    ) -> Result<Option<TrackerRecord>, TrackerDbError> {
        let now = Utc::now().timestamp();
        let result = sqlx::query(sql::SQL_UPDATE_TRACKER)
            .bind(params.address)
            .bind(i64::from(params.port))
            .bind(params.fingerprint)
            .bind(params.password)
            .bind(params.name)
            .bind(params.enabled)
            .bind(now)
            .bind(id)
            .execute(&self.pool)
            .await?;
        if result.rows_affected() == 0 {
            return Ok(None);
        }
        Ok(self.get_by_id(id).await?)
    }

    /// Narrow update: replace only the pinned fingerprint. Used by
    /// the tracker task on TOFU first-connect (writing the observed
    /// fingerprint when the row's pin was `NULL`) and on admin-accept
    /// after a mismatch (writing the previously-pending observation).
    ///
    /// Returns `true` if the row existed, `false` otherwise.
    pub async fn update_fingerprint(
        &self,
        id: i64,
        fingerprint: &str,
    ) -> Result<bool, sqlx::Error> {
        let now = Utc::now().timestamp();
        let result = sqlx::query(sql::SQL_UPDATE_TRACKER_FINGERPRINT)
            .bind(fingerprint)
            .bind(now)
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(result.rows_affected() > 0)
    }

    /// Delete a tracker row by id. Returns `true` if the row existed.
    pub async fn delete(&self, id: i64) -> Result<bool, sqlx::Error> {
        let result = sqlx::query(sql::SQL_DELETE_TRACKER)
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(result.rows_affected() > 0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::testing::create_test_db;

    /// A canonical 95-byte fingerprint string for tests.
    const TEST_FINGERPRINT: &str = "AA:BB:CC:DD:EE:FF:00:11:22:33:44:55:66:77:88:99:\
         AA:BB:CC:DD:EE:FF:00:11:22:33:44:55:66:77:88:99";

    fn create_params<'a>(address: &'a str, name: &'a str) -> CreateTrackerParams<'a> {
        CreateTrackerParams {
            address,
            port: 7510,
            fingerprint: None,
            password: None,
            name,
            enabled: true,
        }
    }

    fn make_record(password: Option<&str>) -> TrackerRecord {
        TrackerRecord {
            id: 1,
            address: "tracker.example.com".to_string(),
            port: 7510,
            fingerprint: None,
            password: password.map(str::to_string),
            name: "Public".to_string(),
            enabled: true,
            created_at: 0,
            updated_at: 0,
        }
    }

    #[test]
    fn debug_redacts_password() {
        let record = make_record(Some("supersecret"));
        let dbg = format!("{record:?}");
        assert!(
            !dbg.contains("supersecret"),
            "raw password leaked into Debug output: {dbg}"
        );
        assert!(
            dbg.contains("<REDACTED>"),
            "expected redaction marker, got: {dbg}"
        );
    }

    #[test]
    fn debug_shows_none_password_verbatim() {
        let record = make_record(None);
        let dbg = format!("{record:?}");
        assert!(
            !dbg.contains("<REDACTED>"),
            "should not redact when no password is set: {dbg}"
        );
        assert!(
            dbg.contains("password: None"),
            "expected `password: None`, got: {dbg}"
        );
    }

    #[tokio::test]
    async fn create_round_trips() {
        let pool = create_test_db().await;
        let db = TrackerDb::new(pool);

        let created = db
            .create(create_params("tracker.example.com", "Public Tracker"))
            .await
            .expect("create");

        assert!(created.id > 0);
        assert_eq!(created.address, "tracker.example.com");
        assert_eq!(created.port, 7510);
        assert!(created.fingerprint.is_none());
        assert!(created.password.is_none());
        assert_eq!(created.name, "Public Tracker");
        assert!(created.enabled);
        assert!(created.created_at > 0);
        assert_eq!(created.created_at, created.updated_at);
    }

    #[tokio::test]
    async fn create_with_fingerprint_and_password() {
        let pool = create_test_db().await;
        let db = TrackerDb::new(pool);

        let created = db
            .create(CreateTrackerParams {
                address: "tracker.example.com",
                port: 7510,
                fingerprint: Some(TEST_FINGERPRINT),
                password: Some("hunter2"),
                name: "Gated Tracker",
                enabled: true,
            })
            .await
            .expect("create");

        assert_eq!(created.fingerprint.as_deref(), Some(TEST_FINGERPRINT));
        assert_eq!(created.password.as_deref(), Some("hunter2"));
    }

    #[tokio::test]
    async fn duplicate_address_port_rejected() {
        let pool = create_test_db().await;
        let db = TrackerDb::new(pool);

        db.create(create_params("tracker.example.com", "First"))
            .await
            .expect("first create");

        let err = db
            .create(create_params("tracker.example.com", "Second"))
            .await
            .expect_err("duplicate (address, port) must fail");
        assert!(
            matches!(err, TrackerDbError::EndpointDuplicate),
            "expected EndpointDuplicate, got: {err:?}"
        );
    }

    #[tokio::test]
    async fn duplicate_address_case_insensitive_rejected() {
        let pool = create_test_db().await;
        let db = TrackerDb::new(pool);

        db.create(create_params("tracker.example.com", "First"))
            .await
            .expect("first create");

        // Same address in different ASCII case must collide via the
        // expression-based unique index on `(LOWER(address), port)`.
        let err = db
            .create(create_params("Tracker.Example.COM", "Second"))
            .await
            .expect_err("duplicate (address, port) case-insensitive must fail");
        assert!(
            matches!(err, TrackerDbError::EndpointDuplicate),
            "expected EndpointDuplicate, got: {err:?}"
        );
    }

    #[tokio::test]
    async fn same_address_different_port_allowed() {
        let pool = create_test_db().await;
        let db = TrackerDb::new(pool);

        db.create(CreateTrackerParams {
            address: "tracker.example.com",
            port: 7510,
            ..create_params("tracker.example.com", "Primary")
        })
        .await
        .expect("first create");

        db.create(CreateTrackerParams {
            address: "tracker.example.com",
            port: 7520,
            ..create_params("tracker.example.com", "Secondary")
        })
        .await
        .expect("second create on different port");
    }

    #[tokio::test]
    async fn list_all_sorted_by_name_case_insensitive() {
        let pool = create_test_db().await;
        let db = TrackerDb::new(pool);

        // Insert in non-alphabetical order with mixed case so the sort
        // does meaningful work.
        db.create(create_params("zeta.example.com", "zeta"))
            .await
            .expect("create zeta");
        db.create(create_params("alpha.example.com", "Alpha"))
            .await
            .expect("create Alpha");
        db.create(create_params("middle.example.com", "middle"))
            .await
            .expect("create middle");

        let all = db.list_all().await.expect("list");
        let names: Vec<&str> = all.iter().map(|r| r.name.as_str()).collect();
        assert_eq!(names, vec!["Alpha", "middle", "zeta"]);
    }

    #[tokio::test]
    async fn duplicate_name_case_insensitive_rejected() {
        let pool = create_test_db().await;
        let db = TrackerDb::new(pool);

        db.create(create_params("a.example.com", "Public Tracker"))
            .await
            .expect("first create");

        // Same name in different case must collide.
        let err = db
            .create(create_params("b.example.com", "public tracker"))
            .await
            .expect_err("duplicate name (case-insensitive) must fail");
        assert!(
            matches!(err, TrackerDbError::NameDuplicate),
            "expected NameDuplicate, got: {err:?}"
        );
    }

    #[tokio::test]
    async fn get_by_id_returns_none_for_unknown() {
        let pool = create_test_db().await;
        let db = TrackerDb::new(pool);
        assert!(db.get_by_id(99_999).await.expect("get").is_none());
    }

    #[tokio::test]
    async fn update_replaces_full_row() {
        let pool = create_test_db().await;
        let db = TrackerDb::new(pool);

        let original = db
            .create(create_params("tracker.example.com", "Old Name"))
            .await
            .expect("create");

        // Sleep a hair so updated_at is detectably later than created_at.
        // (On fast machines `Utc::now().timestamp()` may equal the create
        // timestamp without this. Sub-second resolution would be nicer
        // but `i64` epoch seconds is the existing convention.)
        tokio::time::sleep(std::time::Duration::from_millis(1100)).await;

        let updated = db
            .update(
                original.id,
                UpdateTrackerParams {
                    address: "tracker2.example.com",
                    port: 7520,
                    fingerprint: Some(TEST_FINGERPRINT),
                    password: Some("new-password"),
                    name: "New Name",
                    enabled: false,
                },
            )
            .await
            .expect("update")
            .expect("updated row");

        assert_eq!(updated.id, original.id);
        assert_eq!(updated.address, "tracker2.example.com");
        assert_eq!(updated.port, 7520);
        assert_eq!(updated.fingerprint.as_deref(), Some(TEST_FINGERPRINT));
        assert_eq!(updated.password.as_deref(), Some("new-password"));
        assert_eq!(updated.name, "New Name");
        assert!(!updated.enabled);
        assert_eq!(updated.created_at, original.created_at);
        assert!(
            updated.updated_at > original.updated_at,
            "updated_at must advance"
        );
    }

    #[tokio::test]
    async fn update_unknown_id_returns_none() {
        let pool = create_test_db().await;
        let db = TrackerDb::new(pool);

        let result = db
            .update(
                99_999,
                UpdateTrackerParams {
                    address: "unused.example.com",
                    port: 7510,
                    fingerprint: None,
                    password: None,
                    name: "Unused",
                    enabled: true,
                },
            )
            .await
            .expect("update");
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn update_fingerprint_narrow_update() {
        let pool = create_test_db().await;
        let db = TrackerDb::new(pool);

        let original = db
            .create(create_params("tracker.example.com", "Tracker"))
            .await
            .expect("create");
        assert!(original.fingerprint.is_none());

        let changed = db
            .update_fingerprint(original.id, TEST_FINGERPRINT)
            .await
            .expect("update_fingerprint");
        assert!(changed);

        let fetched = db
            .get_by_id(original.id)
            .await
            .expect("get")
            .expect("row exists");
        assert_eq!(fetched.fingerprint.as_deref(), Some(TEST_FINGERPRINT));
        // Other fields untouched.
        assert_eq!(fetched.address, original.address);
        assert_eq!(fetched.name, original.name);
        assert_eq!(fetched.enabled, original.enabled);
    }

    #[tokio::test]
    async fn update_fingerprint_unknown_id_returns_false() {
        let pool = create_test_db().await;
        let db = TrackerDb::new(pool);
        let changed = db
            .update_fingerprint(99_999, TEST_FINGERPRINT)
            .await
            .expect("update_fingerprint");
        assert!(!changed);
    }

    #[tokio::test]
    async fn delete_round_trips() {
        let pool = create_test_db().await;
        let db = TrackerDb::new(pool);

        let created = db
            .create(create_params("tracker.example.com", "Tracker"))
            .await
            .expect("create");

        let deleted = db.delete(created.id).await.expect("delete");
        assert!(deleted);

        assert!(
            db.get_by_id(created.id).await.expect("get").is_none(),
            "row should be gone after delete"
        );

        // Calling delete again is idempotent (returns false).
        let deleted_again = db.delete(created.id).await.expect("delete again");
        assert!(!deleted_again);
    }

    #[tokio::test]
    async fn deleting_one_does_not_touch_siblings() {
        let pool = create_test_db().await;
        let db = TrackerDb::new(pool);

        let a = db
            .create(create_params("a.example.com", "A"))
            .await
            .expect("create a");
        let b = db
            .create(create_params("b.example.com", "B"))
            .await
            .expect("create b");

        db.delete(a.id).await.expect("delete a");

        let all = db.list_all().await.expect("list");
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].id, b.id);
    }
}
