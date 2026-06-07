# Nexus TODO

## Future Work

| Feature                              | Effort | Notes                         |
| ------------------------------------ | ------ | ----------------------------- |
| Boards                               | High   | Include reactions and search  |
| File previews                        | Low    | See feature spec below        |
| Admin event history                  | Medium | See feature spec below        |
| Offline messages investigation       | Medium | See investigation notes below |
| Connection Monitor egress visibility | Medium | See feature spec below        |

## Feature Specs

### Boards

Persistent discussion boards for longer-form server/community threads.

**Design notes:**

- Include board/thread/post model in the initial spec.
- Include board search as part of the shipped Boards feature, not a later bolt-on.
- Consider lightweight post reactions for acknowledgement without reply noise.
- Keep reactions scoped to Boards unless chat reactions have a separate, clear product reason.

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

### Admin Event History

- Investigate a persistent admin-facing event history for server operations.
- Candidate events: logins, disconnects, bans/trust changes, user/group changes, file operation failures, transfer failures, tracker failures, and server config changes.
- Keep this distinct from chat/news/user-facing notifications.
- Include retention limits and permission gates in the design.

### Offline Messages Investigation

- Investigate offline private messages as a mailbox feature for disconnected users.
- Decide whether this is encrypted-at-rest mailbox storage or true client-verifiable end-to-end encryption.
- If claiming end-to-end encryption, include recipient key pinning/verification in the design so the server cannot silently substitute recipient keys.
- Include queue limits, expiration, delivery acknowledgements, and behavior for shared accounts/multiple sessions.
