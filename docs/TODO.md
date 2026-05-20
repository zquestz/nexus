# Nexus TODO

## Implementation Order (Pre-Launch)

| #   | Feature                     | Effort | Status                      |
| --- | --------------------------- | ------ | --------------------------- |
| 1   | Account groups              | Low    | ✅ Done                     |
| 2   | Password strength           | Low    | ✅ Done                     |
| 3   | Streaming hash transfers    | Medium | ✅ Done                     |
| 4   | Boards                      | High   | Planned                     |
| 5   | File previews               | Low    | Planned                     |
| 6   | Tracker registration        | Medium | ✅ Done                     |
| 7   | Tracker discovery           | Low    | ✅ Done                     |
| 8   | Speed limiting              | Medium | Phase 1 ✅, Phase 2 planned |
| 9   | Flood protection            | Low    | ✅ Done                     |
| 10  | Server logs                 | Medium | ✅ Done                     |
| 11  | Auto-away                   | Low    | ✅ Done                     |
| 12  | Certificate fingerprint pin | Low    | ✅ Done                     |

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

### Speed Limiting

Per-user weighted fair share of server outbound bandwidth, enforced via a single egress scheduler.

**Algorithm: WF2Q+ (Bennett & Zhang, 1997)**

- Variable-size packet fair scheduler. O(log N) per dequeue, bounded delay vs GPS.
- Each user is one "flow." Flow weight controls the user's share of the cap when contending; weight 10 vs weight 1 means 10× the share. Work-conserving — single active flow gets the full cap.
- Paper: Bennett & Zhang, "Hierarchical Packet Fair Queueing Algorithms" (ICNP 1997) — <https://www.cs.cmu.edu/~hzhang/papers/ICNP97.pdf>.
- Reference C implementation: <https://github.com/zquestz/throttled/blob/master/src/wf2q%2B.h>.
- Defer to those for the virtual-time update rule and eligibility check — they're more precise than any in-spec prose.

**Architecture: single central scheduler with per-connection writer tasks**

- One scheduler instance for the entire server, running as a dedicated tokio task that wakes when a flow has data and rate budget is available.
- **The scheduler task does zero I/O.** It is a pure dispatch engine: it maintains WF2Q+ state, rate-limits, and dispatches packets to per-connection bounded channels. It never calls `write_all` on a socket.
- **Per-connection writer tasks.** Each WAN connection gets a dedicated writer task (`tokio::spawn`) that owns the `WriteHalf` and drains its bounded channel. Writer tasks write to their own socket independently — a slow client's socket blocking its writer task cannot stall any other connection or the scheduler itself. This eliminates head-of-line blocking.
- The scheduler uses `try_send()` when dispatching to a connection's channel. If the channel is full (slow client), the packet stays in the WF2Q+ queue and the **entire flow is marked blocked** — the scheduler moves on to the next eligible flow. No blocking, no waiting. Skipping the whole flow (not just the one connection) is intentional: a multi-session user's allocation is one flow, and a slow connection within it is the user's problem. This keeps the algorithm simple (flow-level blocking, not connection-level) and consistent with per-user fairness.
- **Drain notifications (wakeup path).** When a writer task drains a packet and the channel transitions from full to available, it sends a lightweight `Drained(ConnectionId)` notification to the scheduler (via a dedicated mpsc). Notifications are only sent on the full→available edge, not after every write, to minimize noise. The scheduler's main loop `select!`s over: rate budget timer, new submissions, and drain notifications. On `Drained`, the scheduler unblocks the affected flow and resumes dispatch. This avoids both busy-spinning (polling full channels repeatedly) and oversleeping (missing that a channel has capacity).
- LAN connections bypass the scheduler entirely and write directly (see "Scope of cap" below).
- **Outbound only.** Inbound shaping is wasteful (TCP + kernel handle it).
- **`TCP_NODELAY` set on every accepted TCP socket.** Without it, Nagle's algorithm coalesces small writes downstream of the application — defeating WF2Q+'s latency bounds for WAN connections and hurting chat latency on LAN too. Set at accept time regardless of LAN/WAN classification. WebSocket sockets inherit from the underlying TCP socket. UDP voice is unaffected (different protocol).
- **WebSocket adapter integration.** The WebSocket adapter (`nexus-server/src/websocket.rs`) exposes `AsyncRead + AsyncWrite` over a WS message stream. The writer task holds its `WriteHalf` and writes raw bytes; the adapter wraps each write in a binary WS frame. Scheduler-level chunking continues to work; WS frame boundaries are an internal layer transparent to the scheduler.

