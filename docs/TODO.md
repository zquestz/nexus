# Nexus TODO

## Implementation Order (Pre-Launch)

| #   | Feature                     | Effort | Status  |
| --- | --------------------------- | ------ | ------- |
| 1   | Account groups              | Low    | ✅ Done |
| 2   | Password strength           | Low    | ✅ Done |
| 3   | Streaming hash transfers    | Medium | ✅ Done |
| 4   | Boards                      | High   | Planned |
| 5   | File previews               | Low    | Planned |
| 6   | Tracker registration        | Medium | ✅ Done |
| 7   | Tracker discovery           | Low    | Planned |
| 8   | Speed limiting              | Medium | Planned |
| 9   | Flood protection            | Low    | ✅ Done |
| 10  | Server logs                 | Medium | ✅ Done |
| 11  | Auto-away                   | Low    | ✅ Done |
| 12  | Invite system               | Medium | Planned |
| 13  | Certificate fingerprint pin | Low    | Planned |

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

### Certificate Fingerprint Pin

Optional fingerprint field on the Connect dialog and Bookmark
Add / Edit forms. Lets users pin a server's TLS certificate
fingerprint out-of-band before the first connection, defending
against MITM-on-first-connect when the user got the fingerprint from
a trusted source.

Independent of tracker discovery (planning entry #7), but a
prerequisite — the tracker-discovery panel will pre-fill this field
from each tracker-advertised entry so users can see what the tracker
claimed before connecting.

**Forms affected:**

- Connect dialog (`views/connection.rs`)
- Bookmark Add modal (`views/bookmark.rs`)
- Bookmark Edit modal (`views/bookmark.rs`)

**Field placement:**

- After Nickname, before the existing Add Bookmark checkbox on the
  Connect dialog; in the equivalent slot on the Bookmark forms.
- Single-line `text_input`. Optional — empty input is valid and
  means unpinned.
- Placeholder text showing the canonical format.
- On Bookmark Edit, the field is pre-filled with the saved pin (if
  any). Editing replaces it; clearing removes it.

**Tab order:**

Last input field on each form. Existing tab cycle is updated to
include the new field as the final stop before the submit button.
Care needed around `NumberInput` (Port), which consumes Tab and is
already routed around per the CLAUDE.md "UI Quirks" note.

**Validation (on submit only, not live):**

- Empty / whitespace-only → unpinned, no error.
- Leading and trailing whitespace trimmed before validation.
- **Case is strict — uppercase only.** Lowercase input is rejected
  with a localized inline error rather than auto-corrected.
  Rationale: legitimate sources (mismatch dialogs, tracker listings,
  bookmark copy) always emit uppercase canonical form. Lowercase =
  manually typed = likely wrong or malicious; silently uppercasing
  would mask the signal.
- Must match canonical form: 32 hex bytes separated by `:`, exactly
  95 characters total, hex digits `0`–`9` / `A`–`F` only.
- Validator lives in `nexus-common/src/validators/`. Handlers
  translate validator errors to localized strings via the existing
  per-error i18n key pattern.

**TOFU integration:**

- Empty form value: behaves identically to today — TOFU on the first
  connection commits the observed fingerprint.
- Non-empty form value: treated as a Stage 1 pin. The pre-login
  fingerprint check in `network/connect.rs` compares it against the
  TLS-observed fingerprint; mismatch fires the existing mismatch
  dialog.
- Mismatch acceptance with a form-supplied value: accept = use the
  TLS-observed value for this connection. If Add Bookmark was
  checked, the resulting bookmark stores the TLS-observed value, not
  the originally-typed value. Reject = abort the connection.

**i18n:**

13 locales. New keys for the field label, placeholder, and each
distinct validation failure (invalid length, invalid hex, missing
or misplaced separator, must-be-uppercase).

**Out of scope:**

- No "Advanced" collapsible. The fingerprint field is just one more
  optional input, always visible.
- No automatic case-fixing or formatting on the user's behalf.

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
- **Listing**: 3 sortable columns — Name (Fill), Description (Fill),
  Users (fixed width, with sort-arrow space). Default sort Name asc
  (matches wire order). Sort arrows use
  `icon::down_dir()` / `icon::up_dir()` matching the files table.
  No context menu in v1.

**State variants on the toolbar row:**

| Trackers     | Pulldown | Edit / Remove | Refresh  | Status             |
| ------------ | -------- | ------------- | -------- | ------------------ |
| Zero         | disabled | disabled      | disabled | empty              |
| 1+, fetching | enabled  | enabled       | disabled | `Loading…` muted   |
| 1+, success  | enabled  | enabled       | enabled  | `N servers`        |
| 1+, error    | enabled  | enabled       | enabled  | `Error: <msg>` red |

Refresh-disable rule mirrors User Management: gated on
`Option<Result<Vec<ServerEntry>, String>>::is_some()` so that an
error response re-enables the button (otherwise the user gets
stuck after a failed fetch).

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
  selected, or first configured tracker if no prior selection).
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

