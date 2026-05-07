# Nexus TODO

## Implementation Order (Pre-Launch)

| #   | Feature                     | Effort | Status                                |
| --- | --------------------------- | ------ | ------------------------------------- |
| 1   | Account groups              | Low    | ✅ Done                               |
| 2   | Password strength           | Low    | ✅ Done                               |
| 3   | Streaming hash transfers    | Medium | ✅ Done                               |
| 4   | Boards                      | High   | Planned                               |
| 5   | File previews               | Low    | Planned                               |
| 6   | Tracker registration        | Medium | ✅ Done                               |
| 7   | Tracker discovery           | Low    | 🟡 In progress (steps 1–4 of 10 done) |
| 8   | Speed limiting              | Medium | Planned                               |
| 9   | Flood protection            | Low    | ✅ Done                               |
| 10  | Server logs                 | Medium | ✅ Done                               |
| 11  | Auto-away                   | Low    | ✅ Done                               |
| 12  | Invite system               | Medium | Planned                               |
| 13  | Certificate fingerprint pin | Low    | ✅ Done                               |

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

## Tech Debt

### Case-insensitive IP uniqueness for bans / trusts

`nexus-server/migrations/20260109013500_create_ip_bans.sql` and
`nexus-server/migrations/20260110230700_create_ip_trusted.sql` declare
`ip_address TEXT NOT NULL UNIQUE` (column-level, case-sensitive). The
insert paths in `db/bans.rs` and `db/trusts.rs` bind the address as-typed,
so `2001:DB8::1` and `2001:db8::1` can coexist as separate rows pointing
at the same IPv6 host.

SQLite has no `DROP CONSTRAINT`, so removing the column-level UNIQUE
requires a full table rebuild: create new table → `INSERT OR IGNORE`
the data with `LOWER(ip_address)` to drop any case-conflicting rows
→ drop old → rename → recreate non-unique indexes. Pair the rebuild
with a fresh `CREATE UNIQUE INDEX … ON …(LOWER(ip_address))` on the
new table.

Trackers had the same shape but was fixed in-place since the feature
was unlaunched at the time (`(address, port)` → `(LOWER(address), port)`,
plus updating the `ENDPOINT_FAILURE_MARKER` in `db/trackers.rs` to
match the new expression-based-index error message format).

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
- ✅ Server-side tracker registration — per-tracker tasks, admin
  protocol messages, TOFU pin, propagation contract. See
  [docs/protocol/09-admin.md](protocol/09-admin.md) and
  [docs/protocol/18-trackers.md](protocol/18-trackers.md).
- ✅ Protocol vocabulary uses the pointer-shaped-entity convention —
  `TrackerAdd`/`TrackerRemove` with matching permission strings
  `tracker_add`/`tracker_remove`. Dedicated `TrackerAcceptFingerprint`
  message for the Stage 1 cert-rotation accept flow. Convention
  codified in `CLAUDE.md` "Naming Conventions" so future
  pointer-shaped entities follow it.
- ✅ Client-side admin UI — Server Info panel restructured into
  `Config | Trackers` tabs; full tracker management (Add / Edit /
  Remove / Accept Fingerprint) with permission-gated context menus,
  themed-color status indicators, and double-submit guards. See
  [docs/client/10-server-info.md](client/10-server-info.md).
- ✅ Locales — all 13 languages populated across tracker daemon,
  server-side admin protocol, and client-side admin UI.
- ✅ Tracker-side compatibility filter — `TrackerServerList` carries
  the requesting client's `version` (required wire field), and the
  tracker filters returned `servers` to entries whose registered
  `version` is semver-compatible per
  `nexus_common::version::check_compatibility`. Same rule the BBS
  handshake uses. Registration also tightened to validate the server
  `version` end-to-end (semver shape + length cap), guaranteeing
  every stored entry is parseable. See the "Compatibility filter"
  section in [docs/protocol/18-trackers.md](protocol/18-trackers.md).
