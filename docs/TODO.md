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
- ✅ Server-side tracker registration — complete on main (per-tracker
  tasks, admin protocol messages, TOFU pin, propagation contract). See
  [docs/protocol/09-admin.md](protocol/09-admin.md) and
  [docs/protocol/18-trackers.md](protocol/18-trackers.md).
- ✅ Protocol vocabulary aligned with the new pointer-shaped-entity
  convention — `TrackerCreate`/`TrackerDelete` renamed to
  `TrackerAdd`/`TrackerRemove` (and matching permission strings
  `tracker_add`/`tracker_remove`). New `TrackerAcceptFingerprint`
  message for the Stage 1 cert-rotation accept flow. Convention codified
  in `CLAUDE.md` "Naming Conventions" so future entities follow it.

**Remaining work** — v0.8.2 ships when these land:

| Item                            | Notes                                                                                                                                                 |
| ------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------- |
| Client-side admin UI            | Tracker management panel in `nexus-client` admin section; calls the tracker admin protocol messages. **See plan below.**                              |
| Client-side browser integration | `nexus-client` queries one or more trackers and surfaces the listing in the bookmarks / server-list UI. (Separate from the admin UI.)                 |
| 12-locale i18n backfill         | All client-side keys listed below need the other 12 locales after feature is functional. (Server-side `err-tracker-no-pending-fingerprint` is already complete in all 13.) |

#### Server Info Panel + Tracker Admin UI Plan (v0.8.2)

Locked end-to-end before code. Resume from this section if context is
lost. The **rule** of "match Users/Groups patterns EXACTLY" governs
ambiguities — when in doubt, mirror what `views/users.rs`, `views/groups.rs`,
`types/panel/users.rs`, and `handlers/user_management.rs` do.

##### Server Info panel restructure

Replace the current 3-tab `General | Chat | Files` display with
`Config | Trackers`, lift identity-shaped data to a panel header, and
move the `Edit` action into a per-tab toolbar.

Layout (top to bottom):

1. **Identity block** (always visible)
   - Server image, name, description, share URI (existing).
   - **Fingerprint** moves here from the General tab. Centered,
     wrapping with each line independently centered (use
     `rich_text` + `align_x(Center)`, same shape as the share URI).
     `Fingerprint:` label in regular font, hex value in monospace
     span. Click-to-copy via `on_link_click` →
     `Message::CopyServerFingerprint` + new `toast-fingerprint-copied`.