**Scope of cap**

- Single combined cap across BBS port (7500/7502) + transfer port (7501/7503).
- **Voice UDP exempt** — real-time, would degrade quality. UDP packets go directly to socket, not through scheduler.
- **LAN bypass** via `nexus_common::address::is_private_network` (RFC 1918 + IPv6 ULA + loopback). **NOT** Yggdrasil — that mesh routes over the internet, counts as WAN bandwidth. Implementation: LAN connections keep their `WriteHalf` on the connection task and `send_*()` writes directly to the socket — they never touch the scheduler. This decouples LAN traffic from the scheduler's dispatch task, so a saturating LAN client can't starve WAN traffic. The cap=0 optimization is unrelated: at cap=0 the scheduler still dispatches WAN connections (skipping chunking + rate sleep), so live cap changes take effect immediately on existing WAN connections.

**Flow model**

- Flow = nickname (lowercased). For regular accounts nickname equals username, so all sessions of a regular user share one flow. For shared accounts each session has a unique nickname, so each session gets its own flow — fair share works per real person rather than per shared login.
- Pre-login traffic (WAN only): one **anonymous flow per source IP**, weight = 1. Reaped via refcount when the last pre-login connection from that IP disconnects or graduates to a nickname flow. LAN connections bypass the scheduler from accept onward and never enter the anon flow.
- **Trust does NOT bypass cap.** Speed and ban are orthogonal concerns.
- **Admins skip group lookup.** Admins don't belong to groups; if they have no explicit per-user weight, they resolve to `DEFAULT_ADMIN_BANDWIDTH_WEIGHT` (50). They participate in fair scheduling like everyone else — the elevated default just reflects that admins typically warrant a larger share than guests.

**Weight resolution (mirrors permission resolution)**

```
bandwidth_weight =
    user.bandwidth_weight                                     // Some(w) → explicit user override
        .or_else(|| if user.is_admin {                        // admins skip group lookup
            Some(DEFAULT_ADMIN_BANDWIDTH_WEIGHT)              //   → 50
        } else {
            None
        })
        .or(user.group_id
            .and_then(group_lookup)
            .map(|g| g.bandwidth_weight))                     // group's weight
        .unwrap_or(DEFAULT_BANDWIDTH_WEIGHT)                  // → 1 (system default)
```

Cached on `UserSession.bandwidth_weight` at login as an `AtomicU16`. The handler layer reads this for delegation checks (non-admin can't set weight above their own). No "VIP / uncap" mechanism — for an effectively-uncapped user, set their weight very high (e.g., `1000`) so they dominate.

**Bounds:** weight is `INTEGER` with allowed range `1..=65535`. Validate at the protocol boundary; reject out-of-range with the standard validation error path. The scheduler holds it as `u16` internally.

**Scheduler-owned flow weights.** The scheduler maintains flow weights as plain `u16` values (not by reading `UserSession.bandwidth_weight` directly — `AtomicU16` clones snapshot, so the scheduler cannot hold a live reference). Weight updates arrive as explicit commands through the `SchedulerHandle`:

- `update_flow_weight(FlowId, u16)` — updates a single specific flow's weight. Useful for narrow/targeted updates where the caller already knows the exact flow.
- `update_account_flow_weights(user_id, weight)` — fans out to **all** nickname flows whose `user_id` metadata matches, setting each flow's weight independently. This is the primary API for user/group weight changes. It covers transfer-only connections (not in `UserManager`'s session map) and shared accounts (multiple nickname flows, same `user_id`). Each flow remains a separate scheduling identity — this updates weights, not merges queues.
- `rename_flow(old_nickname_lower, new_nickname_lower)` — called from the `UserUpdate` handler when a **regular** account's username changes (where nickname == username). Atomically renames the flow key; all connections in the old flow move to the new flow. The handler has both old and new names. **Not used for shared accounts** — shared account username changes don't affect active session nicknames (each session keeps its login-chosen nickname).
- `transition_to_nickname(conn_id, nickname_lower, user_id, weight)` — carries the initial weight and user identity at login. The `user_id` is stored as metadata on the flow (see below).

