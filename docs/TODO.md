# Nexus TODO

## Implementation Order (Pre-Launch)

| #   | Feature                  | Effort | Status     |
| --- | ------------------------ | ------ | ---------- |
| 1   | Account groups           | Low    | ✅ Done    |
| 2   | Password strength        | Low    | ✅ Done    |
| 3   | Streaming hash transfers | Medium | ✅ Done    |
| 4   | Boards                   | High   | Planned    |
| 5   | File previews            | Low    | Planned    |
| 6   | Trackers                 | Medium | Spec ready |
| 7   | Speed limiting           | Medium | Planned    |
| 8   | Flood protection         | Low    | ✅ Done    |
| 9   | Server logs              | Medium | ✅ Done    |
| 10  | Auto-away                | Low    | ✅ Done    |
| 11  | Invite system            | Medium | Planned    |

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
(tracker protocol v0.1.0). Non-obvious design invariants are recorded
in `CLAUDE.md` under "Tracker (Pending Implementation)".

The reference implementation lands as a separate `nexus-tracker` crate
producing a `nexus-trackerd` binary.

#### Implementation Plan

**Phase 1: `nexus-common` prep commit (do first)**

Single commit to make the workspace ready for tracker code, with no
tracker behavior yet.

| File                        | Change                                                                                                                                                                                                          |
| --------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `error_kind.rs`             | Add `Unauthorized`, `RateLimited`, `Capacity` variants + matching string constants. Update `as_str()`, `parse()`, all 4 test functions.                                                                         |
| `lib.rs`                    | Add `TRACKER_PROTOCOL_VERSION = "0.1.0"` constant alongside `PROTOCOL_VERSION`.                                                                                                                                 |
| `tracker_protocol.rs` (new) | `TrackerClientMessage` (`TrackerRegister`, `TrackerList`) and `TrackerServerMessage` (`TrackerRegisterResponse`, `TrackerListResponse`, `Error`) enums with full message structures matching the protocol spec. |
| `framing/limits.rs`         | Per-message-type payload limits for the 4 typed tracker messages. `TrackerListResponse` set to 0 (unlimited) per spec; others sized via the existing helper functions.                                          |

**Phase 2: `nexus-tracker` crate (`cargo new nexus-tracker --bin`)**

Workspace structure:

```
nexus-tracker/
├── Cargo.toml
├── src/
│   ├── main.rs          # Entry point, signal handling, listener setup
│   ├── args.rs          # CLI parsing (clap)
│   ├── connection.rs    # Per-connection task, role locking
│   ├── handlers/        # TrackerRegister, TrackerList, Error builders
│   ├── registry.rs      # In-memory entry storage (HashMap by connection id)
│   ├── tls.rs           # Cert auto-gen / load (model on nexus-server)
│   ├── auth.rs          # Argon2id password hash load + verify, SIGHUP reload
│   ├── i18n.rs          # Fluent loader (model on nexus-server)
│   ├── rate_limiter.rs  # Per-IP token-bucket rate limits
│   ├── constants.rs     # Filenames, defaults
│   └── errors.rs        # Translated error helpers (err_*)
├── locales/             # 13 language directories
└── tests/               # Integration tests
```

Binary: `nexus-trackerd`.

**CLI surface:**

```
nexus-trackerd [OPTIONS]                 # Run daemon (default)
nexus-trackerd set-password <KIND>       # KIND: registration | listing
nexus-trackerd clear-password <KIND>

DAEMON OPTIONS:
  -b, --bind <ADDR>             [default: 0.0.0.0]
  -p, --port <PORT>             [default: 7510]
      --data-dir <DATA_DIR>     [default: platform-specific via dirs::data_dir()]
      --log-level <LEVEL>       [default: info]
      --log-retention <DUR>     [default: 30d]
      --no-log-timestamps
      --upnp
      --websocket
      --websocket-port <PORT>   [default: 7511]
      --max-entries <N>         range 0..=1_000_000, default 10_000, 0 = unlimited
      --refresh-interval <SECS> range 120..=600, default 300
```