2. **Panel-level action error banner** (between identity and tabs,
   only when set) — mirrors `views/users.rs:1133-1165`. New field
   on the Server Info panel state. Carries non-form errors only:
   `TrackerList` fetch failure, `TrackerEdit` fetch failure (form
   can't open), refresh failure. Form errors stay in their own
   form's banner; modal errors stay in their own modal.
3. **Tab bar:** `Config | Trackers`. Trackers tab gated on
   `tracker_list`; reactive to `PermissionsUpdated`.
4. **Per-tab toolbar** (between tab bar and body, mirrors recent
   User Management refactor):
   - **Config:** single `[icon::edit()]` icon, left-aligned. Always
     visible, disabled when not admin. Clicking swaps the panel
     into the existing monolithic edit form (no change to that view
     for now — it's rarely accessed).
   - **Trackers:** `[icon::plus_circled()]` then `[icon::refresh()]`,
     both left-aligned, mirroring Groups. Add disabled without
     `tracker_add`. Refresh disabled only while a fetch is in flight
     (uses the `.is_some()` check on `all_trackers: Option<Result<…, _>>`
     — don't change to `.is_some_and(Result::is_ok)` or it locks up
     after a failed fetch).
5. **Body** (per tab — see sections below).
6. **No bottom button row.** Dismiss by opening another panel
   (consistent with User Management). No keyboard dismissal — we
   confirmed this is acceptable.

##### Config tab body

- Render the **available** fields (permission-gated per row),
  alphabetically sorted, into two **equal-width** columns
  (`row![Column.width(Fill), Space(SPACER_SIZE_LARGE), Column.width(Fill)]`).
  No empty slots — distribute the visible-to-this-user subset
  across the two columns top-to-bottom.
- Field set: Auto-Join Channels, Chat Burst Limit, Chat Rate
  Limit, File Reindex Interval, Log Level, Max Connections per
  IP, Max Transfers per IP, Min Password Strength, Persistent
  Channels, Version. (10 total before per-row gating.)
- "Reindex" label reuses existing `label-file-reindex-interval`
  (no new key).
- The fingerprint row is gone from Config — it's in the identity
  block. Remove the corresponding trigger from `has_general` /
  the equivalent visibility predicate.
- No refresh button — server already broadcasts `ServerInfoUpdate`
  and the panel redraws live.

##### Trackers tab body

- Sortable table with default sort **Name ascending**. Columns:
  `Status | Name | Address | Port`. Match `Table Consistency`
  rules in `CLAUDE.md`:
  - Stable headers with `Space::new().width(SORT_ICON_SIZE)` placeholder.
  - Wrap with `lazy()` + a `TrackerListDeps` struct (manual `Hash`
    impl matching rendered data: trackers list, sort column,
    sort direction).
  - Per-column wrapping: Status `Word`, Name `WordOrGlyph`,
    Address `WordOrGlyph`, Port `Word`, headers `Word`.
- **Status indicator:** themed-color Unicode bullet `●` at
  `TEXT_SIZE`, top-aligned (so multi-line rows render the bullet
  next to the first line):
  - `theme.success` (green) → connected
  - `theme.warning` (yellow) → `pending_fingerprint.is_some()` (Stage 1)
  - `theme.danger` (red) → disconnected / `last_error` set / Stage 2
    mismatch (which is unrecoverable; `pending_fingerprint` is `None`).
  - Tooltip on hover surfaces `last_error` (server-side translated)
    or `"Connected"` / `"Fingerprint mismatch — review and accept"`.
- **Right-click row** → `LazyContextMenu` with `MenuButton` items
  (no icons, text-only — Users/Groups parity):
  - **Accept Fingerprint** — only when `tracker.pending_fingerprint.is_some()`,
    positioned above Edit. Gated on `tracker_edit`.
  - **Edit** — gated on `tracker_edit`.
  - **Remove** — gated on `tracker_remove`. `menu_button_danger_style`.
- **Empty state** — match Users/Groups exactly (same key/style).

##### Add Tracker subview

Triggered by clicking `[+]` in the Trackers toolbar. Swaps the
**entire panel content** into a tracker-add form (panel
header/tabs/toolbar gone, replaced by form title + Cancel/Save
row — same shape as User Management Create).

- Title: `title-add-tracker` (new).
- Fields: name, address, port (`NumberInput`), fingerprint
  (optional), password (optional), enabled (checkbox).
- Validators already in `nexus-common/src/validators/`:
  `validate_tracker_name`, `validate_tracker_address`,
  `MAX_PASSWORD_LENGTH`, fingerprint canonical-form check.
  Translated via the handler-error-helper pipeline.
- Submit button: `button-add` (verify if exists; new × 13 if not).
  Edit form's submit stays `button-save`.
- `is_submitting: bool` guard. Validate-then-submit on Enter
  (`on_submit` dispatches `Validate*` when incomplete,
  `*Pressed` when complete). Submit uses `on_press_maybe`.
- Wire: `ClientMessage::TrackerAdd { … }` →
  `TrackerAddResponse { success, error, id, name }`. On success
  close subview, refetch list, show `toast-tracker-added`. On
  error display in form banner.

##### Edit Tracker subview

Triggered by right-click → Edit. **Refetches via `TrackerEdit`**
(don't use the cached list row — User/Group parity rule).

- Send `TrackerEdit { id }` → receive
  `TrackerEditResponse { tracker: Option<TrackerInfo> }`. On
  success populate the form. On error: form can't open; surface
  error in the **panel-level** banner (the form has nowhere to
  show it because it never opened).
- Title: `title-edit-tracker` (new).
- Same fields as Add, prefilled from `tracker`. Form's banner
  shows `last_error` when present so admins see the operational
  context while editing.
- Submit: `button-save` (reuse). Submit disabled when
  `has_changes()` returns false.
- `is_submitting: bool` guard.
- Wire: `ClientMessage::TrackerUpdate { id, … all fields }` →
  `TrackerUpdateResponse { success, error, id, name }`. On
  success close, refetch list, show `toast-tracker-updated`.

##### Accept Fingerprint dialog

Modal, **mirrors `views/fingerprint.rs` structure almost verbatim**.
Shown when a row has `pending_fingerprint` set and the admin
chooses Accept Fingerprint from the row's context menu.

- Reuse: `format_fingerprint_multiline()`, `FINGERPRINT_SPACE_*`
  constants, `scrollable_modal` wrapper, two-button row pattern
  (secondary Cancel + danger Accept, both `width(Fill).center()`).
- Reuse: `label-expected-fingerprint` and `label-received-fingerprint`
  (semantically: BBS server _expected_ the configured fingerprint,
  _received_ the new one from the tracker — saves 26 locale entries).
- Identity line: `tracker_name - host:port` (no label, same format
  as the existing dialog).
- New keys (× 13 each): `title-accept-fingerprint`,
  `tracker-fingerprint-warning` (framed as routine verification:
  "verify the new fingerprint via a trusted channel before accepting"),
  `button-accept-fingerprint`, `toast-tracker-fingerprint-accepted`,
  `err-tracker-accept-fingerprint-failed`.
- Wire: `ClientMessage::TrackerAcceptFingerprint { id }` →
  `TrackerAcceptFingerprintResponse { success, error, id, name }`.
  Server promotes the row's `pending_fingerprint` to active
  `fingerprint` atomically and respawns the task.
- On success close dialog, refetch list, show success toast. On
  error display in dialog.
- Visibility rule: the menu item shows only when
  `tracker.pending_fingerprint.is_some()`. Server semantics
  guarantee that's only ever true for Stage 1 mismatches —
  Stage 2 mismatches never populate it, so the UI never offers
  Accept for Stage 2 (which is unrecoverable; admin must Edit
  or Remove).

##### Remove Tracker confirm modal

- Stays open until server responds (delete-confirmation pattern).
- Title: `dialog-remove-tracker-title` (new × 13).
- Body: `dialog-remove-tracker-body` (new × 13).
- Buttons: `button-cancel` (reuse) + `button-remove` (new × 13,
  also serves as the row context menu label and dialog action),
  danger-styled.
- `is_remove_submitting: bool` (separate from `is_submitting`
  since remove can overlap with editing).
- Wire: `ClientMessage::TrackerRemove { id }` →
  `TrackerRemoveResponse { success, error, name }`. On success
  close + refetch + `toast-tracker-removed`. On error display
  in modal with retry/cancel.

##### Permission-gating rules

| Permission       | Gates                                                                 |
| ---------------- | --------------------------------------------------------------------- |
| `tracker_list`   | Trackers tab visibility; Refresh button                               |
| `tracker_add`    | Add Tracker button enable state (always visible, disabled if missing) |
| `tracker_edit`   | Edit + Accept Fingerprint context menu items; Edit form submit        |
| `tracker_remove` | Remove context menu item; Remove confirm submit                       |

Reactive to `PermissionsUpdated` via existing client-side wiring
— no new plumbing needed.

##### Data flow

- **On Server Info panel open:** if user has `tracker_list`,
  fetch `TrackerList` in parallel with whatever else opens with
  the panel (mirrors User Management's `UserList` + `GroupList`
  parallel prefetch in `handlers/user_management.rs`).
- **Config fields:** continue to update live via existing
  `ServerInfoUpdate` broadcasts. No refresh path needed.
- **Tracker list:** manual refresh only. **No auto-poll** —
  state changes (connected/error/pending) would be noisy and
  push isn't worth it for this surface.
- **After successful Add / Update / Remove / Accept Fingerprint:**
  refetch via `TrackerList` (User Management parity).

##### User Management feature parity (mandatory)

These are non-negotiable patterns — replicate exactly, don't
reinvent. References point at the canonical implementation site.

- Sortable columns w/ stable headers (`SORT_ICON_SIZE` placeholder)
  — `views/users.rs`.
- `lazy()` + `*Deps` struct caching — `views/users.rs:lazy(...)`.
- `LazyContextMenu` + `MenuButton` + `menu_button_style` /
  `menu_button_danger_style` — `widgets/`.
- Standardized icon sizing: `ICON_SIZE`, `TOOLBAR_BUTTON_PADDING`,
  `HEADING_BUTTON_PADDING` — `style/icons.rs` + `style/layout.rs`.
- `tab_toolbar_icon_button(...)` helper — `views/helpers.rs`.
- `is_submitting` / `is_remove_submitting` guards (per CLAUDE.md
  "Double-Submit Prevention").
- Validate-then-submit on Enter (per CLAUDE.md "Double-Submit
  Prevention" subsection).
- Per-field validation via `validators/` → `handlers/errors.rs`
  helper pipeline (no raw error strings).
- Confirm modal stays open until server response (per CLAUDE.md
  "Delete Confirmation Dialog Pattern").
- Panel-level action-error banner (mirrors recent User Management
  refactor, commit `956d7e46`).

##### i18n key slate (English-only during dev; backfill later)

| Key                                     | Status      | Notes                                     |
| --------------------------------------- | ----------- | ----------------------------------------- |
| `tooltip-add-tracker`                   | new × 13    | toolbar tooltip                           |
| `tooltip-refresh`                       | reuse       | toolbar tooltip                           |
| `title-add-tracker`                     | new × 13    | Add subview title                         |
| `title-edit-tracker`                    | new × 13    | Edit subview title                        |
| `title-accept-fingerprint`              | new × 13    | Accept dialog title                       |
| `tracker-fingerprint-warning`           | new × 13    | Accept dialog warning text                |
| `label-expected-fingerprint`            | reuse       | from existing fingerprint dialog          |
| `label-received-fingerprint`            | reuse       | from existing fingerprint dialog          |
| `label-fingerprint`                     | reuse       | identity-block label                      |
| `label-file-reindex-interval`           | reuse       | Config tab row (replaces shorter key)     |
| `button-add`                            | new × 13    | Add form submit (verify if exists first)  |
| `button-save`                           | reuse       | Edit form submit                          |
| `button-edit`                           | reuse       | row context menu                          |
| `button-remove`                         | new × 13    | row context menu + confirm modal action   |
| `button-cancel`                         | reuse       | forms / dialogs                           |
| `button-accept-fingerprint`             | new × 13    | Accept dialog action + context menu label |
| `dialog-remove-tracker-title`           | new × 13    | confirm modal                             |
| `dialog-remove-tracker-body`            | new × 13    | confirm modal                             |
| `toast-tracker-added`                   | new × 13    | success                                   |
| `toast-tracker-updated`                 | new × 13    | success                                   |
| `toast-tracker-removed`                 | new × 13    | success                                   |
| `toast-tracker-fingerprint-accepted`    | new × 13    | success                                   |
| `toast-fingerprint-copied`              | new × 13    | identity-block click-to-copy              |
| `err-tracker-add-failed`                | new × 13    | action error banner                       |
| `err-tracker-update-failed`             | new × 13    | action error banner                       |
| `err-tracker-remove-failed`             | new × 13    | action error banner                       |
| `err-tracker-accept-fingerprint-failed` | new × 13    | action error banner                       |
| `permission-tracker_add`                | rename × 13 | from `permission-tracker_create`          |
| `permission-tracker_remove`             | rename × 13 | from `permission-tracker_delete`          |

Plus tracker form-field labels/placeholders (name / address / port /
fingerprint / password / enabled) and per-field validation error keys.
Enumerate during implementation.

##### Recommended implementation order

1. **Types layer.** New tracker-management state struct (per-tab list /
   subview / modal state, error banner field). New `Message` variants.
   Wire `ClientMessage` send paths.
2. **Network response handlers.** Six tracker responses
   (`TrackerListResponse`, `TrackerAddResponse`, `TrackerEditResponse`,
   `TrackerUpdateResponse`, `TrackerRemoveResponse`,
   `TrackerAcceptFingerprintResponse`).
3. **Server Info panel restructure** — identity-block fingerprint
   move, tab consolidation, per-tab toolbar plumbing, two-column
   alphabetical Config rendering.
4. **Trackers tab list view** — sortable table, status bullet,
   `LazyContextMenu`.
5. **Add Tracker subview.**
6. **Edit Tracker subview** with `TrackerEdit` refetch on entry.
7. **Remove confirm modal.**
8. **Accept Fingerprint dialog.**
9. **Permission-gated visibility/enable wiring across all surfaces.**
10. **English locale entries** for all new/renamed keys.
11. **Browser test pass** — all flows including error states.
12. **12-locale i18n backfill** — last (single batched pass).

##### Design rationale (so we don't re-litigate)

- **Add/Remove on the wire (not just UI):** trackers are
  pointer-shaped — the row points at an external service, doesn't
  _own_ a created resource. Convention added to `CLAUDE.md` so
  future pointer-shaped entities follow the same vocabulary
  without reopening the debate.
- **Dedicated `TrackerAcceptFingerprint`** (vs reusing
  `TrackerUpdate` with the new fingerprint pasted in): smaller
  payload (server already has `pending_fingerprint`, client just
  says "promote it"); cleaner audit-log wording; atomic by
  construction (no client-supplied data to validate). Not a
  security argument — `tracker_edit` gates both paths.
- **Stage-2 unrecoverable:** server only populates
  `pending_fingerprint` on Stage 1 mismatches by construction.
  Stage 2 (TLS-observed vs server-self-reported) is an
  interception signal; admin recovery paths are Edit (manual
  fingerprint entry with out-of-band verification) or Remove.
  No UI affordance attempts to bypass this.
- **Match Users/Groups exactly:** the Sept-2025 User Management
  refactor (commit `956d7e46`) is recent, validated, and ships
  the exact widgets/state-machines we need. Reinventing them is
  zero-upside.
- **Themed bullet for status (vs new icons):** zero font work,
  fits the existing extended palette (`theme.success`/`warning`/`danger`),
  scales nicely. Pattern proven in many apps (Discord, Slack).
- **No auto-poll:** state churn would create UI noise; manual
  refresh is cheap and matches Connection Monitor's existing
  behavior.
- **`TrackerEdit` refetch (not cached row) on Edit click:** strict
  Users/Groups parity. `TrackerEditResponse` returns the latest
  full `TrackerInfo` including current `last_error` and
  `pending_fingerprint` so the form can show operational
  context while editing.
- **Single-icon Config toolbar accepted:** thin visually but
  preserves the "every tab has a toolbar" pattern. Alternative
  was Edit on the right side of the tab bar — non-standard,
  rejected.
- **No keyboard dismissal for the panel:** consistent with all
  other Nexus panels (User Management, News). Acceptable.

#### Discovered during planning, not yet decided

- **Sort state lifecycle on the Trackers table** — does the
  user's chosen sort survive panel close? Survive a server
  reconnect? Match whatever User Management's table does
  rather than reinventing.
- **Form field layout for Add/Edit Tracker** — the fields and
  validators are pinned, but the visual layout (single column,
  two columns, paired rows) is left to implementation. Match
  whichever pattern Add/Edit User uses.