Each `FlowState` carries a `user_id: Option<i64>` set by `transition_to_nickname`. Anonymous flows have `None`. The flow key is always `FlowId::Nickname(nickname_lower)` — `user_id` is metadata only, never a scheduling identity. This metadata enables `update_account_flow_weights` to find all flows for a given account without requiring the handler to enumerate nicknames from `TransferRegistry`. For regular accounts all sessions share one nickname (and thus one flow). For shared accounts each session has a unique nickname but the same `user_id`, so `update_account_flow_weights` catches all of them while keeping their queues separate.

**Cache invalidation.** Two updates fire when a user's effective weight changes:

- **Server-side (session cache)**: refresh `UserSession.bandwidth_weight` (the `AtomicU16`) for every active session of the affected user via `UserManager::update_bandwidth_state`. For `UserUpdate` changing `bandwidth_weight` or `group_id`, that's the target user's sessions. For `GroupUpdate` changing the group's weight, fan out via `get_sessions_by_group_id` to every member's session (same pattern as group permission cascades).
- **Server-side (scheduler)**: the same handler code that refreshes the session cache also calls `scheduler.update_account_flow_weights(user_id, resolved_weight)`. This fans out to all nickname flows for the account (including transfer-only flows). For group cascades, call it once per affected `user_id`. For regular-account username renames, call `scheduler.rename_flow(old, new)` first (the weight is preserved across the rename, so a separate weight update is only needed if the weight also changed).
- **Client-visible**: the resolved weight is in `UserInfo`, so it naturally rides on existing `UserUpdated` broadcasts (which fire for renames, group changes, etc.). Weight-only changes do not require a new broadcast trigger — the value is "best-effort current at last broadcast" the same way `group_name` is today. Clients render the latest broadcast value; brief staleness on weight-only edits is acceptable since the client UI doesn't render weight for non-admins anyway.

**Intra-user priority (two sub-queues per user)**

When a user has both BBS-port (protocol) and transfer-port (bulk) connections sending data, the user's flow uses strict priority internally:

- `Protocol` queue (BBS port + WS-BBS) — drained first. **Starves Bulk until empty.**
- `Bulk` queue (transfer port + WS-transfer) — drained only when protocol queue is empty.

Connection class is set at accept time based on which listener accepted the connection. Worst-case chat delay during a sustained transfer = one chunk transmission time (`chunk_size / cap_rate`).

WF2Q+ virtual-time accounting is unaffected — a packet is a packet regardless of sub-queue. Per-user fairness vs other users is preserved.

**Why Protocol starves Bulk — user behavior on slow connections.** Consider a user on a 0.5 Mbps link downloading a large file. They click a directory: the `FileListResponse` must arrive fast because they're actively interacting. If Protocol competes with Bulk for the same pipe, a 1 MB file listing takes ~32 seconds instead of ~16. With starvation, Protocol gets the full pipe — the listing arrives as fast as physically possible, the transfer pauses, and then resumes once the user stops clicking. The same logic applies to news listings, chat messages, user lists — anything interactive. Most Protocol messages are small and go out so fast that the starvation is invisible. When they're large (e.g., a huge directory listing), starvation is even more valuable because the user would notice the delay.

**Backpressure model.**

- **Protocol `send_frame`: blocks only on its own backlog, never on Bulk.** Each flow has a **`PER_FLOW_PROTOCOL_CAP` (4 MB)** — the blocking rule is: if pending Protocol bytes are **already ≥ cap**, `send_frame` blocks until drain brings them below; if pending bytes are **below cap**, the enqueue always succeeds (even if the message itself pushes the total over). This means exactly one message can overshoot the cap, but the next caller blocks until the queue drains. A single 5 MB `FileListResponse` gets through (we were under cap when it arrived), but the next `send_frame` blocks — no unbounded bypass. The 4 MB cap is extremely generous: a flow is one user (regular accounts) or one session (shared accounts), so Protocol traffic comes from at most one active BBS connection plus broadcasts. Most sessions are idle and only receive broadcasts; the active one generates responses. A user would need to fall 4 MB behind on their own Protocol output to trigger blocking — realistically only a frozen client. When blocking does trigger, it pauses exactly the right thing: the connection task's `select!` rx arm (for broadcasts) or the handler (for direct responses). The broadcaster (`UserManager.broadcast()`) is never blocked — it fire-and-forgets via the unbounded per-session `tx`, so backpressure stays local to the slow connection's task.
- **Bulk `send_bytes`: blocks when per-user pending bulk bytes exceed `PER_USER_BULK_BUFFER_CAP` (1 MB).** Backpressure propagates naturally to callers: a transfer task pumping 64 KB chunks blocks when the user's bulk queue is full, the file read pauses. Per-user (not per-connection) so a user opening many transfer connections doesn't get a larger memory budget than a single-connection user, consistent with the per-user fairness model.
- **Bulk can never block Protocol.** The two caps are independent. A saturating transfer cannot prevent a chat message or file listing from being enqueued and dispatched. Protocol always starves Bulk at dispatch time, and the Protocol cap is separate from the Bulk cap.