- ✅ Tracker registration field validation — `name`, `description`,
  `locale` use full validators (not length-only); `port` and
  `websocket_port` reject zero. Mirrors BBS server-info validation
  for the same fields.

**Remaining work — Client-Side Discovery Panel:**

Dedicated panel for browsing servers advertised by trackers, plus
add / edit / remove of the user's own tracker list. Per-tracker view
(no aggregation across trackers — one tracker selected at a time,
Hotline-style).

Depends on the Certificate Fingerprint Pin work (planning entry
#13) — the discovery panel pre-fills that field from each
tracker-advertised entry.

**Persistence:**

- New `ClientTracker` struct in client config alongside `bookmarks`:
  `id: Uuid`, `name`, `address`, `port` (default 7510), `password`
  (Option<String>), `fingerprint` (Option<String>, pinned via TOFU
  on first successful query).
- Stored in `~/.config/nexus/config.json` under
  `client_trackers: Vec<ClientTracker>`.
- **Default port** uses the `nexus_common::DEFAULT_TRACKER_PORT`
  constant (currently 7510). Don't hardcode the literal.

**Save timing — when `config.save()` runs:**

- **Add**: synchronously after the form's Save button writes the
  new entry into the config.
- **Edit**: synchronously after the form's Save button updates
  the entry.
- **Remove**: synchronously after the ConfirmRemove dialog
  removes the entry.
- **First-query fingerprint pin commit**: auto-save after the
  TLS-observed fingerprint is written to the row (no user-facing
  Save action; the user already authorized the connection by
  triggering the query).
- **AcceptFingerprint accept**: auto-save after the new pin is
  written to the row.

In every case the save is synchronous — no batching, no defer.
Failure to save surfaces the same way it does for bookmarks
(`err-failed-save-config` toast / inline error).

- **Fingerprint normalization** — mirror the
  `ServerBookmark.certificate_fingerprint` invariant from
  `types/bookmark.rs`:
  - `Some("")` and `Some(<whitespace>)` always collapse to `None` so
    the stored representation is `None | Some(<trimmed>)`.
  - Add a `normalize_tracker_fingerprint(Option<String>) -> Option<String>`
    helper alongside `ClientTracker` (or share the existing helper if
    the bookmark version is generalized first). Use it at every
    mutation site: input handler, form save, TOFU pin commit on
    successful first query, AcceptFingerprint accept path.
  - Add `#[serde(default, deserialize_with = "deserialize_normalized_fingerprint")]`
    to the field so hand-edited or pre-normalization configs collapse
    empty strings to `None` on load. Stage 1 then never has to
    disambiguate "empty string" from "no pin".

**Header button:**

- Satellite-dish icon (`fontawesome-satellite-dish`, added to
  `fonts/icons.toml` → `icon::satellite_dish()`).
- Position: right of the Transfers button in the top toolbar (in the
  global / right-hand button group with About / Settings — not the
  per-connection group).
- Always visible, regardless of connection state.

**Panel layout (List mode):**

```
┌──────────────────────────────────────────────────────────────────┐
│                       Trackers                    [+]           │
│                                                                  │
│  [tracker.example.com ▼]  [✎] [🗑] [↻]            12 servers     │
│  [ Search servers…                                             ] │
│                                                                  │
│  Name              ▾    Description                      Users  │
│  ─────────────────────  ──────────────────────────────   ─────  │
│  My BBS                 Welcome to my server!            12     │
│  Other BBS              Cool stuff happens here          3      │
│  Third One                                               0      │
└──────────────────────────────────────────────────────────────────┘
```

- **Title row**: "Trackers" centered (news-style spacer balancing
  the right-side button), `[+]` plus button on the right with
  tooltip "Add Tracker", always visible (no permission gating —
  every user manages their own client tracker list).
- **Toolbar row**: tracker pulldown (alphabetical by display name)
  on the left; Edit / Remove / Refresh icon buttons next to it
  (tooltips "Edit", "Remove", "Refresh"); status text right-aligned.
- **Search row**: full-width `text_input`, live filter (no submit
  step), placeholder "Search servers…", no magnifier button.
- **Listing**: 3 sortable columns — Name, Description, Users. Default
  sort Name asc (matches wire order). See the two subsections below
  for context-menu items and the column-width convention.

**Listing — context menu (right-click on a row):**

Three items, no separators (similar-weight actions, no destructive
item to set apart):

- **Connect** — same as left-clicking Name (opens Connect dialog
  pre-filled per the click-to-connect section below).
- **Bookmark** — opens the Bookmark Add dialog pre-filled with
  name / address / port / fingerprint from this entry. User
  finishes filling and saves; this never opens Connect.
- **Copy URI** — copies `nexus://address:port` for this entry to
  the clipboard (no user, no path; minimal canonical form). Shows
  the standard "Server URI copied" toast (subject-first form
  matching existing copy-URI flows; key
  `toast-server-uri-copied` or the existing equivalent if one
  already exists for the Server Info "Copy URI" button — check at
  implementation time, reuse if so).

**Listing — column-width convention** (CLAUDE.md "Table Consistency"):

- All header buttons `.width(Length::Shrink)` so the sort icon hugs
  the column-name text.
- Variable-content columns (Name, Description) use
  `Length::FillPortion(1)` so they share leftover space.
- Fixed/short-content column (Users) uses `Length::Shrink`.
- **Users is rightmost** → trailing
  `Space::new().width(SCROLLBAR_PADDING)` in both the header content
  row AND the cell render closure so content doesn't abut the
  scrollbar.
- Sort indicators via the shared
  `views/helpers.rs::sort_icon_or_placeholder(is_active, is_ascending)`
  helper — do NOT inline the icon-vs-placeholder match.

**State variants on the toolbar row:**

| Trackers     | Pulldown | Edit / Remove | Refresh  | Status             |
| ------------ | -------- | ------------- | -------- | ------------------ |
| Zero         | disabled | disabled      | disabled | empty              |
| 1+, fetching | enabled  | enabled       | disabled | `Loading…` muted   |
| 1+, success  | enabled  | enabled       | enabled  | `N servers`        |
| 1+, error    | enabled  | enabled       | enabled  | `Error: <msg>` red |

Refresh-disable rule mirrors User Management: gated on
`!is_fetching` so an error response re-enables the button
(otherwise the user gets stuck after a failed fetch). See the
Cache structure section for the full state representation.

**Long error messages in the status text:** the error variant
shows `Error: <short>` truncated with an ellipsis if it would
overflow the toolbar row. The full untruncated message is
available as a tooltip on hover. Truncation is visual-only —
the underlying `error: Option<String>` keeps the full text.

**Search behaviour (live filter, mirrors files panel UX):**

- Filters as the user types; one `search_input: String` field on
  the panel state.
- Case-insensitive substring match.
- Two-pass relevance ordering:
  1. Entries whose `name` contains the query.
  2. Entries whose `description` contains the query and whose name
     does not.
- Within each group, the active column sort is preserved.
- `search_input` resets when the selected tracker changes.

**Auto-fetch behaviour:**

- On panel open: query the currently selected tracker (last
  selected, or — if no prior selection in this app session — the
  alphabetically-first configured tracker, matching the dropdown's
  visible order).
- On dropdown change: query the newly selected tracker, **only if
  not already cached** for this session — otherwise switch shows
  the cached listing instantly.
- On `[Refresh]`: re-query the selected tracker (forces a fresh
  query even if cached).
- No timer-based refresh (the spec defines listings as
  non-subscriptions; clients re-query for fresh data).
- In-memory cache only; cleared on app exit.
- Last-selected tracker remembered across panel close / reopen
  within the session (in-memory).

**Cache structure:**

- `HashMap<Uuid, TrackerCacheEntry>` keyed by `ClientTracker.id`.
- `TrackerCacheEntry` carries three fields:
  ```rust
  struct TrackerCacheEntry {
      /// Most recent successful fetch. Survives across failed
      /// refreshes — keeps the list visible when a refresh errors.
      entries: Option<Vec<ServerEntry>>,
      /// Most recent fetch error (already localized). Cleared on
      /// the next successful fetch.
      error: Option<String>,
      /// Whether a fetch is currently in flight.
      is_fetching: bool,
  }
  ```
- Refresh-disable rule: gated on `!is_fetching`. Mirrors the User
  Management pattern (Refresh re-enables on success OR error).
- Removing a tracker drops its cache entry (and silently discards
  any in-flight result for it — see below).
- Editing a tracker drops its cache entry (per the Edit section).

**List area display logic** — single source of truth for what the
user sees:

| `entries` | `is_fetching` | List area renders                     |
| --------- | ------------- | ------------------------------------- |
| `Some(_)` | any           | the table (last successful fetch)     |
| `None`    | `true`        | loading indicator                     |
| `None`    | `false`       | empty state (initial / never fetched) |

Errors are **NEVER** rendered in the list area. The list area is
content-only: it shows entries when we have them, a loading
indicator while waiting for the first-ever fetch, or an empty
state. **All error reporting lives in the toolbar status text**
(see the next section).

This means a refresh that fails after a successful fetch keeps the
table visible — the user doesn't lose context, and the error
appears only in the toolbar above the list.

**In-flight fetch behaviour:**

- Switching trackers mid-fetch does **not** cancel the in-flight
  query. The query completes and writes its result into the cache
  for the originating tracker id. If the user switches back to that
  tracker before / after the result lands, they see the populated
  cache instantly with no re-fetch.
- The currently-selected tracker is the source of truth for what's
  displayed: a stale result writing into the cache for a different
  tracker doesn't affect the visible list.
- If the originating tracker was removed before the query
  completed, the result is silently discarded (no cache entry to
  write into).

**Status text on tracker switch:**

- Switching the dropdown shows the new tracker's cached state
  immediately:
  - Cached entries present → status reads `N servers`, table
    renders. If the cache also has an `error` from a later failed
    refresh, status reads `Error: <msg>` in red but the table
    still renders (last successful entries).
  - Cache has only an `error` (failed first-ever fetch) → status
    reads `Error: <msg>` in red, list area shows the empty state
    (no entries to render).
  - No cache entry → status reads `Loading…` muted while the
    auto-fetch runs.

**After Add succeeds:**

- Panel switches back to List mode with the newly-added tracker
  selected in the dropdown. An auto-fetch fires (no cache entry
  yet), status shows `Loading…`. Same applies after Add for the
  first-ever tracker — the empty-state message is replaced by the
  populated dropdown and a loading status.

**Empty / error states:**

- **No trackers configured**: empty-state copy reads simply
  `No trackers configured.` in the list area. No CTA arrow / button
  pointing at `[+]` — the `[+]` is right there in the title row,
  the message is just a flat statement of fact. Matches how other
  list views surface empty-state copy. New i18n key:
  `empty-no-trackers-configured`, 13 locales.
- **Tracker fetch failed**: localized error (transport, auth,
  rate-limit) surfaces only as `Error: <msg>` in red in the
  toolbar status text. List area is unaffected — it keeps showing
  the previous successful entries if we have any, or the empty
  state if this was a first-ever fetch.
- **Tracker reachable but listing empty**: explicit "this tracker
  has no registered servers" message, distinct from a fetch error.
- **Empty `description` cells render blank**, not a localized
  `(no description)` placeholder.

**Five views (mode-based full-panel takeover, mirroring User Management):**

The panel has a `TrackerBrowserMode` enum dispatched by
`views/layout.rs`. When mode is `List`, the table renders; other
modes replace the panel content with the relevant form / dialog.
Patterns match `views/users.rs` (full-panel takeover, not a popup
overlay).

1. **List** — main view (toolbar + search + table).
2. **Add** — full-panel form. Fields in tab order: Name → Address →
   Port (`NumberInput`, **skipped by Tab** per CLAUDE.md "UI
   Quirks" note) → Password (secure) → Fingerprint. Footer buttons
   are `[Cancel] [Save]` — submit is **Save** (per the project-wide
   button-verb convention; trackers are pointer-shaped at the title
   level — title says "Add Tracker" — but the action button is the
   universal "Save" used by every other form except File New
   Directory). `is_submitting` guard. Validate-then-submit on
   Enter. Submit-time validation:
   - Name required (non-empty after trim).
   - Address required (non-empty after trim).
   - Port: always valid (`NumberInput<u16>`).
   - Password: optional, no format validation.
   - Fingerprint: optional. Trim, treat empty as unpinned. Non-empty
     must pass `nexus_common::fingerprint::is_canonical_fingerprint`
     — same validator the Phase A Connect/Bookmark forms use. On
     failure, show `err-fingerprint-invalid` (i18n key already
     exists in 13 locales).
3. **Edit** — same fields as Add, all editable (including address
   and port — the user might want to point an existing entry at a
   different tracker without losing other state). Pre-populated
   with current values.
   - **Trust the user.** Changing address/port doesn't auto-clear or
     warn about the fingerprint pin. If the pin is now wrong for the
     new host, the next fetch will surface a Stage 1 mismatch dialog
     and the user can resolve it there. Same applies to other
     edits — validation surfaces format errors; we don't second-guess
     the user's intent.
   - **Editing a tracker drops its cache entry** so a fresh query
     fires on the next selection. Editing the password (gated
     tracker), address/port (different host), or fingerprint
     (different pin) all change the meaningful identity of the
     tracker; the cached listing was for the old configuration and
     should not survive.
4. **ConfirmRemove** — confirmation view with the tracker's display
   name. `Cancel` + `Remove` buttons.
5. **AcceptFingerprint** — Stage 1 TOFU mismatch dialog, mirroring
   `views/trackers.rs::accept_fingerprint_modal` (the BBS-admin
   side). Title, server-identification line, warning, expected /
   received fingerprints in monospace multiline form, `Cancel` +
   `Accept` buttons. Cancel returns to List with the mismatch error
   shown in the toolbar status text next to the pulldown.
   - **Distinct message namespace** from the BBS-admin side. The
     admin panel already has `Message::AcceptFingerprintConfirm` /
     `…Cancel` for _server_ tracker registrations. The discovery
     panel needs its own variants — e.g.
     `Message::TrackerDiscoveryAcceptFingerprint*` — so dispatch
     doesn't conflate the two flows. Don't reuse the admin keys.

**Keyboard shortcuts:**

- **Escape** → cancel, return to List mode (works in Add / Edit /
  ConfirmRemove / AcceptFingerprint).
- **Enter** on a text_input → validate-then-submit (handler routes
  to `Validate*` if incomplete, `*Pressed` if complete).
- **Tab** → cycles between text inputs only (Name → Address →
  Password → Fingerprint → Name). The Port `NumberInput` is skipped
  (consumes Tab internally per CLAUDE.md UI Quirks). The Cancel /
  Save buttons are mouse-or-Enter only — Tab does not step into
  them.

**Click-to-connect from listing:**

- Clicking the **Name** column on a server row opens the existing
  Connect dialog (`views/connection.rs`) with name / address /
  port / fingerprint pre-filled from the tracker entry.
- **Add Bookmark checkbox is NOT auto-checked.** Bookmarking is the
  user's deliberate choice; opt-out (the user has to remember to
  uncheck) is wrong UX. The user can tick the box themselves if
  they want to save the entry as a bookmark.
- The tracker-supplied fingerprint is a display aid only; the
  BBS-side two-stage TOFU runs from scratch when the user actually
  connects (per spec: "The listed fingerprint is a display aid,
  not a trust assertion").
- **Plumbing**: a new `Message::OpenConnectFromTracker(ServerEntry)`
  (or equivalent) writes the four pre-fill fields into
  `ConnectionFormState`, then transitions to the Connect view.
  - **Clear before fill.** The handler calls
    `ConnectionFormState::clear()` first to drop any half-typed
    state from a previous Connect attempt, then writes the four
    pre-fill values (`server_name`, `server_address`, `port`,
    `fingerprint`). No half-state mixing.
  - **`add_bookmark` stays `false`.** `clear()` doesn't reset it
    today (pre-existing behaviour), but for tracker-launched
    Connect we explicitly set `add_bookmark = false` after `clear()`
    to defend against any prior `true` value lingering. Bookmarking
    remains the user's deliberate choice.
  - **Cancel returns to the Trackers panel**, not to chat. The
    Connect dialog's Cancel handler today goes to chat
    unconditionally — needs to be conditional on "where did this
    Connect dialog open from?" Implementation options: a
    `connect_origin: Option<ActivePanel>` field on
    `ConnectionFormState`, or a separate one-shot `ReturnPanel`
    state that the Connect handlers consult on Cancel and on
    successful connect. Same conditional applies to the
    **Bookmark** context-menu item: Bookmark Add dialog launched
    from the Trackers panel returns to the Trackers panel on
    Cancel / Save, not to chat.

**Network layer (`nexus-client/src/network/tracker_query.rs`):**

- One-shot `query_tracker(tracker, locale, version) -> Result<Vec<ServerEntry>, TrackerError>`.
- TLS connect → handshake → two-stage fingerprint check (Stage 1
  vs stored pin if present, Stage 2 vs `HandshakeResponse.fingerprint`)
  → send `TrackerServerList { password, locale, version }` →
  receive `TrackerServerListResponse` → close. `version` is the
  client's own crate version; the tracker uses it to filter the
  returned list to semver-compatible entries.
- Stage 1 mismatch fires the `AcceptFingerprint` mode dialog; on
  accept, rotates the pin and re-queries. Stage 2 mismatch aborts
  with no accept path (active interception).
- On first successful query (no prior pin), commits the
  TLS-observed fingerprint to the tracker config row.
- Reuses `resolve_host_for_connection`, proxy bypass logic, and TLS
  patterns from existing `network/` code.

**`TrackerError` type** — typed enum so the handler can route
Stage 1 mismatches to `AcceptFingerprint` mode, and so other
errors land in the cache as a localized string:

```rust
enum TrackerError {
    /// Stage 1 fingerprint mismatch — handler enters
    /// AcceptFingerprint mode with these values.
    Stage1Mismatch { expected: String, received: String },
    /// Stage 2 fingerprint mismatch (active interception) — abort,
    /// no accept path. Handler surfaces as a fatal error in the
    /// status text.
    Stage2Mismatch { received: String },
    /// All other errors, already localized and ready to display.
    /// Includes:
    ///   - Server-returned errors from TrackerServerListResponse
    ///     (`success: false`, `error: "<localized per request locale>"`).
    ///     The tracker returns these pre-translated per the request's
    ///     `locale` field, so we pass them through unchanged.
    ///   - Client-side TLS / handshake / protocol failures, localized
    ///     in the handler before being wrapped.
    Other(String),
}
```

The handler converts `Stage1Mismatch` into a mode transition (no
cache write), `Stage2Mismatch` into a cache `Err` with a localized
"interception detected" message, and `Other(s)` into a cache
`Err(s)` directly.

**Naming:**

User-facing label is "Trackers" — same as the BBS-admin tracker
management panel. The two never co-locate in the UI (admin panel
is gated on `tracker_list` and lives inside Server Info), so the
shared label is unambiguous in context.

**i18n / docs:**

- 13 locales: new keys for the panel title, search placeholder,
  toolbar tooltips, table column headers, status text variants,
  error messages, empty-state copy, modal labels (Add / Edit /
  Remove / Accept), and validation errors. Keys follow the
  entity-first convention (`title-tracker-…`, `tooltip-tracker-…`,
  etc.) — the codebase was normalized to entity-first in a sweep
  preceding this work.
- New chapter under `docs/client/` covering the Trackers panel
  from the user's perspective.

**`ServerEntry` fields the v1 listing does NOT display:**

The wire-level `nexus_common::tracker_protocol::ServerEntry` carries
a few fields beyond Name / Description / Address / Port / Users /
Fingerprint:

- **`version`** — server software version (e.g. `"0.8.2"`). Used by
  the _tracker_ to filter the returned list to entries semver-
  compatible with the requesting client (see the "Compatibility
  filter" section in
  [docs/protocol/18-trackers.md](protocol/18-trackers.md)); the
  client itself does not display a Version column.
- **`allows_guest`** — boolean: whether the guest account is
  enabled. Not displayed in v1. Could become a row icon (👤 /
  similar) in a future iteration.
- **`websocket_port`** — irrelevant; the client doesn't speak the
  WebSocket transport.

**Out of scope (v1):**

- No "All trackers" aggregation view — Hotline-style per-tracker
  view is simpler and avoids cross-tracker dedup decisions.
- No WebSocket transport for tracker queries; client uses TCP
  only.
- No timer-based auto-refresh.
- No persistent on-disk cache of fetched server lists.
- No automatic retry on fetch failure — user hits `[Refresh]`.
- No version / guest-allowed indicators in the listing (see
  `ServerEntry` fields section above).

**Implementation order:**

Each step should compile and partially work before moving on; the
panel becomes incrementally more useful as you go.

1. ✅ **Config layer** — `ClientTracker` struct, `Vec<ClientTracker>`
   in `Config`, serde with the normalize-on-deserialize pin. Added
   `Config::insert_tracker(index, tracker)` (clamps to len, never
   panics) so the Remove rollback path can restore a row at its
   original Vec position rather than appending to the end.
2. ✅ **Panel state** — `TrackerBrowserState` + `TrackerBrowserMode`
   enum, cache `HashMap<Uuid, Option<Result<…>>>`. Password fields
   on the Mode and on the State have manual `Debug` impls that
   redact via `[REDACTED]`.
3. ✅ **List view shell** — toolbar (pulldown + Edit/Remove/Refresh +
   status text), search row, listing table with the column-width
   convention. Renders against the stubbed empty cache; data flows
   in step 5.
4. ✅ **Add / Edit / Remove flows** — full-panel forms, full
   field-shape validation via `handlers/tracker_form_errors.rs`
   (shared with the BBS-admin tracker form), Unicode-aware dedup on
   name (case-insensitive) and `(address, port)` (case-insensitive,
   catches IDN collisions the server's ASCII `LOWER()` index
   misses), `is_submitting` guard, Tab cycle, Save persists to
   config with rollback-on-save-failure (Add: drop the new row;
   Edit: restore the original; Remove: re-insert at original Vec
   index via `Config::insert_tracker`).
5. **Network layer** — `nexus-client/src/network/tracker_query.rs`
   one-shot `query_tracker(tracker, locale, version)`. The
   `version` arg is the client's own crate version (sent as the
   required `version` field in `TrackerServerList`) so the tracker
   can filter to semver-compatible entries. Wire to a
   `Message::TrackerQueryResult { tracker_id, result }` handler
   that writes into the cache.
6. **Auto-fetch + cache wiring** — fetch on panel open / dropdown
   change / Refresh; in-flight handling per the cache-structure
   section above.
7. **AcceptFingerprint mode** — Stage 1 mismatch dialog, accept
   path commits the new pin and re-queries.
8. **Click-to-connect + context menu** — Name click and "Connect"
   menu item open Connect with pre-filled fields (no
   `add_bookmark` change). "Bookmark" menu item opens the Bookmark
   Add dialog pre-filled. "Copy URI" copies `nexus://` to
   clipboard.
9. **i18n keys** — 13 locales for everything new.
10. **Docs** — new chapter under `docs/client/` covering the panel
    from the user's perspective.
