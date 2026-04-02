# Nexus TODO

## Implementation Order (Pre-Launch)

| # | Feature | Effort | Status |
|---|---------|--------|--------|
| 1 | Account groups | Low | ✅ Done |
| 2 | Password strength | Low | ✅ Done |
| 3 | Streaming hash transfers | Medium | ✅ Done |
| 4 | Boards | High | Planned |
| 5 | File previews | Low | Planned |
| 6 | Trackers | Medium | Planned |
| 7 | Speed limiting | Medium | Planned |
| 8 | Flood protection | Medium | Planned |
| 9 | Server logs | Medium | ✅ Done |
| 10 | Auto-away | Low | ✅ Done |

**Post-launch:** IRC gateway (if demand exists)

## Decided Against

Features intentionally excluded with rationale.

| Feature | Reason |
|---------|--------|
| `/me's` (possessive) | i18n complexity — each language handles possessives differently |
| Disable encryption | Security — Nexus requires TLS always |
| File aliases | OS concern — admin can use filesystem symlinks |
| Process monitor | Out of scope — BBS server, not system management tool |
| Custom text colors | Novelty feature that makes chat hard to read |
| Folder comments | Use descriptive folder names instead |
| News categories | Flat list simpler for typical use cases |
| Remote shutdown | Docker/systemd auto-restart defeats purpose; users with container access can stop directly |
| File tree view | Tabs work well, tree view adds rendering complexity without real benefit |
| DCC | Peer-to-peer adds complexity; server-mediated transfers work well |
| Remote desktop | Most servers are headless; out of scope for BBS software |

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

### Server Logs

Structured logging for the server with file rotation and retention.

**Dependencies (nexus-server):**
- `tracing` — macros (`error!`, `warn!`, `info!`, `debug!`)
- `tracing-subscriber` — layer composition, stderr (human-readable), JSON formatter (file layer), level filtering
- `tracing-appender` — daily rolling file writer
- `humantime` — parse duration strings for `--log-retention`

**CLI Flags:**
- `--log-level` — `None` / `Error` / `Warn` / `Info` / `Debug` (default: `Info`)
- `--log-retention` — Human-readable duration (default: `30d`). `0` means stderr only (no file logging). Non-zero values must be at least `1d`.
- `--no-log-timestamps` — Disable timestamps in stderr output. Useful for Docker/systemd environments that provide their own timestamps. Does not affect JSON file output (always includes timestamps).

**Output:**
- **All output through tracing**: No `println!` or `eprintln!` in the codebase. Startup info, operational events, and errors all go through the tracing subscriber.
- **stderr**: Human-readable tracing layer (level, message, fields). Timestamps shown by default, disabled with `--no-log-timestamps`.
- **File**: JSONL via `tracing-subscriber` built-in JSON formatter + `tracing-appender` daily rolling writer. Always includes timestamps. Only active when retention is non-zero and log level is not None.
- **Log directory**: `~/.local/share/nexusd/logs/`

**Retention:**
Purge log files older than the retention period on startup and on a daily timer. Filename-based date check. When retention is `0`, no file layer is created, no log directory is used.

**Refactoring:**
- Replace all `println!()`, `eprintln!()` with `error!`/`warn!`/`info!`/`debug!`
- Remove `debug: bool` from `ConnectionParams`, `TransferParams`, `VoiceUdpServer`, `HandlerContext`, and everywhere it's threaded
- ~66 `if debug { eprintln!(...) }` blocks become plain `debug!()` calls

**Log level categories:**
- **Error**: Database failures, TLS errors, unexpected internal errors
- **Warn**: Permission denied, privilege escalation attempts, login failures
- **Info**: User connected/disconnected, login success, admin actions (ban/trust/kick/delete/create), transfer completed, file index rebuilt
- **Debug**: Connection limit hits, banned IP rejections, transfer progress, channel pruning, cache stats

**Not logged:**
- Chat messages
- Chat join/leave events

**Protocol change:**
Add `log_level` field to `ServerInfo` in `nexus-common`. Read-only, not settable via `ServerInfoUpdate`.

**Client changes:**
- Rename **Limits** tab to **General** in Server Info panel, move Version into tab
- Display items in General tab alphabetically: Log Level, Max Connections, Max Transfers, Min Password Strength, Version
- Reorder Channels tab alphabetically: Auto-Join, Persistent
- Rename label `label-connections-short` from "Connections" to "Max Connections" across all 13 locales
- Rename label `label-transfers-short` from "Transfers" to "Max Transfers" across all 13 locales
- Add i18n keys for log level label and values across all 13 locales

**Not in scope (v1):**
- Client fetching/viewing log contents
- Client-side log search/filter UI
- Admin changing log level at runtime via client
