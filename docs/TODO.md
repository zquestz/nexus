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
| 10 | Streaming hash transfers | Medium | Planned |

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

### Streaming Hash Transfers

Eliminate upfront full-file SHA-256 computation that blocks transfer startup. Currently, downloads hash the full file on the server before sending `FileStart`, and uploads hash the full file on the client before sending `FileStart`. For a 6GB file, this means ~12 seconds of blocking I/O before any data flows.

**Core idea:** Both sender and receiver hash while streaming (integrated hashers), not before or after. The sender hashes while reading from disk and sending to network. The receiver hashes while receiving from network and writing to disk. A new `FileHash` message carries the sender's per-file hash after the data, replacing the upfront hash in `FileStart`. The receiver compares its independently computed hash against `FileHash` — no post-transfer re-read needed.

#### Protocol Changes

| Message | Change |
|---------|--------|
| `FileStart` | Remove `sha256` field (hash moves to `FileHash` message) |
| `FileHash` | **New** shared message: `{ sha256: String }` |
| `PROTOCOL_VERSION` | Bump (breaking change) |

`FileStartResponse`, `TransferComplete`, `FileData` — no changes.

#### Per-File Message Flow

After the `FileStart`/`FileStartResponse` exchange, the sender always sends one of:
- `FileData` then `FileHash` — data was transferred, here's the hash
- `FileHash` alone — file was already complete or zero-byte (no `FileData`)

The receiver reads the next frame and dispatches:
- `FileHash` → already complete or zero-byte, verify, done with this file
- `FileData` → stream to disk, then read `FileHash`, verify completed file

`FileHashing` keepalives may arrive before either and must be skipped as usual.

#### Single-Pass Hashing (Clone-and-Finalize)

SHA-256 hashers are incremental and cloneable. Both sides maintain a `StreamingHasher`.

**Sender** (for resume at offset N):
1. Create hasher, read 0..N into hasher (send `FileHashing` keepalives)
2. `hasher.clone().finalize()` → partial hash for resume verification
3. Read N..end into hasher + send over network
4. `hasher.finalize()` → full file hash → send `FileHash`

**Receiver** (for resume at offset N):
1. Create hasher, read existing .part 0..N into hasher (same read used for `FileStartResponse`)
2. `hasher.clone().finalize()` → partial hash for `FileStartResponse`
3. Receive N..end from network → hasher + write to disk
4. `hasher.finalize()` → full file hash → compare against `FileHash`

One read per byte per side. One hasher per side. Zero re-reads.

#### Scenarios — Downloads (server = sender)

**Fresh (offset=0, common case):**
1. Server sends `FileStart { path, size }`
2. Client has no local file → `FileStartResponse { size: 0, sha256: None }`
3. Server: offset=0, create hasher. Client: create hasher.
4. Server: read 0..end → hasher + `FileData`. Client: receive `FileData` → hasher + write to .part.
5. Server: `hasher.finalize()` → `FileHash { sha256: full_hash }`
6. Client: `hasher.finalize()` → client_hash. Read `FileHash`, compare with client_hash.

**Resume (offset=N):**
1. Server sends `FileStart { path, size }`
2. Client has N-byte .part → create hasher, read 0..N into hasher
3. `hasher.clone().finalize()` → `FileStartResponse { size: N, sha256: partial }`
4. Server: create hasher, read 0..N into hasher (keepalives)
5. `hasher.clone().finalize()` → verify partial hash → match
6. Server: read N..end → hasher + `FileData`. Client: receive `FileData` → hasher + append to .part.
7. Server: `hasher.finalize()` → `FileHash { sha256: full_hash }`
8. Client: `hasher.finalize()` → client_hash. Read `FileHash`, compare with client_hash.

**Already complete (offset=file_size):**
1. Server sends `FileStart { path, size }`
2. Client has full file → create hasher, read 0..end into hasher
3. `hasher.clone().finalize()` → `FileStartResponse { size: file_size, sha256: full }`
4. Server: create hasher, read 0..end into hasher (keepalives)
5. `hasher.clone().finalize()` → verify full hash → match
6. Server: `hasher.finalize()` → `FileHash { sha256: full_hash }` (no `FileData`)
7. Client receives `FileHash`, done (client already has verified file)

**Zero-byte:**
1. Server sends `FileStart { path, size: 0 }`
2. Client → `FileStartResponse { size: 0, sha256: None }`
3. Server sends `FileHash { sha256: empty_hash }`
4. Client creates empty file

#### Scenarios — Uploads (client = sender)

**Fresh (offset=0, common case):**
1. Client sends `FileStart { path, size }`
2. Server has nothing → `FileStartResponse { size: 0, sha256: None }`
3. Client: offset=0, create hasher. Server: create hasher.
4. Client: read 0..end → hasher + `FileData`. Server: receive `FileData` → hasher + write to .part.
5. Client: `hasher.finalize()` → `FileHash { sha256: full_hash }`
6. Server: `hasher.finalize()` → server_hash. Read `FileHash`, compare with server_hash.

**Resume (offset=N):**
1. Client sends `FileStart { path, size }`
2. Server has N-byte .part → create hasher, read 0..N into hasher (keepalives)
3. `hasher.clone().finalize()` → `FileStartResponse { size: N, sha256: partial }`
4. Client: create hasher, read 0..N into hasher (keepalives)
5. `hasher.clone().finalize()` → verify partial → match
6. Client: read N..end → hasher + `FileData`. Server: receive `FileData` → hasher + append to .part.
7. Client: `hasher.finalize()` → `FileHash`. Server: `hasher.finalize()` → server_hash.
8. Server: read `FileHash`, compare with server_hash.