**Chunking (scheduler-internal, protocol-transparent)**

The scheduler chunks every enqueued payload into `scheduler_chunk_size`-byte WF2Q+ packets. Chunking is invisible above the scheduler — TCP is a byte stream, the client reassembles frames as usual.

Worst-case latency for a small message arriving behind one in-flight chunk = `chunk_size / cap_rate`.

**Optimization: when `max_outbound_rate = 0` (unlimited), chunking is skipped.** The whole reason to chunk is bounding the `chunk_size / cap_rate` latency for small messages competing with bulk transfers — at cap=0, the kernel drains the socket at line rate (microseconds per packet), so the bound isn't needed. Each enqueued payload becomes one WF2Q+ packet of whatever size the caller submitted. The `scheduler_chunk_size` knob only applies when cap > 0. This reduces scheduler ops by ~8× at 100 MB/s outbound (one op per 64 KB transfer chunk instead of one op per 8 KB sub-chunk); WAN egress still flows through the scheduler so live cap changes take effect immediately.

**Operator config (two knobs)**

| Setting                | Unit         | Default         | Bounds        | Storage              |
| ---------------------- | ------------ | --------------- | ------------- | -------------------- |
| `max_outbound_rate`    | Mbps (float) | `0` (unlimited) | `≥ 0`, finite | bytes/sec internally |
| `scheduler_chunk_size` | bytes        | `8192` (8 KB)   | 1 KB – 64 KB  | bytes                |

Mbps chosen to match how ISPs print plans (1 Gbps → `1000`, 100 Mbps → `100`, slow link → `0.5`). Live changes picked up on the next tick.

**Connection writer API**

Callers (protocol handlers, transfer code) don't talk to the scheduler directly. Each connection task holds a `ConnectionWriter` — an enum that internally routes to the scheduler (WAN) or writes directly to its own `WriteHalf` (LAN). Same API both ways:

```rust
use bytes::Bytes;

/// Per-connection egress handle. Routes to scheduler for WAN, direct write for LAN.
/// The `Direct` variant uses type erasure (`Box<dyn AsyncWrite>`) which eliminates
/// the `<W>` generic parameter from `HandlerContext`, `Transfer`, and all handler
/// functions — a major simplification of the server codebase.
pub enum ConnectionWriter {
    Scheduled { handle: SchedulerHandle, conn_id: ConnectionId },
    Direct { writer: Box<dyn AsyncWrite + Unpin + Send> },
}

impl ConnectionWriter {
    /// Send a BBS-protocol frame. Serializes the message to bytes internally
    /// (one allocation, same as today's FrameWriter). Blocks only when
    /// the flow's Protocol backlog exceeds PER_FLOW_PROTOCOL_CAP (4 MB);
    /// never blocked by Bulk backpressure.
    pub async fn send_frame(&mut self, msg: ServerMessage, msg_id: MessageId) -> Result<(), SendError>;

    /// Send raw bytes (transfer-port path). Takes ownership via `Bytes` so the
    /// scheduled path can enqueue without copying; the direct path forwards
    /// `bytes.as_ref()` to `write_all`. Blocks when per-user bulk buffer cap
    /// is exceeded (Scheduled path only; Direct writes block on kernel backpressure).
    pub async fn send_bytes(&mut self, bytes: Bytes) -> Result<(), SendError>;

    /// Flush pending writes. No-op for Scheduled (scheduler handles dispatch
    /// timing). For Direct, calls the underlying writer's flush.
    pub async fn flush(&mut self) -> Result<(), SendError>;

    /// Shut down the writer. For Scheduled, unregisters from the scheduler
    /// (which flushes remaining packets before closing). For Direct, calls
    /// the underlying writer's shutdown.
    pub async fn shutdown(&mut self) -> Result<(), SendError>;
}

pub enum SendError {
    ConnectionGone,  // socket closed, writer dropped, peer disconnect
}
```

