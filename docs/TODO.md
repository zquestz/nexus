# Nexus TODO

## Implementation Order (Pre-Launch)

| #   | Feature                  | Effort | Status      |
| --- | ------------------------ | ------ | ----------- |
| 1   | Account groups           | Low    | ✅ Done     |
| 2   | Password strength        | Low    | ✅ Done     |
| 3   | Streaming hash transfers | Medium | ✅ Done     |
| 4   | Boards                   | High   | Planned     |
| 5   | File previews            | Low    | Planned     |
| 6   | Trackers                 | Medium | In progress |
| 7   | Speed limiting           | Medium | Planned     |
| 8   | Flood protection         | Low    | ✅ Done     |
| 9   | Server logs              | Medium | ✅ Done     |
| 10  | Auto-away                | Low    | ✅ Done     |
| 11  | Invite system            | Medium | Planned     |

**Post-launch:** IRC gateway (if demand exists)

## Decided Against

Features intentionally excluded with rationale.

| Feature              | Reason                                                                                     |
| -------------------- | ------------------------------------------------------------------------------------------ |
| `/me's` (possessive) | i18n complexity — each language handles possessives differently                            |
| Disable encryption   | Security — Nexus requires TLS always                                                       |
| File aliases         | OS concern — admin can use filesystem symlinks                                             |
| Process monitor      | Out of scope — BBS server, not system management tool                                      |
| Custom text colors   | Novelty feature that makes chat hard to read                                               |
| Folder comments      | Use descriptive folder names instead                                                       |
| News categories      | Flat list simpler for typical use cases                                                    |
| Remote shutdown      | Docker/systemd auto-restart defeats purpose; users with container access can stop directly |
| File tree view       | Tabs work well, tree view adds rendering complexity without real benefit                   |
| DCC                  | Peer-to-peer adds complexity; server-mediated transfers work well                          |
| Remote desktop       | Most servers are headless; out of scope for BBS software                                   |

## Feature Specs

### File Previews

Preview files before downloading.

**Supported types (v1):**

- Images: PNG, JPEG, WebP, GIF, BMP
- Text: TXT, MD, JSON (plain monospace, no syntax highlighting)

**Dialog UI:**

- Modal overlay (like current dialogs)
- Top bar: `← Back` (left) | filename | `Download` (right)
- Content area: progress bar while downloading, then image (scale to fit, center if small) or text (scrollable)
- Escape = Back

**Behavior:**

- Single click on file: preview (if enabled + supported) or download (fallback)
- Context menu: Preview option only for previewable types; Download always shown
- Works from file listing and search results
- Full error reporting in dialog (download fails, etc.)

**Keyboard navigation:**

- Escape: close preview, return to listing
- Left/Right arrows: prev/next previewable file (loops at ends, skips non-previewable)

**Transfer:**

- Uses existing transfer system to download to temp directory
- Shows in transfers panel (for visibility on large files)
- Preview dialog shows progress bar while downloading
- "Download" button saves from temp to user's chosen location (no re-download)

**Cancellation:**

- User clicks Back → cancel transfer, close dialog
- User presses Escape → cancel transfer, close dialog
- User navigates to different panel → cancel transfer, close dialog

**Cleanup:**

- Delete temp files on disconnect from server
- Delete temp files on app exit
- On startup: clean up orphaned temp directory from previous crash

**User Settings (Settings > Files):**

- Preview files before download: `Enabled` / `Disabled`

**Future (v2):**

- Syntax highlighting via `syntect` (light/dark themes)
- Line numbers

### Trackers

Discovery service for Nexus servers. Protocol design is complete; see
[protocol/18-trackers.md](protocol/18-trackers.md) for the full spec
(tracker protocol v0.1.0).

**Status:**

- ✅ `nexus-tracker` daemon — shipped (binary: `nexus-trackerd`)
- ✅ Packaging — Docker image (`ghcr.io/zquestz/nexus-trackerd`),
  systemd unit, `release.yml` integration with independently versioned
  artifacts and Docker tags
- ✅ Operator docs — [docs/tracker/](tracker/)
- ✅ Locales — all 13 languages populated in `nexus-tracker/locales/`