**Already complete:**
1. Client sends `FileStart { path, size }`
2. Server has full file → create hasher, read 0..end into hasher (keepalives)
3. `hasher.clone().finalize()` → `FileStartResponse { size: file_size, sha256: full }`
4. Client: create hasher, read 0..end into hasher (keepalives)
5. `hasher.clone().finalize()` → verify → match
6. Client sends `FileHash` (no `FileData`). Server reads `FileHash` → already complete.

**Zero-byte:**
1. Client sends `FileStart { path, size: 0 }`
2. Server → `FileStartResponse { size: 0, sha256: None }`
3. Client sends `FileHash { sha256: empty_hash }`
4. Server creates empty file

#### Independent Verification (Both Sides)

Both sender and receiver independently compute the full file hash via their own `StreamingHasher`. Neither side trusts the other's hash — they compare independently computed results.

**Server upload verification:** The server maintains its own `StreamingHasher`:
- Pre-existing .part (0..N): fed into hasher during `FileStartResponse` computation
- Received `FileData` (N..end): fed into hasher as chunks are written to disk
- After all data: `hasher.finalize()` → server-computed hash
- Compare against client's `FileHash.sha256` → mismatch = reject, delete .part
- This catches disk corruption during write and malicious clients sending bad data.

**Client download verification:** The client maintains its own `StreamingHasher`:
- Pre-existing .part (0..N): fed into hasher during `FileStartResponse` computation (same read)
- Received `FileData` (N..end): fed into hasher as chunks are written to .part
- After all data: `hasher.finalize()` → client-computed hash
- Compare against server's `FileHash.sha256` → mismatch = reject, delete .part
- This replaces the current post-download re-read of the completed file, eliminating a full file read.

#### I/O Comparison

| Scenario | Current (total both sides) | Proposed (total both sides) |
|----------|---------------------------|----------------------------|
| Fresh download 6GB | Server: 6+6=12GB, Client: 6+6=12GB → **24GB** | Server: 6GB, Client: 6GB → **12GB** |
| Resume download 1.5/6GB | Server: 6+1.5+4.5=12GB, Client: 1.5+4.5+6=12GB → **24GB** | Server: 6GB, Client: 6GB → **12GB** |
| Fresh upload 6GB | Client: 6+6=12GB, Server: 6GB → **18GB** | Client: 6GB, Server: 6GB → **12GB** |
| Resume upload 1.5/6GB | Client: 6+1.5+4.5=12GB, Server: 1.5+4.5=6GB → **18GB** | Client: 6GB, Server: 6GB → **12GB** |

Every scenario reads every byte exactly once per side. **2-4x total I/O reduction.**

#### Implementation Layers

**Layer 1 — `nexus-common`:**
- Remove `sha256` field from `FileStart`
- Add `FileHash { sha256: String }` to `ClientMessage` and `ServerMessage` (shared)
- Add `StreamingHasher` wrapper: `new()`, `update(&[u8])`, `partial_hash() -> String` (clone+finalize), `finalize() -> String`
- Frame limits for `FileHash`
- Debug impls, io.rs type mappings
- Protocol version bump

**Layer 2 — `nexus-server`:**
- Downloads: remove upfront `compute_file_sha256_with_keepalive`. Restructure `stream_file_with_hash` to use `StreamingHasher` — single pass (0..N hasher-only with keepalives, N..end hasher+network). Send `FileHash` after each file.
- Uploads: add `StreamingHasher` to receive path — feed existing .part content (0..N) + received `FileData` (N..end). Read `FileHash` from client after each file. Compare with server-computed hash. Reject and delete on mismatch.

**Layer 3 — `nexus-client`:**
- Downloads: new per-file dispatch (`FileHash` alone vs `FileData`+`FileHash`). Add receive-side `StreamingHasher` — feed existing .part content (0..N, same read as `FileStartResponse`) + received `FileData` (N..end) as chunks are written. Verify `hasher.finalize()` against `FileHash.sha256`. Remove post-download file re-read for hash verification.
- Uploads: remove upfront `compute_file_sha256_with_keepalive`. Use `StreamingHasher` — single pass (0..N hasher-only with keepalives for resume, 0/N..end hasher+network). Send `FileHash` after each file.

#### Implementation Notes

- Keepalives during hasher-only phase (0..N): time-based check between chunks, send `FileHashing` periodically. No spawn_blocking needed — SHA-256 with hardware acceleration is ~microseconds per 64KB chunk.
- Upload hash mismatch is a new failure mode: server deletes the .part file, sends error in `TransferComplete`. Needs i18n error message (`err-upload-hash-mismatch` already exists, verify it covers this case).
- Backward compatibility: breaking protocol change due to `FileStart.sha256` removal. Protocol version bump required.
- Existing `compute_*_sha256_with_keepalive` functions in `nexus-common/src/hash.rs` can be removed or refactored — both sender and receiver now use `StreamingHasher` inline. The blocking-task approach is no longer needed since SHA-256 with hardware acceleration adds negligible overhead per chunk.
- Client download: the `FileStartResponse` hash computation and post-download verification merge into a single `StreamingHasher` that spans both phases. The `compute_file_sha256` call after download (current line 540 of `executor/mod.rs`) is eliminated.