**Empty / error states:**

- **No trackers configured**: empty-state message in the list area
  pointing at the `[+]` button (matches how other list views
  surface empty-state copy in the list area, not as a separate
  empty-state CTA).
- **Tracker fetch failed**: localized error message in place of the
  table (transport, auth, rate-limit). Status text in the toolbar
  also shows the same error in red.
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
   Quirks" note) → Password (secure) → Fingerprint. `Cancel` +
   `Save` buttons. `is_submitting` guard. Validate-then-submit on
   Enter.
3. **Edit** — same fields as Add, all editable (including address
   and port — the user might want to point an existing entry at a
   different tracker without losing other state). Pre-populated
   with current values.
4. **ConfirmRemove** — confirmation view with the tracker's display
   name. `Cancel` + `Remove` buttons.
5. **AcceptFingerprint** — Stage 1 TOFU mismatch dialog, mirroring
   `views/trackers.rs::accept_fingerprint_modal` (the BBS-admin
   side). Title, server-identification line, warning, expected /
   received fingerprints in monospace multiline form, `Cancel` +
   `Accept` buttons. Cancel returns to List with the mismatch error
   shown in the toolbar status text next to the pulldown.

**Keyboard shortcuts:**

- **Escape** → cancel, return to List mode (works in Add / Edit /
  ConfirmRemove / AcceptFingerprint).
- **Enter** on a text_input → validate-then-submit (handler routes
  to `Validate*` if incomplete, `*Pressed` if complete).
- **Tab** → cycles between text inputs only; the Port `NumberInput`
  is skipped (consumes Tab internally per CLAUDE.md UI Quirks).

**Click-to-connect from listing:**

- Clicking the **Name** column on a server row opens the existing
  Connect dialog (`views/connection.rs`) with name / address /
  port / fingerprint pre-filled from the tracker entry.
- The Add Bookmark checkbox in the Connect dialog defaults
  **checked** when launched from a tracker entry (the user already
  curated this from a directory; saving as a bookmark is the
  expected default).
- The tracker-supplied fingerprint is a display aid only; the
  BBS-side two-stage TOFU runs from scratch when the user actually
  connects (per spec: "The listed fingerprint is a display aid,
  not a trust assertion").

**Network layer (`nexus-client/src/network/tracker_query.rs`):**

- One-shot `query_tracker(tracker, locale) -> Result<Vec<ServerEntry>, TrackerError>`.
- TLS connect → handshake → two-stage fingerprint check (Stage 1
  vs stored pin if present, Stage 2 vs `HandshakeResponse.fingerprint`)
  → send `TrackerServerList { password, locale }` → receive
  `TrackerServerListResponse` → close.
- Stage 1 mismatch fires the `AcceptFingerprint` mode dialog; on
  accept, rotates the pin and re-queries. Stage 2 mismatch aborts
  with no accept path (active interception).
- On first successful query (no prior pin), commits the
  TLS-observed fingerprint to the tracker config row.
- Reuses `resolve_host_for_connection`, proxy bypass logic, and TLS
  patterns from existing `network/` code.

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

**Out of scope (v1):**

- No "All trackers" aggregation view — Hotline-style per-tracker
  view is simpler and avoids cross-tracker dedup decisions.
- No WebSocket transport for tracker queries; client uses TCP
  only.
- No timer-based auto-refresh.
- No persistent on-disk cache of fetched server lists.
- No context menu on the listing (right-click → connect / copy
  URI / save as bookmark) — possible future addition.