**Remaining work:**

| Item                              | Notes                                                                                                                                                                                                                                                                                              |
| --------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Server-side publisher integration | Per-tracker publisher tasks in `nexus-server`; admin-configured via protocol; persisted in `trackers` DB table. **Design captured in [Server-Side Publisher Implementation Plan](#server-side-publisher-implementation-plan) below.**                                                              |
| Client-side admin UI              | Tracker management panel in `nexus-client` admin section; calls the tracker admin protocol messages.                                                                                                                                                                                               |
| Client-side browser integration   | `nexus-client` queries one or more trackers and surfaces the listing in the bookmarks / server-list UI                                                                                                                                                                                             |

#### Server-Side Publisher Implementation Plan

Track 1 of the Trackers integration. Adds a per-tracker publisher
task system to `nexus-server` that maintains long-lived TLS
connections to admin-configured trackers and refreshes the registration
on the tracker-supplied interval.

##### DB schema

Migration `20260501002917_create_trackers.sql` ✅ landed.

`trackers` table: `id, address, port, fingerprint?, password?, name, enabled, created_at, updated_at` with `UNIQUE(address, port)` and `UNIQUE(LOWER(name))` indexes. Configuration only — runtime status (connection state, errors, pending fingerprints) lives in memory in the publisher manager, not in the DB.

##### DB layer (`nexus-server/src/db/trackers.rs`)

✅ Staged but uncommitted (waiting to land alongside handlers to avoid dead-code warnings). Includes `TrackerRecord`, `CreateTrackerParams`, `UpdateTrackerParams`, `TrackerDb` with CRUD methods + narrow `update_fingerprint` for TOFU-write path.

Plus a method on `Database` for the publisher's per-refresh field bundle:
- `ConfigDb::get_tracker_fields()` — single query for `(server_name, description, public_address)` from `config` table.
- `UserDb::guest_enabled()` — single query for the guest account's `enabled` field.
- `Database::tracker_registration_fields()` — composes both into a `TrackerRegistrationFields` struct for the publisher to consume.

##### Permissions (`nexus-common::ALL_PERMISSIONS`)

Four new permissions, alphabetically ordered:
- `tracker_create` — gates `TrackerCreate`
- `tracker_delete` — gates `TrackerDelete`
- `tracker_edit` — gates `TrackerEdit` + `TrackerUpdate`
- `tracker_list` — gates `TrackerList`

`is_admin` implicitly grants all four. **Not** in `SHARED_ACCOUNT_PERMISSIONS`.

Server-side `Permission` enum gets four new variants with snake_case mapping. Bump `assert_eq!(ALL_PERMISSIONS.len(), 50)` (was 46).

##### Protocol messages (`nexus-common/src/protocol.rs`)

Five request/response pairs, **no broadcasts** (admin uses a refresh button like Connection Monitor):

| Request | Response |
|---|---|
| `TrackerList` | `TrackerListResponse { success, error, trackers: Vec<TrackerInfo> }` |
| `TrackerEdit { id }` | `TrackerEditResponse { success, error, tracker: Option<TrackerInfo> }` |
| `TrackerCreate { address, port, fingerprint?, password?, name, enabled }` | `TrackerCreateResponse { success, error, id?, name? }` |
| `TrackerUpdate { id, address, port, fingerprint?, password?, name, enabled }` | `TrackerUpdateResponse { success, error, id?, name? }` |
| `TrackerDelete { id }` | `TrackerDeleteResponse { success, error, name? }` |

After Create/Update/Delete success, the client refetches via `TrackerList` (matching `refresh_user_management_list_for` pattern in user_admin.rs).

`TrackerInfo` struct combines the DB row (id, address, port, fingerprint, password, name, enabled, created_at, updated_at) with runtime status (connected, last_connected_at, last_error, last_error_kind, pending_fingerprint, refresh_interval). Disabled trackers have all-default runtime fields. Password is echoed plaintext (it's a shared invite-code-style secret, not a personal credential).

`Debug` impl for `ClientMessage::TrackerCreate` and `TrackerUpdate` redacts the `password` field.

##### Validators

New `nexus-common/src/validators/tracker_name.rs`:
- `MAX_TRACKER_NAME_LENGTH = 256`
- `validate_tracker_name(&str) -> Result<(), TrackerNameError>` mirroring `validate_server_name`: trim-not-empty, length cap, no control chars (newlines distinguished). Unicode and emoji allowed.

Reused: `validate_public_address` (boolean), `MAX_PASSWORD_LENGTH`, `nexus_common::fingerprint::is_canonical_fingerprint`. Port type-checked as `u16`; non-zero verified at handler.

Client-side trims the `name` (and `address`) before submitting; server validates as-received.

##### Publisher task (`nexus-server/src/tracker/`)

Parallel to `nexus-server/src/voice/`, `transfers/`, etc.

**`TrackerManager`** holds `Arc<Mutex<HashMap<i64, TrackerHandle>>>`. Each handle wraps a `tokio::task::JoinHandle` plus an `Arc<RwLock<TrackerStatus>>` for runtime state. API: `new`, `bootstrap`, `spawn`, `replace`, `terminate`, `status_for`, `status_all`, `shutdown`. Lock not held across `await` (so `std::sync::Mutex`).

**`PublisherContext`** (shared across all per-tracker tasks): `Arc<Database>`, `Arc<UserManager>`, server fingerprint, server port, optional websocket port. Server version is `CARGO_PKG_VERSION`.

**Per-task lifecycle:** outer loop runs connect → fingerprint check (two-stage: TLS-observed vs pin, then TLS-observed vs server-reported in HandshakeResponse) → tracker handshake → refresh loop. Inner refresh loop uses `tokio::select!` between sleep and reader, so connection drops mid-sleep are detected promptly. Read response with 30s timeout per refresh. **TOFU pin only committed after both fingerprint stages pass.**

**Backoff:** exponential 5s → 10s → 20s → 40s → 80s → 160s → 300s (cap), ±25% jitter, reset to 5s on successful register.

**Permanent vs transient errors:**
- Transient (retry forever): TCP fail, TLS fail, handshake fail, connection drop, read timeout, `rate_limited`, `capacity`.
- Permanent (try once, then exit task): `fingerprint_mismatch` (Stage 1), `fingerprint_intercepted` (Stage 2), `unauthorized`, `invalid`. Admin needs to fix; task respawns when admin updates the row.

The `last_error_kind` field encodes whether the error is unrecoverable; a shared `is_unrecoverable_error_kind` helper lives in `nexus-common` for both server and client to consume.

**Per-refresh `TrackerServerRegister` payload sourcing:**
- `password`, `address` → tracker row fields.
- `name`, `description`, `allows_guest` → `Database::tracker_registration_fields()`.
- `port`, `websocket_port`, `fingerprint` → `PublisherContext` (CLI args + startup-computed cert fingerprint).
- `version` → `CARGO_PKG_VERSION`.
- `user_count` → `UserManager` (count of distinct online nicknames per protocol spec).
- `locale` → `"en"` hardcoded.

**Disabled flag:** manager only tracks running tasks. Disabled rows = no HashMap entry. `status_for` returns `None` for disabled trackers; the handler fills runtime fields with their default "no task" values when composing `TrackerInfo`.

**Startup:** `bootstrap()` after UserManager init; loads enabled rows and spawns one task each. Synchronous (no I/O at spawn time).

**Shutdown:** `manager.shutdown()` aborts all tasks and awaits cancellation. Added to `DaemonHandles` alongside existing background-task abortions.

##### Handlers (`nexus-server/src/handlers/tracker_*.rs`)

Five handler files following the standard pattern (auth → permission → validate → DB op → manager interaction → typed response):
- `tracker_list.rs` — `TrackerList` → join all DB rows with `manager.status_all()`.
- `tracker_edit.rs` — `TrackerEdit { id }` → fetch row + `manager.status_for(id)`.
- `tracker_create.rs` — validate inputs → `db.trackers.create` → `manager.spawn(record)`. Map UNIQUE-constraint violations on `(address, port)` and `LOWER(name)` to specific error helpers via error-message inspection.
- `tracker_update.rs` — validate inputs → `db.trackers.update` → `manager.replace(record)`.
- `tracker_delete.rs` — fetch existing for the toast-message name → `db.trackers.delete` → `manager.terminate(id)`.

Empty-string password normalizes to `None` at the handler boundary. Address stored as-typed (matching the IDN as-typed convention elsewhere); the tracker daemon does its own normalization at registration time.

##### Wiring

- `HandlerContext` gets `&TrackerManager`.
- `connection.rs` dispatch loop adds five new arms.
- `main.rs` constructs `PublisherContext`, `TrackerManager`, calls `bootstrap`, threads the manager into accept loops, adds it to `DaemonHandles` for shutdown.
- Frame-size limits in `nexus-common::framing::limits` add five entries with calculated bounds based on field caps.

##### i18n

10 new error keys in `nexus-server/locales/en/errors.ftl`, then translated to all 12 other locales:
- `err-tracker-not-found`
- `err-tracker-name-invalid`, `err-tracker-name-too-long`
- `err-tracker-address-invalid`, `err-tracker-address-too-long`
- `err-tracker-port-invalid`
- `err-tracker-fingerprint-invalid`
- `err-tracker-password-too-long`
- `err-tracker-endpoint-duplicate`
- `err-tracker-name-duplicate`

Helpers in `handlers/errors.rs` mirror the news/groups conventions.

##### Tests

| Category | Count | Notes |
|---|---|---|
| DB layer | 13 | ✅ already written in `db/trackers.rs::tests` |
| Validators | ~12 | mirror `server_name.rs::tests` |
| Handlers | ~25 | per-handler happy path + permission denial + error paths |
| Manager unit | ~8 | spawn/replace/terminate/bootstrap/shutdown surface tests |
| Task lifecycle | ~10 | needs in-process mock tracker (~150 lines scaffolding) |
| Integration | ~1 | spin up real `nexus-trackerd` + `nexusd`, verify registration cycle |
| **Total** | **~69** | |

### User/Group Admin-Config Broadcasts

Backfill the news-style broadcast pattern for User Management and
Group Management panels. Currently admin B viewing those panels sees
stale data when admin A creates/deletes/updates entries — `UserCreate`
and `UserDelete` don't broadcast, and `UserUpdated` only covers
presence/identity-rendering for users currently online (away, status,
rename of an online user, etc.) — it doesn't fire for offline users
or for create/delete events.

**Pattern to apply** (mirrors `NewsUpdated` + `NewsShow`):

- New broadcast `UserAccountUpdated { action: UserAccountAction, id }`
  fired on every account-level CRUD operation, gated on
  `user_create | user_edit | user_delete | is_admin`. Distinct from
  the existing `UserUpdated` which stays for presence/chat-user-list
  rendering (gated on `user_list`).
- New broadcast `GroupUpdated { action: GroupAction, id }` fired on
  every group CRUD operation, gated on
  `group_create | group_edit | group_delete | is_admin`. Groups have
  no presence concept so this is the only broadcast they need.
- Action enums: `Created | Updated | Deleted`.
- Clients receiving the broadcast send a targeted fetch (e.g.,
  `UserShow { id }` / `GroupShow { id }` — both new, mirroring
  `NewsShow`) for Created/Updated, and remove locally for Deleted.

**Cross-entity consistency:** when a group is renamed, every cached
user row that references the group has a stale `group_name`. The
client must handle this when it receives `GroupUpdated`: after
fetching the new group data via `GroupShow`, walk the local
user-account cache and update `group_name` in place on every cached
user with the matching `group_id`. One broadcast plus one fetch,
regardless of how many members the group has — no per-user
broadcasts or per-user refetches, which would be O(members) and
prohibitive at scale.

**Why this is worth doing:** user lists can be very large
(thousands of accounts); full-list refresh on every change would be
expensive. Broadcast-then-fetch-by-id is bandwidth-proportional to
the actual change.

**Why this isn't being done for trackers:** tracker lists are small
(handful of entries), so a manual refresh button (matching the
Connection Monitor pattern) is sufficient there.
