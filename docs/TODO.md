# Nexus TODO

## Future Work

| Feature                              | Effort | Notes                         |
| ------------------------------------ | ------ | ----------------------------- |
| Boards                               | High   | Include reactions and search  |
| File previews                        | Low    | See feature spec below        |
| Admin event history                  | Medium | See feature spec below        |
| Offline messages investigation       | Medium | See investigation notes below |
| Connection Monitor egress visibility | Medium | See feature spec below        |
| Protocol consistency cleanup         | Medium | Finish 0.9.0 audit            |

## Feature Specs

### Boards

Persistent discussion boards for longer-form server/community threads.

**Design notes:**

- Include board/thread/post model in the initial spec.
- Include board search as part of the shipped Boards feature, not a later bolt-on.
- Consider lightweight post reactions for acknowledgement without reply noise.
- Keep reactions scoped to Boards unless chat reactions have a separate, clear product reason.

**Board metadata scope:**

- Boards are the community/container object, similar to a Reddit community.
- Posts/threads are distinct objects and should be designed separately.
- Board IDs are UUIDv7 values. This is intentional for boards and does not
  imply changing older integer-ID resources.
- Use plain `id` in board-specific protocol messages, following existing Nexus
  CRUD patterns.

**Board fields:**

- `id`: UUIDv7 board ID as a canonical string.
- `name`: display name.
- `name_lower`: `fold_name(name)` for Unicode-aware case-insensitive lookup and
  uniqueness.
- `slug`: user-defined Unicode URI slug.
- `slug_lower`: `fold_name(slug)` for Unicode-aware case-insensitive lookup and
  uniqueness.
- `description`: Markdown description, empty string allowed.
- `icon`: optional image data URI.
- `banner`: optional image data URI.
- `creator_id`: server-owned user ID of the account that created the board, or
  `null` if that account has been deleted.
- `creator`: display username for client presentation, or `<deleted>` when
  `creator_id` is `null`. This sentinel must not be a valid username.
- `enabled`: client-supplied on create; editable later.
- `created_at`: Unix epoch seconds, signed integer.
- `updated_at`: Unix epoch seconds, signed integer.

**Database notes:**

- `id TEXT PRIMARY KEY`.
- `name TEXT NOT NULL UNIQUE COLLATE NOCASE`.
- `name_lower TEXT NOT NULL UNIQUE`.
- `slug TEXT NOT NULL UNIQUE COLLATE NOCASE`.
- `slug_lower TEXT NOT NULL UNIQUE`.
- `description TEXT NOT NULL`.
- `icon TEXT`.
- `banner TEXT`.
- `creator_id INTEGER REFERENCES users(id) ON DELETE SET NULL`; do not
  cascade-delete boards when users are deleted.
- `enabled INTEGER NOT NULL`.
- `created_at INTEGER NOT NULL`.
- `updated_at INTEGER NOT NULL`.
- On create, set `created_at == updated_at`.
- On update, preserve `created_at` and advance `updated_at`.
- Default board list ordering is `ORDER BY name_lower ASC`.
- Map duplicate `name_lower` and duplicate `slug_lower` separately so the
  client can report the exact field that conflicts.
- Server generates UUIDv7 IDs on create. Validate/parse board IDs as UUIDv7
  where accepting them from clients.

**Slug validation:**

- User-defined only; never server-generated.
- Slug is required on create.
- Trimmed value must be non-empty.
- Trim before validation/storage.
- Maximum 64 characters.
- Reject control characters.
- Reject whitespace.
- Reject URI reserved characters: `: / ? # [ ] @ ! $ & ' ( ) * + , ; =`.
- Reject `%` so percent-encoded and literal forms cannot alias.
- Board protocol CRUD messages remain ID-based; slugs are reserved for future
  board URI/canonical-link support.
- Slug uniqueness uses `fold_name(slug)` only. Do not add extra Unicode
  normalization for board slugs.
