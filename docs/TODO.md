# Nexus TODO

## Future Work

| Feature                              | Effort | Notes                  |
| ------------------------------------ | ------ | ---------------------- |
| Boards                               | High   | Spec TBD               |
| File previews                        | Low    | See feature spec below |
| Connection Monitor egress visibility | Medium | See feature spec below |
| BLAKE3 transfer hashes               | Medium | 0.9.x breaking change  |

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

### Connection Monitor Egress Visibility

- Connection Monitor integration: surface per-user current outbound rate and backlog in the admin UI.

### BLAKE3 Transfer Hashes

Migrate transfer hashes from SHA-256 to BLAKE3 in 0.9.x for faster large-file downloads/uploads and resume verification.

- Benchmark SHA-256 vs BLAKE3 on desktop and NAS-class hardware before choosing defaults.