Platform-specific data-dir defaults (via `dirs::data_dir().join("nexus-trackerd")`):

- Linux: `~/.local/share/nexus-trackerd/`
- macOS: `~/Library/Application Support/nexus-trackerd/`
- Windows: `%APPDATA%\nexus-trackerd\`

**Data layout (`<data-dir>/`):**

| File                         | Mode | Purpose                                       |
| ---------------------------- | ---- | --------------------------------------------- |
| `tracker.crt`                | 0644 | TLS certificate (auto-generated on first run) |
| `tracker.key`                | 0600 | TLS private key (auto-generated on first run) |
| `registration.password.hash` | 0600 | Argon2id PHC string (absent = open)           |
| `listing.password.hash`      | 0600 | Argon2id PHC string (absent = open)           |

**Password management:**

- `set-password <kind>` reads the password from stdin (TTY → `rpassword` prompt with no echo; pipe → first line of stdin). Validates length per nexus-server's `MAX_PASSWORD_LENGTH = 256` bytes. Hashes with Argon2id (matching nexus-server's parameters). Atomic-writes the PHC string to the hash file (`<file>.tmp` then rename) with mode 0600.
- `clear-password <kind>` removes the hash file. Idempotent — succeeds even if the file didn't exist.
- **SIGHUP reload (Unix only):** daemon catches `SIGHUP`, re-reads both hash files, atomically swaps the in-memory cache. Existing connections continue uninterrupted; password changes take effect on the next refresh / new connection. **Malformed hash file on reload preserves the previous state and logs a loud error** — don't crash a running daemon over a typo.
- **Windows:** restart-required. Documented platform difference. Future work could add a `notify`-crate file watcher for cross-platform parity.
- **No PID file.** Operators rely on systemd / supervisord / `pidof` for PID tracking.

**Validations (clap value parsers):**

| Flag                 | Range           | Notes                                                                                                                                        |
| -------------------- | --------------- | -------------------------------------------------------------------------------------------------------------------------------------------- |
| `--max-entries`      | `0..=1_000_000` | 0 = unlimited; default 10,000 catches typos and prevents unintentional unbounded growth                                                      |
| `--refresh-interval` | `120..=600`     | Min: matches the protocol's 120s server-side floor. Max: matches the spec's "exceeding ~600 seconds risks NAT-induced disconnects" guidance. |

**Locales:**

- `nexus-tracker/locales/<lang>/errors.ftl`, fully independent from `nexus-server/locales/`. No shared keys.
- `err-tracker-*` prefix for all keys.
- ~20 keys for v1 (auth, field validation, rate / capacity, protocol-level).
- **English ships first.** Other 12 locales are populated before tracker v0.1.0 actually releases. Loader falls back to English for missing keys (matching nexus-server's pattern).

**Open vs. gated:**

- A tracker with neither password set is **open** (registration and listing both unrestricted). Open trackers are valid by design.
- Daemon logs the open/gated status loudly at startup: `Registration: open, Listing: gated` (etc.).
- If a peer sends a password to an open flow, it's silently ignored.

**Testing strategy (within Phase 2):**

- Unit tests in `nexus-tracker/src/**` (validators, registry, rate limiter, auth).
- Integration tests in `nexus-tracker/tests/` exercising real TCP + TLS + framed JSON against a daemon instance.

**Build / CI (within Phase 2):**

- Add `nexus-tracker` to workspace `Cargo.toml` members.
- Add to `ci.yml` and `release.yml` (test, build, clippy, fmt for the new crate).

**Out of scope for Phase 1 + Phase 2:**

Server-side publisher integration, client-side browser integration, user
docs, packaging (Docker images, etc.) are all separate scoping
conversations to be had later. Phase 1 + Phase 2 produces a fully
spec-compliant `nexus-trackerd` daemon and nothing else.