Routing is a single enum branch — no heap allocation per send, no byte-level copy. The scheduler's internal API (`SchedulerHandle::send_*`) mirrors the same signatures.

**Zero-copy design.** The scheduler adds no byte-level copies beyond what the existing code already does:

| Step                   | Today       | With scheduler                    |
| ---------------------- | ----------- | --------------------------------- |
| Frame serialize        | 1 alloc     | 1 alloc                           |
| File read into buffer  | 1 alloc     | 1 alloc                           |
| Hand off to dispatcher | n/a         | move (no copy)                    |
| Chunk for WF2Q+        | n/a         | `Bytes::slice()` on Arc (no copy) |
| Socket write           | kernel copy | kernel copy                       |

Same total memory traffic as today. The scheduler adds bookkeeping (pointer/length, refcount) but no copies. `bytes` is already a transitive dependency in `Cargo.lock`; add it as a direct dependency on `nexus-server`.

Per-user backpressure and the scheduler-registry semantics described next apply to scheduled (WAN) connections only. LAN connections use the kernel's natural `write_all` backpressure and surface socket errors directly to the connection task — they have no scheduler queue and no per-user cap because they don't share the rate budget. See the "Backpressure model" section above for the full Protocol-never-blocked / Bulk-capped design.

**Writer errors and death detection.** Each WAN connection's writer task owns its `WriteHalf` and drains a bounded channel from the scheduler. When a writer task's `write_all` fails (socket error, peer disconnect), it exits, dropping its channel receiver. The connection task detects writer death via the writer task's `JoinHandle` in its `select!` loop — when the handle completes, the connection task breaks its read loop and cleans up the session. Future `send_*` calls on the `ConnectionWriter` after the writer has exited return `Err(ConnectionGone)`. The scheduler detects the dead channel on its next `try_send()` and removes the connection from its registry.

**Lifecycle**

The scheduler maintains two internal registries:

- `connections: HashMap<ConnectionId, ConnectionEntry>` — per-connection state. `ConnectionEntry` holds the bounded channel sender (to the writer task), `ConnectionClass`, and the current `FlowId` (an enum: `FlowId::Anon(IpAddr)` or `FlowId::Nickname(String)`). The `WriteHalf` itself is owned by the per-connection writer task, not stored here. The current flow is how `send_*(conn_id, …)` resolves which flow's queue to enqueue into.
- `flows: HashMap<FlowId, FlowState>` — per-flow state (queue, virtual time, weight ref). Plus a separate `anon_refcount: HashMap<IpAddr, usize>` for anon-flow GC.

- **Connection register**: registration happens **after** TLS handshake (and optional WebSocket handshake) and `tokio::io::split()` — not at raw TCP accept. Pre-handshake error messages (TLS/WS failures) are exempt from scheduling (tiny, rare, pre-split). At registration time, check `is_private_network(peer_ip)`. LAN connections keep their `WriteHalf` on the connection task wrapped in `ConnectionWriter::Direct` and bypass the scheduler entirely. WAN connections hand their `WriteHalf` to the scheduler along with the `ConnectionClass` (Protocol for BBS/WS-BBS ports, Bulk for transfer/WS-transfer ports — determined by which listener accepted). The scheduler creates a bounded dispatch channel, stores the channel sender in a `ConnectionEntry` with `FlowId::Anon(peer_ip)`, and returns a `ConnectionId` plus the channel receiver. The connection task spawns a **writer task** (`tokio::spawn`) with the `WriteHalf` and channel receiver; the writer task owns the socket and drains the channel. The connection task holds the `ConnectionId` in a `ConnectionWriter::Scheduled`.
- **Pre-login (WAN only)**: traffic flows into the anon flow at `FlowId::Anon(peer_ip)`, weight = 1. Anon-flow refcount incremented. LAN connections bypass the scheduler from accept onward and never enter the anon flow; `ConnectionClass` is meaningful only for scheduled (WAN) connections.
- **Login transition (BBS port)**: after auth succeeds but **before sending `LoginResponse`**, the login handler calls `scheduler.transition_to_nickname(conn_id, nickname_lower, user_id, weight)`. The scheduler atomically:
  - Swaps the conn's `FlowId` to `FlowId::Nickname(nickname_lower)`.
  - Decrements the anon-flow refcount (reaping the anon flow if it hits zero).
  - If this is the first session for that nickname, creates the nickname flow with the provided `weight`. If the nickname flow already exists (multi-session login), the new connection joins it and inherits its current weight — sessions of the same user always resolve to the same weight, so this is consistent by construction.
  - Initializes the new flow's WF2Q+ virtual-time state at the current system V(t).

  The `LoginResponse` itself is then sent through the new nickname flow — so the response message is the first thing the user's flow dispatches at their proper weight. Protocol semantics guarantee the client sends nothing between `Login` and receiving `LoginResponse`, so this window is the safe transition point.