- Client forms must not auto-generate slugs from names and must not auto-update
  slug when the board name changes. Users enter and edit slugs explicitly.

**URI support:**

- Add `nexus://host/boards` support to open the main Boards view.
- Do not add board slug/detail routes in this scope.
- Do not add board highlighting/selection from URI in this scope.

**Future board slug URI support:**

- Future board detail URI shape: `nexus://host/boards/{slug}`.
- Board-specific generated share URIs use the slug, not the UUID.
- Generate board-specific share URIs now from the context menu, even before the
  slug/detail route is handled.
- Generated board-specific share URIs percent-encode the Unicode slug as a
  UTF-8 path segment; UI display may show the original Unicode slug.
- Opening a future board detail/share URI decodes the slug, resolves it through
  `fold_name(decoded_slug)`, and opens the board detail route if it exists and
  is visible to the user.
- Missing, disabled, or invisible boards use the normal not-found/error path.
- Future board detail URI visibility follows the same rules as `BoardShow`;
  disabled boards that are not visible to the requester behave as not found.
- Slug changes immediately change the canonical board URI. Do not preserve slug
  history or redirects for old slugs.

**ID validation:**

- Dedicated `validate_board_id` validator.
- Accept canonical UUID strings only.
- Require UUID version 7.
- Reject empty, malformed, and non-v7 UUIDs.
- Distinguish invalid board ID from valid-but-missing board not found errors.

**Description validation:**

- Dedicated `validate_board_description` validator.
- Maximum 4096 characters.
- Empty string allowed.
- Markdown allowed.
- Preserve description text as submitted; do not trim automatically.
- Same control-character rules as news body: allow newline, carriage return,
  and tab; reject other control characters.

**Name validation:**

- Dedicated `validate_board_name` validator.
- Trimmed value must be non-empty.
- Trim before validation/storage.
- Maximum 64 characters.
- Unicode allowed.
- Spaces and punctuation allowed.
- Reject control characters, including newline and tab.
- Uniqueness uses `fold_name(name)` stored in `name_lower`.

**Image validation:**

- Use board-specific validators even when rules match existing image classes.
- Supported formats: PNG, JPEG, WebP, SVG.
- No server-side intrinsic width/height validation.
- `validate_board_icon`: max data URI length `352_000`; client caches raster
  icons at `128px` square and renders smaller fixed square sizes where needed.
- `validate_board_banner`: max data URI length `700_000`; client caches/renders
  constrained to board content width, like news/server images.

**Validator and error coverage:**

- Add board-specific validators under `nexus-common/src/validators/`:
  - `validate_board_id`
  - `validate_board_name`
  - `validate_board_slug`
  - `validate_board_description`
  - `validate_board_icon`
  - `validate_board_banner`
- Add localized server errors for:
  - board not found
  - invalid board ID
  - board name already exists
  - board slug already exists
  - board name required / too long / invalid characters
  - board slug required / too long / invalid characters
  - board description too long / invalid characters
  - board icon too large / invalid format / unsupported type
  - board banner too large / invalid format / unsupported type
  - cannot delete board with posts unless the requester has `board_delete`

**Feature and permissions:**

- Add client feature `boards`.
- Add permissions:
  - `board_list`
  - `board_create`
  - `board_edit`
  - `board_delete`
- `BoardList` and `BoardShow` require `board_list`.
- `BoardCreate` requires `board_create`.
- `BoardEdit` and `BoardUpdate` are allowed for the creator or users with
  `board_edit`. If `creator_id` is `null`, only `board_edit` can edit the
  board.
- `BoardDelete` is allowed for the creator only while the board has no posts;
  deleting a board with posts requires `board_delete`. If `creator_id` is
  `null`, only `board_delete` can delete the board.
- Users with `board_edit` can edit any board.
- Users with `board_delete` can delete any board.
- `board_edit` and `board_delete` do not imply `board_list`; grant `board_list`
  separately when browsing/listing is intended.
