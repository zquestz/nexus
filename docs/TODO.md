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
| 7   | Tracker discovery           | Low    | ✅ Done |
| 8   | Speed limiting              | Medium | Planned |
| 9   | Flood protection            | Low    | ✅ Done |
| 10  | Server logs                 | Medium | ✅ Done |
| 11  | Auto-away                   | Low    | ✅ Done |
| 12  | Invite system               | Medium | Planned |
| 13  | Certificate fingerprint pin | Low    | ✅ Done |

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
