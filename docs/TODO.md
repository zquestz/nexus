# Nexus TODO

## Implementation Order (Pre-Launch)

| # | Feature | Effort | Status |
|---|---------|--------|--------|
| 1 | Account groups | Low | ✅ Done |
| 2 | Password strength | Low | ✅ Done |
| 3 | Boards | High | Planned |
| 4 | File previews | Low | Planned |
| 5 | Trackers | Medium | Planned |
| 6 | Speed limiting | Medium | Planned |
| 7 | Flood protection | Medium | Planned |
| 8 | Server logs | Medium | Planned |
| 9 | Auto-away | Low | Planned |

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