- Admins bypass board permissions through the existing permission model.
- Groups and non-admin users use the normal explicit permission grant/revoke
  system.
- Shared accounts may list/show boards with `board_list`.
- Shared accounts cannot be granted `board_create`, `board_edit`, or
  `board_delete`, so they cannot create, edit/update, or delete boards through
  valid permission state.
- Only `board_list` is shared-account-allowed. `board_create`, `board_edit`,
  and `board_delete` are not shared-account-allowed.
- Include `board_list` in the client's normal default user permission set. The
  existing shared-account toggle will keep it because it is shared-allowed and
  will disable board management permissions because they are not shared-allowed.

**Visibility:**

- Enabled boards are visible to users with `board_list`.
- Disabled boards are visible only to:
  - the creator
  - users with `board_edit`
  - users with `board_delete`
- `BoardEdit` can fetch disabled boards for users allowed to edit them.
- Disabled boards behave as not found for users who cannot see them. Do not
  reveal that the row exists but is disabled.

**Protocol messages:**

- Client to server:
  - `BoardList`
  - `BoardShow { id }`
  - `BoardEdit { id }`
  - `BoardCreate { name, slug, description, icon?, banner?, enabled }`
  - `BoardUpdate { id, name?, slug?, description?, icon?, banner?, enabled? }`
  - `BoardDelete { id }`
- Server to client:
  - `BoardListResponse { success, error?, boards? }`
  - `BoardShowResponse { success, error?, board? }`
  - `BoardEditResponse { success, error?, board? }`
  - `BoardCreateResponse { success, error?, board? }`
  - `BoardUpdateResponse { success, error?, board? }`
  - `BoardDeleteResponse { success, error?, id? }`
  - `BoardUpdated { action, id }`
- `BoardAction` values: `Created`, `Updated`, `Deleted`.
- Shared object:
  - `Board { id, name, slug, description, icon?, banner?, creator_id, creator,
enabled, created_at, updated_at }`
- `BoardListResponse` returns full board metadata needed for list/grid
  rendering, but omits `banner` to avoid shipping large board-header art for
  every listed board.
- `BoardList` returns all visible boards with no pagination.
- Do not impose an artificial max board count. The server operator can decide
  whether their board count/image usage is appropriate for their deployment.
- `BoardShowResponse`, `BoardEditResponse`, `BoardCreateResponse`, and
  `BoardUpdateResponse` return the full board object including `banner`.
- `BoardShow` and `BoardEdit` return the same full board shape. `BoardShow` is
  the browse/read path and requires `board_list`; `BoardEdit` is the edit-form
  path and requires creator ownership or `board_edit`.

**Update semantics:**

- `BoardUpdate` is partial.
- Omitted fields are unchanged.
- `name`, `slug`, `description`, and `enabled` replace the stored value when
  present.
- `icon` and `banner`: omitted means unchanged, empty string clears, non-empty
  data URI replaces.
- Do not trim icon/banner data URI values; validate exact submitted value.
- `created_at`, `updated_at`, `creator_id`, and `creator` are never accepted
  from create/update requests.
- Create and update are all-or-nothing: validate every provided field before
  mutation, compute folded keys after validation, and leave the row unchanged on
  validation or duplicate-name/duplicate-slug failure.
- Advance `updated_at` only after a successful mutation.
- Stale concurrent edits use last-write-wins semantics; no optimistic
  `updated_at` precondition is required.

**Delete semantics:**

- Boards-only v1 hard-deletes the board row.
- Delete is allowed for the creator or users with `board_delete`.
- Broadcast uses pre-delete visibility.
- `BoardDeleteResponse` returns the deleted `id`.
- When posts exist, creator-only delete is allowed only while post count is
  zero. Deleting a board with posts requires `board_delete`; decide then whether
  destructive delete cascades posts or requires an explicit confirmation path.