- **Login transition (transfer port)**: `transfers/auth.rs` resolves the bandwidth weight (via `get_resolved_bandwidth_weight`) and carries it alongside `AuthenticatedUser`. After transfer auth succeeds, the transfer handler calls `scheduler.transition_to_nickname(conn_id, nickname_lower, user_id, weight)` — same as the BBS path. The transfer connection joins the same nickname flow as the user's BBS connections (if any), sharing the WF2Q+ allocation.
- **Nickname rename**: when a `UserUpdate` changes a username (and for regular accounts, nickname == username), the handler calls `scheduler.rename_flow(old_nickname_lower, new_nickname_lower)`. The scheduler atomically renames the flow key; all connections (BBS and transfer) in the old flow move to the new flow. The handler has both old and new names from the update path. The `user_id` metadata on the flow is preserved across the rename.
- **Graceful unregister**: when unregistering a connection (disconnect, kick, ban), the scheduler flushes only the **Protocol sub-queue** for that connection into its bounded channel (via `try_send`; packets that don't fit are dropped). All pending **Bulk data is dropped** — there's no value in sending megabytes of stale transfer data before a disconnect, and attempting to do so could block shutdown if the channel is full or the writer is dead. After the Protocol flush (or if the channel is already dead), the scheduler drops the channel sender. The writer task drains remaining items, flushes the socket, and exits. This ensures `send_error_and_disconnect` messages are delivered (they're Protocol class and typically tiny) while not delaying shutdown for expendable Bulk backlog.
- **Connection disconnect**: after flush-on-unregister completes and the writer task exits, the scheduler removes the conn_id from `connections`. The flow stays alive if other conn_ids still reference it.
- **Last session disconnects**: when the last conn_id referencing a nickname flow disconnects, the nickname flow is reaped.
- **Anon flow refcount**: incremented on each pre-login WAN connection from an IP, decremented on either successful login (graduation) or disconnect. Reaped when it hits zero.
- **Writer task lifecycle**: the writer task runs a simple loop: `recv() → write_all() → (continue)`. On channel close (unregister path), it drains remaining items, flushes, and shuts down the socket. On write error, it exits immediately. The connection task detects writer death via the `JoinHandle` completing in its `select!` loop, breaks its read loop, and proceeds to session cleanup.

**Logging**

- `INFO`: cap changes (rate or chunk size — operator visibility), flow created/reaped (debugging fairness), scheduler-detected unrecoverable error.
- `DEBUG`: flow becomes backlogged > N seconds (slow-client signal), per-flow stats on demand. Off in production logs unless explicitly enabled.
- No `TRACE`-level per-dispatch logging (would flood logs at line rate).

**Testing**

- Unit tests for WF2Q+ math: virtual-time advance, eligibility checks, finish-time ordering for variable packet sizes. Reproducible without I/O.
- Integration tests for fairness scenarios use `tokio::time::pause()` and explicit `advance(...)`, not real-time waits. This deterministically tests "user A with weight 10 and user B with weight 1 should see 10:1 throughput ratio over N dispatches."
- Tests for lifecycle edge cases: connection registered while flow was already alive (multi-session login), connection disconnect while in-flight packet hadn't finished, anon→user transition while bytes were queued.

**Docs**

- `docs/server/02-configuration.md` — new "Bandwidth" subsection: what `max_outbound_rate` does, what `scheduler_chunk_size` does, when to tune each, worked examples (slow link vs gigabit, mostly-chat vs mostly-transfers).
- `docs/server/05-user-management.md` — explain `bandwidth_weight`, the user/group resolution rule, worked example ("guest group weight 1, regulars weight 10, in a contention of 1 guest + 1 regular the regular sees 10/11 of the cap").
- `docs/client/10-server-info.md` (or equivalent admin panel doc) — document the new Bandwidth section in the Server Info panel.
- English locale strings only during development; other locales after feature ships (per project convention).

**Future work (out of this PR pair)**

- Connection Monitor integration: surface per-user current outbound rate and backlog in the existing admin panel.
- Stats / metrics endpoint for external monitoring.
- Per-channel / per-file-area sub-budgets (if ever justified).

**Schema**

- `users` table: `ALTER TABLE users ADD COLUMN bandwidth_weight INTEGER NULL` — NULL = inherit from group (or system default if no group).
- `groups` table: `ALTER TABLE groups ADD COLUMN bandwidth_weight INTEGER NOT NULL DEFAULT 1`.
- `config` table (existing key-value store): new rows `max_outbound_rate` (default `'0'`, stored as bytes/sec) and `scheduler_chunk_size` (default `'8192'`). Values stored as `TEXT` per the existing convention; setter / getter in `db/config.rs` handle the int parse.

**Protocol additions**

- `UserCreate` / `UserUpdate` / `GroupUpdate` requests gain optional `bandwidth_weight` field. `UserCreate` / `UserUpdate` additionally carry `inherit_bandwidth_weight: Option<bool>` (when `Some(true)`, no per-user override is stored; the resolver falls back through admin-default → group → system default). `GroupCreate.bandwidth_weight` is non-optional `u16` with serde default = `DEFAULT_BANDWIDTH_WEIGHT`.
- `UserEditResponse` / `GroupEditResponse` return the field (the raw stored value, including `None` for "inherit from group" on users).
- `UserInfo` and `UserInfoDetailed` gain `bandwidth_weight: Option<u16>` (#[serde(default, skip_serializing_if = "Option::is_none")]) carrying the **resolved** effective weight (server always sends `Some(weight)`; old clients ignore the field). Visible to all users, consistent with the rest of the UserInfo fields. Rides naturally on `UserUpdated` broadcasts.
- `ServerInfo` and `ServerInfoUpdate` both gain `max_outbound_rate` and `scheduler_chunk_size`: `max_outbound_rate` visible to all (like `chat_rate_limit` / `max_connections_per_ip`); `scheduler_chunk_size` admin-only (pure internal tuning knob with no user-facing meaning). `ServerInfoUpdated` broadcast carries either when changed, with the same per-field visibility.

**UI**

- New **"Bandwidth"** section in the Server Info admin panel — present in **both** the Config display tab and the edit form.
- Section order in Server Info: General (special, first) → Bandwidth (B) → Chat → … (alphabetical after General).
- Fields: `Max outbound (Mbps)` (float, 0 = unlimited), `Scheduler chunk size (bytes)` (integer, default 8192).
- User create + user edit forms: directly **under the Group dropdown row**, add a `Bandwidth weight` number field plus an **always-visible** "Inherit Bandwidth Weight" checkbox.
  - The inherited baseline mirrors the server resolution rule: the selected group's weight when a group is set, `DEFAULT_ADMIN_BANDWIDTH_WEIGHT` (50) when the target is admin, otherwise `DEFAULT_BANDWIDTH_WEIGHT` (1).
  - Inherit checked → number field disabled, shows the baseline greyed; save sends "inherit" (null override).
  - Inherit unchecked → number field enabled, **bold when it differs from the baseline** (same visual rule as the permission overrides above it).
- Group create + group edit forms: between **Shared** and **Permissions**, add a single `Bandwidth weight` number field, default 1.
- Not shown in user list, user info, user management table, or any non-edit surface.

**Permissions**

- No new permissions. `user_edit` / `group_edit` gate access to the bandwidth_weight field, and it follows the same delegation pattern as the rest of Nexus: non-admins can set it to any value **at or below their own current resolved bandwidth weight**. They cannot grant a tier above their own, mirroring how non-admins can't grant permissions they don't have.
- The rule is enforced at the server, not in the UI — the number input uses the field's inherent bounds (`MIN_BANDWIDTH_WEIGHT..=u16::MAX`) for everyone, and the server rejects on submit if a non-admin's request exceeds their own resolved weight.
- The rule applies uniformly to every site that can change a user's or group's effective bandwidth:
  - `UserCreate` / `UserUpdate` with `bandwidth_weight: Some(N)` → `N ≤ requester.resolved_weight`
  - `UserUpdate` with `inherit_bandwidth_weight: Some(true)` → the inherited effective weight (admin-default → group → 1) ≤ requester's resolved weight
  - `UserCreate` / `UserUpdate` with `group_id: Some(g)` → `g.bandwidth_weight ≤ requester.resolved_weight`
  - `GroupCreate` with `bandwidth_weight: N` (non-default) → `N ≤ requester.resolved_weight`
  - `GroupUpdate` with `bandwidth_weight: Some(N)` → `N ≤ requester.resolved_weight`
- Admins bypass the rule entirely. The server reads the requester's current weight from `UserSession.bandwidth_weight` (the live `AtomicU16` cache), so admin downgrades take effect immediately on subsequent delegation attempts.
- Admin-only ServerInfo edit still covers rate-limit config (`max_outbound_rate`, `scheduler_chunk_size`) — existing pattern, unchanged.

**Implementation phasing (two PRs)**

1. ✅ **DONE** — Schema + plumbing (small, safe). DB migration, protocol additions, UI for weight, new Bandwidth section in Server Info (`max_outbound_rate` and `scheduler_chunk_size` fields), resolution helper cached on `UserSession`. Values are stored and settable but inert until phase 2 lands.
2. **Scheduler + cap (the big one).** New `nexus-server/src/scheduler/` module hosting WF2Q+ state + dispatch task. Wire all four accept loops, voice UDP exempt, LAN bypass at accept, migration of every `frame_writer.send` and transfer write to the `ConnectionWriter` API. Scheduler consumes the config values phase 1 made available.

**What Phase 1 delivered (for the Phase 2 implementer)**

- **Shared resolver**: `nexus_common::validators::resolve_bandwidth_weight(user_override: Option<u16>, group_weight: Option<u16>, is_admin: bool) -> u16` is the single source of truth for the precedence rule (per-user override > admin default > group inherit > system default). Phase 2's scheduler should call this when it needs to resolve at startup or in tests, rather than re-implementing the cascade.
- **Session cache**: `UserSession.bandwidth_weight: AtomicU16` (plain `AtomicU16`, not `Arc<AtomicU16>` — Phase 1 dropped the Arc so the scheduler's per-dispatch read is one pointer chase rather than two). Cloning a `UserSession` snapshots the atomic value; live updates flow through `UserManager::update_bandwidth_state` (per-user fan-out, writes override + resolved atomically) and `UserManager::update_bandwidth_weight_for_group_inheritors` (group cascade, filters by `bandwidth_weight_override.is_none()`). Multi-session invariant: every session of a given `user_id` holds the same value.
- **Typed update returns**: `db::UpdateUserResult::Updated { account: UserAccount, resolved_bandwidth_weight: u16 }` and `db::UpdateGroupResult { group: GroupRecord, previous_permissions: Permissions, final_permissions: Permissions }`. The resolved value is computed inside the same transaction as the write — no torn states, no follow-up read. Cache refreshes consume these directly, and any new code path that writes to bandwidth-relevant fields should follow the same shape. The group bandwidth cascade target set is no longer returned from `update_group`; the handler calls `update_bandwidth_weight_for_group_inheritors` post-commit to scan live session state.
- **DB clamp**: `db::util::clamp_db_bandwidth_weight(i64) -> u16` defends against corrupt rows and emits `warn!(raw, clamped, ...)` (constant `LOG_BANDWIDTH_WEIGHT_CLAMPED`) when it fires. Under normal operation it's the identity function.
- **Login disconnect**: `handle_login` disconnects with `err_database` if `get_resolved_bandwidth_weight` fails — seeding the session cache with a wrong value would silently demote/promote the user. Phase 2 should treat its own startup-time resolution failures the same way.

**Out of scope**

- Inbound rate limiting.
- Voice UDP shaping.
- Per-channel / per-file-area limits.
- Trust-based rate-limit bypass.
- Burst budgets beyond what chunk size already bounds.