**Broadcast behavior:**

- `BoardUpdated { action, id }` carries only action and ID.
- Originator is excluded from the broadcast because they receive the typed
  response.
- `BoardCreateResponse` and `BoardUpdateResponse` carry the full board object;
  `BoardDeleteResponse` carries the deleted ID.
- Broadcasts require the client `boards` feature.
- Broadcast to users with `board_list` who could see the board before or after
  the change.
- Enabled board changes broadcast to all users with `board_list`.
- Disabling an enabled board still broadcasts to all users with `board_list` so
  clients know to remove or refresh the visible board.
- Changes that start and remain disabled broadcast only to users with
  `board_list` who can see disabled boards: creator, `board_edit`, or
  `board_delete`.
- Deleting a disabled board follows the same disabled visibility rule; deleting
  an enabled board broadcasts to all users with `board_list`.

**Frame limits:**

- Add explicit per-field limits for board IDs, names, slugs, descriptions,
  icons, banners, and `BoardAction`.
- Treat `BoardListResponse` like `NewsListResponse`: server-trusted and
  unbounded by item count.
- Other board request/response messages should use normal computed per-type
  limits.

**Client Scope And Tests:**

- Client views for the main board view: list mode, icon grid mode with
  small/large icon sizes, and create/edit forms.
- The main board view should not have a manual refresh button. Like news, it
  stays current through `BoardUpdated` broadcasts and targeted list reloads.
- The toolbar should match the news toolbar pattern, titled `Boards`, with the
  same create-button flow.
- List view row fields: icon, name, description. Do not show slug or creator in
  the list row.
- Disabled boards use a visibly muted state.
- Icon grid mode has small `64px` and large `128px` options, with the board name
  under the icon. Disabled boards are visibly muted here too.
- Create/edit forms follow the existing Nexus create/edit form pattern and
  include:
  - name
  - slug
  - description Markdown editor
  - icon picker
  - banner picker
  - enabled checkbox
  - save/cancel
- Board descriptions are Markdown. Decide exact list/grid rendering behavior
  during UI implementation; prefer consistent Markdown handling unless it causes
  concrete UI problems.
- Right-clicking a board opens a context menu for board actions:
  - Share
  - separator
  - Edit
  - separator
  - Delete, styled as destructive/red
- The board context-menu Share action generates the protocol-correct future
  board URL `nexus://host/boards/{slug}` now, even though opening that
  slug/detail route is not part of the initial work.
- Board delete uses a confirmation screen, matching other destructive delete
  actions.
- Board show/detail UI is a separate design discussion and should not be
  bundled into the main board view scope.
- Tests for folded uniqueness, slug validation/share URL generation, visibility
  filtering, partial update clearing, image limits, creator permissions, and
  filtered broadcasts. Add slug route resolution tests later when board
  slug/detail URI handling is implemented.

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

### Protocol Consistency Cleanup

Protocol 0.9.0 is the right place for intentional consistency-breaking
cleanup. Keep these out of 0.8.x unless the protocol is deliberately bumped.

**Protocol message audit:**

- Final pass over every protocol message shape for field naming,
  required/optional behavior, response shape, and consistency with related
  messages.
- Confirm remaining update-style messages either already follow the 0.9.0
  partial-update convention or have a documented reason not to.
- Standardize clearing semantics for optional string/image fields where the
  field supports clearing: empty string clears, non-empty string replaces,
  omitted field leaves unchanged.
- Avoid `null`-as-clear semantics in new protocol shapes.
- Audit create/update form trimming behavior and decide where protocol/server
  semantics should trim, preserve exact input, or reject surrounding whitespace.

**Feature negotiation and unsolicited-message gating:**

- Keep direct command responses tied to the command; feature gating is for
  unsolicited broadcasts/events.
- Apply feature gating to new unsolicited broadcasts/events as features are
  added. Boards events must not be sent until a session has negotiated the
  `boards` feature.

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
