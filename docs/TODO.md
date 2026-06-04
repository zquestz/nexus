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
| 8   | Speed limiting              | Medium | ✅ Done |
| 9   | Flood protection            | Low    | ✅ Done |
| 10  | Server logs                 | Medium | ✅ Done |
| 11  | Auto-away                   | Low    | ✅ Done |
| 12  | Certificate fingerprint pin | Low    | ✅ Done |
| 13  | Unicode name folding        | Low    | ✅ Done |

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

Weighted fair share of server outbound bandwidth per user, enforced via a single egress scheduler. All of a user's sessions — BBS and transfer, regular or shared account — share one flow keyed on `user_id`; the weight bounds that account's total outbound share.

**Current egress implementation (implemented)**

- BBS session broadcasts and queued cross-task sends go through `ConnectionWriter`, not a raw `mpsc` sender. `ConnectionWriter` has a bounded normal queue of 1024 `SessionEvent`s plus a priority control queue of exactly one slow-client disconnect event.
- BBS outbound frames route through the egress task: `ServerMessage` → complete frame bytes → `Arc<[u8]>` → scheduler chunks → per-connection dispatch channel → connection task writes raw pre-framed bytes.
- The scheduler core is WF2Q+-based, deterministic, and clock-free. The egress task owns the token bucket, lifecycle commands, staging, dispatch, ack, and write-failure cleanup.
- `DirectWriter` stages handler responses through the same egress path as priority frames, so direct responses are rate-accounted and cannot interleave into a partially written queued frame. `send_message_via_channel` remains normal-priority FIFO behind queued messages.
- Each BBS connection has a staged-frame admission limit (`DEFAULT_EGRESS_QUEUED_FRAMES_PER_CONNECTION = 32`). `QueueFull` is lossless for queued messages and maps direct-response `QueueFull` to drain-then-disconnect, not silent drop.
- Login transitions the connection from the global anon flow to `FlowId::User(user_id)` before `LoginResponse`, so authenticated BBS traffic uses the user's current resolved `bandwidth_weight`. Live `UserUpdate` / `GroupUpdate` weight changes update active BBS flows after commit by synchronously enqueueing settings commands under the user-state lock, preserving lock-order → FIFO ordering without any lock-held await.
- Transfer connections register as `ConnectionClass::Bulk`, transition from anon to `FlowId::User(user_id)` before transfer `LoginResponse`, and share the same user flow as that user's BBS sessions. Transfer auth keeps the expensive password work outside the user-state lock, then resolves the current weight and enqueues the transition under the read lock so it is ordered against concurrent weight updates.
- Transfer control frames route through egress as complete frames. Transfer `FileData` routes through the egress streaming-frame API as exactly one logical `FileData` frame on the wire: a header chunk, one or more file-data blocks, and the terminator as the only final chunk. The transfer reader does not read ahead; it stages one block, drains it, and then reads the next block, so memory remains bounded by the current file block and scheduler chunks rather than file size.
- Live `UserUpdate` / `GroupUpdate` weight changes update active transfer-only flows too. User updates key directly by `user_id`; group cascades use the transfer registry's active `user_id` set intersected with current DB inheritors, avoiding stale captured group metadata.
- Live Server Info updates call egress `set_max_outbound_rate` and `set_chunk_size` after the DB commit succeeds. Settings commands use a dedicated unbounded settings queue, so they can be enqueued under user/server-info locks without blocking or dropping; the egress task drains settings in FIFO order before ordinary command batches.
- Slow-client disconnect, parse-error disconnect, regular `Disconnect`, in-flight frame boundaries, and direct responses are all coordinated so the socket sees whole frames only.
- Covered policy tests pin Protocol-over-Bulk within a user flow, no multiplied user share from adding transfer sessions, shared-account sessions sharing one flow, cross-user Bulk weighted fairness, and rate-limited transfer `FileData` completion.
- Final routing shape is implemented for the current architecture: private-network peers bypass egress, WAN/Yggdrasil peers schedule through egress, and voice UDP remains outside this scheduler. The only remaining question is whether to keep the current BBS/transfer egress surfaces or do a future writer-task/raw-byte refactor.
- Real Yggdrasil testing at a 5 Mbps cap confirmed accurate aggregate rate limiting, Protocol responsiveness while Bulk transfers run, same-user transfer sharing, weighted cross-user sharing, and unlimited mode returning to high throughput. A 20 GB unlimited transfer test showed the default 8 KB chunk size remains reasonable, while 32 KB improves transfer-heavy throughput modestly and is a good tuning option.

**Algorithm: WF2Q+ (Bennett & Zhang, 1997)**

- Variable-size packet fair scheduler. O(log N) per dequeue, bounded delay vs GPS.
- Each user flow is one WF2Q+ "flow." Flow weight controls its share of the cap when contending; weight 10 vs weight 1 means 10× the share. Work-conserving — a single active flow gets the full cap.
- Paper: Bennett & Zhang, "Hierarchical Packet Fair Queueing Algorithms" (ICNP 1997) — <https://www.cs.cmu.edu/~hzhang/papers/ICNP97.pdf>.
- Reference C implementation: <https://github.com/zquestz/throttled/blob/master/src/wf2q%2B.h>.
- Defer to those for the virtual-time update rule and eligibility check — they're more precise than any in-spec prose.

**Future combined architecture with per-connection writer tasks**

This section is **not current behavior**. It describes a deferred raw-byte writer-task redesign that may replace the current, working BBS/transfer egress surfaces only if future profiling or maintenance needs justify it.

- One scheduler instance for the entire server, running as a dedicated tokio task that wakes when a flow has data and rate budget is available.
- **The scheduler task does zero I/O.** It is a pure dispatch engine: it maintains WF2Q+ state, rate-limits, and dispatches packets to per-connection bounded channels. It never calls `write_all` on a socket.
- **Per-connection writer tasks.** Each WAN connection gets a dedicated writer task (`tokio::spawn`) that owns the `WriteHalf` and drains its bounded channel. Writer tasks write to their own socket independently — a slow client's socket blocking its writer task cannot stall any other connection or the scheduler itself. This eliminates head-of-line blocking.
- The scheduler uses `try_send()` when dispatching to a connection's channel. If the channel is full (slow client), the packet stays in **that connection's queue** and **only that connection is skipped** (its writer channel is at the `WRITER_CHANNEL_BYTE_CAP` in-flight limit — a derived state, see Lifecycle) — the scheduler moves on and keeps dispatching the flow's other connections. The flow itself stays eligible as long as at least one of its member connections is dispatchable. No blocking, no waiting. Blocking is per-connection, not per-flow: a frozen session never stalls a sibling session of the same user. The flow remains the WF2Q+ scheduling unit (one weighted share per user flow), so cross-flow fairness is unchanged — isolation is purely about which of a flow's own connections gets serviced.
- **Drain notifications (wakeup path).** The scheduler must wake when a blocked connection's writer channel drains below `WRITER_CHANNEL_BYTE_CAP` — without busy-polling full channels or oversleeping. **Preferred model:** each connection keeps an atomic in-flight-byte counter (decremented as the writer completes each buffer); a writer that drops below the cap calls `notify_one()` on a single shared `tokio::Notify`. The scheduler's main loop `select!`s over the rate-budget timer, new submissions, and that `Notify`; on wake it rescans its flows' connections, deriving each connection's _blocked_ state from its counter (queued data + writer alive + `in_flight_bytes ≥ WRITER_CHANNEL_BYTE_CAP`). No stored `blocked` flag, no per-connection edge detection, no `ConnectionId` in the wakeup — at small connection counts the rescan is free. **Surgical fallback** (only if rescans ever prove too coarse): per-connection `Drained(ConnectionId)` sent on the full→available edge, which re-marks exactly that connection dispatchable; correctness (never leaving a drainable connection blocked) outranks minimizing notifications, so over-sending `Drained` is acceptable.
- LAN connections bypass the scheduler entirely and write directly (see "Scope of cap" below).
- **Outbound only.** Inbound shaping is wasteful (TCP + kernel handle it).
- **`TCP_NODELAY` set on every accepted TCP socket.** Without it, Nagle's algorithm coalesces small writes downstream of the application — defeating WF2Q+'s latency bounds for WAN connections and hurting chat latency on LAN too. Set at accept time regardless of LAN/WAN classification. WebSocket sockets inherit from the underlying TCP socket. UDP voice is unaffected (different protocol).
- **WebSocket adapter integration.** The WebSocket adapter (`nexus-server/src/websocket.rs`) exposes `AsyncRead + AsyncWrite` over a WS message stream. The writer task holds its `WriteHalf` and writes raw bytes; the adapter wraps each write in a binary WS frame. Scheduler-level chunking continues to work; WS frame boundaries are an internal layer transparent to the scheduler.

**Scope of cap**

- Implemented today: a single combined cap across BBS port (7500/7502) + transfer port (7501/7503), including WebSocket variants, for connections registered with egress.
- **Voice UDP exempt** — real-time, would degrade quality. UDP packets go directly to socket, not through scheduler.
- **LAN bypass implemented** via `nexus_common::address::is_private_network` (RFC 1918 + IPv6 ULA + loopback). **NOT** Yggdrasil — that mesh routes over the internet, counts as WAN bandwidth. Production BBS and transfer TCP/WebSocket connections keep LAN/private peers on the connection task's direct writer path and skip egress registration/transition entirely. Tests can disable the bypass through the connection params when they need loopback traffic to exercise egress. This decouples LAN traffic from the scheduler's dispatch task, so a saturating LAN client can't starve WAN traffic. The cap=0 optimization is unrelated: at cap=0 the scheduler still dispatches WAN connections (skipping rate sleep), so live cap changes take effect immediately on existing scheduled connections.

**Flow model**

- Flow = authenticated `user_id`. **All** of a user's sessions — BBS and transfer, regular or shared account — share one flow and one weighted share; sessions split it via the intra-flow rule below. The weight bounds the _account's_ total outbound share regardless of session count. This is deliberately per-account, not per-nickname: per-nickname flows would let anyone multiply their share by opening more sessions under distinct nicknames (a shared account permits many logins). Nicknames are display-only and never key a flow. To give a busy shared/guest login more aggregate bandwidth, an operator raises that account's weight. Consequence: a shared/guest login pools all its sessions into one share, so one heavy session starves its siblings — intended, and tunable via the account's weight.
- Pre-login traffic: one **single global pre-login flow** (`FlowId::Anon`, no per-IP split), weight = `ANON_FLOW_WEIGHT` (50). The flow is permanent — scheduled connections join its member set at register and leave on graduation/disconnect, so there's no per-IP refcount or reaping. The elevated weight is a latency knob, not a bandwidth grant: pre-login egress is tiny (a `HandshakeResponse`, maybe an error frame) and WF2Q+ is work-conserving, so the weight just keeps handshakes/logins responsive when the cap is saturated, then yields the unused share back. LAN/private connections bypass the scheduler from accept onward and never enter this flow.
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

**Scheduler-owned flow weights.** The scheduler maintains flow weights as plain `u16` values (not by reading `UserSession.bandwidth_weight` directly — `AtomicU16` clones snapshot, so the scheduler cannot hold a live reference). Weight updates arrive as explicit commands through the scheduler / egress handle:

- `update_user_weight(user_id, weight)` — updates the weight of the user's flow. One flow per `user_id`, so this is a single-flow update. In the final transfer-integrated scheduler this also reaches transfer-only users (not in `UserManager`'s session map) because the flow is keyed by `user_id`, not by session presence. Primary API for user/group weight changes; for a group cascade, call once per affected `user_id`.
- `transition_to_user(conn_id, user_id, weight)` — graduates a connection from its anon flow to its user flow at login, carrying the initial weight (see Lifecycle).

The flow key is `FlowId::User(user_id)` for authenticated traffic and `FlowId::Anon` pre-login (a single global flow) — `user_id` _is_ the scheduling identity, no separate metadata needed. A username rename never touches the scheduler: the flow is keyed on the immutable `user_id`, so the key is stable across renames (no `rename_flow`).

**Cache invalidation.** Two updates fire when a user's effective weight changes:

- **Server-side (session cache)**: refresh `UserSession.bandwidth_weight` (the `AtomicU16`) for every active session of the affected user via `UserManager::update_bandwidth_state`. For `UserUpdate` changing `bandwidth_weight` or `group_id`, that's the target user's sessions. For `GroupUpdate` changing the group's weight, fan out via `get_sessions_by_group_id` to every member's session (same pattern as group permission cascades).
- **Server-side (scheduler / egress)**: the same handler code that refreshes the session cache also calls `update_user_weight(user_id, resolved_weight)`. For a `UserUpdate`, one call for the target, including transfer-only users when the transfer registry shows an active flow. For `GroupUpdate`, BBS sessions are covered by `UserManager::update_bandwidth_weight_for_group_inheritors`, and transfer-only flows are covered by intersecting the registry's active `user_id`s with the DB's current inheriting members for the changed group (`SELECT id FROM users WHERE group_id = ? AND bandwidth_weight IS NULL`). Username renames need no scheduler call at all (the flow is keyed on `user_id`).
- **Client-visible**: the resolved weight is in `UserInfo`, so it naturally rides on existing `UserUpdated` broadcasts (which fire for renames, group changes, etc.). Weight-only changes do not require a new broadcast trigger — the value is "best-effort current at last broadcast" the same way `group_name` is today. Clients render the latest broadcast value; brief staleness on weight-only edits is acceptable since the client UI doesn't render weight for non-admins anyway.

**Intra-flow priority (Protocol-class connections before Bulk-class)**

Each connection carries a single class, fixed at accept time by which listener accepted it: `Protocol` (BBS port + WS-BBS) or `Bulk` (transfer port + WS-transfer). A connection therefore has exactly **one per-connection queue** of its own class — there are no per-connection sub-queues. The "two queues" exist at the flow level only because a flow can own both a BBS connection and transfer connections.

When a flow's turn comes up, the scheduler dispatches the highest-priority class that has at least one **dispatchable** connection (queued data + channel room), skipping blocked connections and round-robining among connections within the chosen class:

- **Protocol-class connections drained first.** A user's interactive BBS traffic preempts their own transfers.
- **Bulk-class connections drained only when no Protocol-class connection is dispatchable.**

Priority is evaluated over _dispatchable_ packets only. If the flow's Protocol-class connection is **blocked** (frozen client, channel full), its pending data does **not** hold back the flow's Bulk-class connections — the scheduler falls through and dispatches Bulk. This is what keeps a frozen BBS session from freezing the same user's downloads; the priority rule serves interactive latency, and a connection that can't receive anyway forfeits its precedence until it drains.

Within a user's own flow, this bounds the worst-case delay for that user's chat behind their _own_ already-dispatched Bulk chunk = one chunk transmission time (`chunk_size / cap_rate`). Across users it does not apply: inter-flow service is governed by WF2Q+ weights, so a user's Protocol packet still waits for their flow's weighted turn relative to other flows.

WF2Q+ virtual-time accounting is unaffected — a packet is a packet regardless of class or which connection it came from. Per-user-flow fairness vs other flows is preserved.

**Why Protocol starves Bulk — user behavior on slow connections.** Consider a user on a 0.5 Mbps link downloading a large file. They click a directory: the `FileListResponse` must arrive fast because they're actively interacting. If Protocol competes with Bulk for the same pipe, a 1 MB file listing takes ~32 seconds instead of ~16. With starvation, Protocol gets the full pipe — the listing arrives as fast as physically possible, the transfer pauses, and then resumes once the user stops clicking. The same logic applies to news listings, chat messages, user lists — anything interactive. Most Protocol messages are small and go out so fast that the starvation is invisible. When they're large (e.g., a huge directory listing), starvation is even more valuable because the user would notice the delay.

**Future writer-task transfer/raw-byte backpressure model.**

The current transfer implementation uses egress streaming frames for `FileData` and drains each staged block before reading the next block. The model below is a future writer-task/raw-byte refactor if we decide to replace the current frame/block drain loop with a generic byte writer surface.

- **Protocol `send_frame`: blocks only on its own connection's backlog, never on Bulk.** Each Protocol-class connection has a **`PER_CONNECTION_PROTOCOL_CAP` (4 MB)** — the blocking rule is: if that connection's pending Protocol bytes are **already ≥ cap**, `send_frame` blocks until drain brings them below; if pending bytes are **below cap**, the enqueue always succeeds (even if the message itself pushes the total over). This means exactly one message can overshoot the cap, but the next caller blocks until the queue drains. A single 5 MB `FileListResponse` gets through (we were under cap when it arrived), but the next `send_frame` blocks — no unbounded bypass. The 4 MB cap is extremely generous: Protocol traffic for a connection comes from its handler responses plus broadcasts. A connection would need to fall 4 MB behind on its own Protocol output to trigger blocking — realistically only a frozen client. When blocking does trigger, it pauses exactly the right thing: that connection task's `select!` rx arm (for broadcasts) or its handler (for direct responses). The broadcaster (`UserManager.broadcast()`) is never blocked — full session queues arm the slow-client disconnect control path and report `Ok(())`; scheduler-side backpressure must remain local to the slow connection's task. **Session isolation:** the cap is per-connection, so a frozen session's Protocol backlog is its own — it cannot block `send_frame` for any sibling session of the same user flow. The flow shares only the weighted rate budget, never buffer capacity. **Upstream bound:** the existing `ConnectionWriter` queue is already bounded at 1024 events and disconnects slow consumers when full; any future raw-byte writer refactor should keep that property rather than reintroducing an unbounded broadcast queue. The writer **stall timeout** (see "Writer task lifecycle") is what bounds an active zero-progress socket write inside the future writer task: zero write progress for `WRITER_STALL_TIMEOUT` (60s) → disconnect, capping accumulation at ~one stall-window of rate-limited broadcasts.
- **Bulk `send_bytes`: blocks when a connection's pending bulk bytes exceed `PER_CONNECTION_BULK_CAP` (1 MB).** Backpressure propagates naturally to callers: a transfer task pumping 64 KB chunks blocks when its own bulk queue is full, and the file read pauses. Per-connection, so a frozen transfer connection backpressures only itself, not the user's other transfers. A user opening many transfer connections does get a larger aggregate memory budget (bounded by `max_connections` / `max_connections_per_ip`) — an accepted trade for full per-session isolation.
- **Bulk can never block Protocol.** The caps are per-connection and independent. A saturating transfer cannot prevent a chat message or file listing from being enqueued and dispatched. Protocol-class connections are dispatched before Bulk-class within a flow, and every connection's cap is its own.
- **Pending-byte accounting.** The cap counts bytes still in **this connection's scheduler queue** — dispatched bytes leave the counter when handed to the writer channel. In the stuck case this stays accurate: a full writer channel refuses further dispatch, so the bytes remain queued and counted. The writer channel itself is bounded by **bytes, not packet count** — **`WRITER_CHANNEL_BYTE_CAP = 512 KiB`**, with the same one-oversized-packet-may-pass rule as the scheduler queue (a frame larger than the budget passes if in-flight bytes are below the cap, but the next dispatch waits). Implementation: the dispatch channel is a thin wrapper over `tokio::mpsc` (which is item-bounded) plus a per-connection in-flight-byte counter (or semaphore); "full" means `in_flight_bytes ≥ WRITER_CHANNEL_BYTE_CAP`, not a slot count. Byte-bounding (not packet-bounding) matters because at cap = 0 chunking is skipped, so a single Protocol "packet" can be a multi-MB `FileListResponse`; an 8-_packet_ channel would let 8 such frames pile up, whereas the byte budget admits at most one oversized frame ahead of the drain. No per-write progress reporting is needed — the budget counts every byte handed to the writer channel and releases it only when that buffer's write **completes** (not at `recv()`), so the buffer currently being written stays counted against the budget until it is fully on the socket. The `Drained` full→available edge is this byte total crossing back under the cap. True per-connection memory ≈ scheduler-queue soft cap + writer-channel soft byte cap (the active write buffer is inside that cap, not added to it).

**Chunking (scheduler-internal, protocol-transparent)**

The scheduler chunks every enqueued payload into `scheduler_chunk_size`-byte WF2Q+ packets. Chunking is invisible above the scheduler — TCP is a byte stream, the client reassembles frames as usual.

Worst-case added latency for a small message behind one already-dispatched chunk on the same flow = `chunk_size / cap_rate` (the non-preemptible transmission). This is the intra-flow bound only; WF2Q+ weights govern when the message's flow is served relative to other flows.

**Future optimization: when `max_outbound_rate = 0` (unlimited), chunking can be skipped.** The current egress implementation chunks at staging time even when the cap is unlimited. That is correct but not minimal work, and real unlimited-transfer testing shows it is good enough to defer. A future writer/raw-byte path could make chunking a dispatch-time decision: retain the payload as `Bytes` and slice it by the _current_ `scheduler_chunk_size` when cap > 0, or hand it over whole when cap = 0. If `set_rate` raises the cap before an unlimited payload finishes draining, the remaining payload must be sliced before dispatch so no oversized packet escapes the new bound.

**Rate bucket.** The implemented egress rate limiter is a token bucket with a bounded burst of four scheduler chunks (`EGRESS_RATE_BURST_CHUNKS = 4`). Tokens accumulate only to that bounded depth, so idle time cannot bank into an unlimited burst. Dispatch consumes the actual chunk length; if a chunk overshoots the current token count, the bounded deficit delays the next dispatch. At cap = 0 there is no rate limiting.

**Operator config (two knobs)**

| Setting                | Unit         | Default         | Bounds        | Storage              |
| ---------------------- | ------------ | --------------- | ------------- | -------------------- |
| `max_outbound_rate`    | Mbps (float) | `0` (unlimited) | `≥ 0`, finite | bytes/sec internally |
| `scheduler_chunk_size` | bytes        | `8192` (8 KB)   | 1 KB – 64 KB  | bytes                |

Mbps chosen to match how ISPs print plans (1 Gbps → `1000`, 100 Mbps → `100`, slow link → `0.5`). Live changes picked up on the next tick via `set_rate` / `set_chunk_size` (see Scheduler-owned global config below).

**Scheduler-owned global config.** `max_outbound_rate` and `scheduler_chunk_size` live inside the egress task. Two handle commands update them live:

- `set_rate(bytes_per_sec: u64)` — updates the token-bucket cap. `0` = unlimited.
- `set_chunk_size(bytes: u32)` — updates the chunk size for frames staged after the update and the rate bucket's burst depth. Already-staged chunks keep their existing ranges.

Both are invoked from the `ServerInfoUpdate` handler's post-commit runtime-side-effects block (where the `connection_tracker` / `flood_config` updates already apply) — never by polling the DB or restarting. Runtime update failures are logged but do not roll back the committed config change.

**Future connection writer API / optional refactor**

Current BBS integration keeps the existing `ConnectionWriter` / `DirectWriter` surface and routes BBS frames through the egress task from the connection loop. Current transfer integration uses `TransferEgress` with complete-frame control messages and streaming-frame `FileData`. A future cleanup could hide both behind one writer surface so callers (protocol handlers, transfer code) don't talk to the scheduler directly. Each connection task would hold a `ConnectionWriter` — an enum that internally routes to the scheduler (WAN) or writes directly to its own `WriteHalf` (LAN). Same API both ways:

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
    /// this connection's Protocol backlog exceeds PER_CONNECTION_PROTOCOL_CAP
    /// (4 MB); never blocked by Bulk backpressure or by a sibling connection.
    pub async fn send_frame(&mut self, msg: ServerMessage, msg_id: MessageId) -> Result<(), SendError>;

    /// Send raw bytes (transfer-port path). Takes ownership via `Bytes` so the
    /// scheduled path can enqueue without copying; the direct path forwards
    /// `bytes.as_ref()` to `write_all`. Blocks when this connection's bulk
    /// backlog exceeds PER_CONNECTION_BULK_CAP (Scheduled path only; Direct
    /// writes block on kernel backpressure).
    pub async fn send_bytes(&mut self, bytes: Bytes) -> Result<(), SendError>;

    /// Flush pending writes. No-op for Scheduled (scheduler handles dispatch
    /// timing). For Direct, calls the underlying writer's flush.
    pub async fn flush(&mut self) -> Result<(), SendError>;

    /// Shut down the writer. For Scheduled, triggers a *graceful* unregister:
    /// the scheduler keeps dispatching this connection's queue (Protocol and
    /// Bulk) until it empties, then drops the channel sender — bounded by the
    /// writer stall timeout so a frozen client is reaped, not waited on. For
    /// Direct, calls the underlying writer's shutdown.
    pub async fn shutdown(&mut self) -> Result<(), SendError>;

    /// Abort the writer (force-close paths — kick, ban, writer death). For
    /// Scheduled, triggers an *abort* unregister: the scheduler drops this
    /// connection's queue immediately (no drain) and the channel sender. For
    /// Direct, drops the underlying writer with no graceful flush. Sync —
    /// nothing to await.
    pub fn abort(&mut self);
}

pub enum SendError {
    ConnectionGone,  // socket closed, writer dropped, peer disconnect
}
```

Routing is a single enum branch — no heap allocation per send, no byte-level copy. The scheduler's internal API (`SchedulerHandle::send_*`) mirrors the same signatures.

`Drop for ConnectionWriter` is the **leak guard**: dropping a `Scheduled` writer without an explicit `shutdown()` / `abort()` (panic, early return) performs an abort-unregister — releasing the scheduler's registry entry and flow membership — so a connection task that exits unexpectedly can't strand a flow. Because `Drop` / `abort()` are sync and can't await, the unregister travels on a **dedicated unbounded cleanup channel** (separate from the bounded channel carrying weights / submissions / register), so it can't be backpressured — the leak guard is a guarantee, not best-effort; cleanup volume is bounded by connection count (one per teardown), so unbounded is safe. The lazy `try_send`-failure reap stays as a final backstop. Graceful delivery requires calling `shutdown()`; `abort()` and `Drop` are the force-close and safety-net paths.

**Zero-copy design.** The scheduler adds no byte-level copies beyond what the existing code already does:

| Step                   | Today       | With scheduler                    |
| ---------------------- | ----------- | --------------------------------- |
| Frame serialize        | 1 alloc     | 1 alloc                           |
| File read into buffer  | 1 alloc     | 1 alloc                           |
| Hand off to dispatcher | n/a         | move (no copy)                    |
| Chunk for WF2Q+        | n/a         | `Bytes::slice()` on Arc (no copy) |
| Socket write           | kernel copy | kernel copy                       |

Same total memory traffic as today. The scheduler adds bookkeeping (pointer/length, refcount) but no copies. `bytes` is already a transitive dependency in `Cargo.lock`; add it as a direct dependency on `nexus-server`.

Per-connection backpressure and the scheduler-registry semantics described next apply to scheduled (WAN) connections only. LAN connections use the kernel's natural `write_all` backpressure and surface socket errors directly to the connection task — they have no scheduler queue and no per-connection cap because they don't share the rate budget. See the "Backpressure model" section above for the full Protocol-never-blocked / Bulk-capped design.

**Writer errors and death detection.** Each WAN connection's writer task owns its `WriteHalf` and drains a bounded channel from the scheduler. When a writer task's `write_all` fails (socket error, peer disconnect), it exits, dropping its channel receiver. The connection task detects writer death via the writer task's `JoinHandle` in its `select!` loop — when the handle completes, the connection task breaks its read loop and cleans up the session. Future `send_*` calls on the `ConnectionWriter` after the writer has exited return `Err(ConnectionGone)`. The scheduler detects the dead channel on its next `try_send()` and removes the connection from its registry.

**Target combined lifecycle**

The scheduler maintains two internal registries:

- `connections: HashMap<ConnectionId, ConnectionEntry>` — per-connection state. `ConnectionEntry` holds the bounded channel sender (to the writer task), `ConnectionClass`, the current `FlowId` (an enum: `FlowId::Anon` or `FlowId::User(i64)`), **this connection's own queue**, its pending-byte counter (against `PER_CONNECTION_PROTOCOL_CAP` / `PER_CONNECTION_BULK_CAP` per class), and its writer-channel in-flight-byte counter. _Blocked_ is **derived** from that counter — queued data present, writer alive, and `in_flight_bytes ≥ WRITER_CHANNEL_BYTE_CAP` — evaluated at dispatch, not a stored flag. The `WriteHalf` itself is owned by the per-connection writer task, not stored here. `send_*(conn_id, …)` enqueues into **that connection's own queue**; the `FlowId` resolves which flow's weighted rate budget governs when the queue is dispatched.
- `flows: HashMap<FlowId, FlowState>` — per-flow **scheduling** state: weight, WF2Q+ virtual time, and the set of member `ConnectionId`s. No queue lives here — queues are per-connection (above); the flow only arbitrates which of its member connections to service next (Protocol-class before Bulk-class, skipping blocked connections, round-robin within a class). At dequeue the flow selects its next dispatchable packet from its member connections by that rule, then advances the flow's virtual finish time by **that packet's** length / weight — so per-connection queues stay consistent with flow-granular WF2Q+ accounting. The single pre-login flow (`FlowId::Anon`) is created at startup and never reaped; only user flows are created on first session and reaped on last disconnect.
- **Flow idle/reactivation (no phantom reservation).** A flow with no dispatchable connection (all members blocked or empty) is treated as **inactive** — removed from the eligible set, reserving no rate, so other flows receive the full cap. When a member becomes dispatchable again the flow re-enters with its virtual start tag clamped to `max(its last virtual finish, current system V(t))` — standard WFQ idle handling. Without the clamp, a stale head tag from before the idle gap would grant catch-up service, violating the no-phantom-reservation property the work-conservation test asserts. Concretely, assign WF2Q+ start/finish tags when a packet becomes **dispatchable**, not at enqueue — otherwise a head packet queued before the idle gap carries a stale tag and reproduces the catch-up burst the flow-level clamp was meant to prevent.

- **Connection register**: current BBS and transfer connections register after TLS/WebSocket setup only when LAN bypass does not apply. Scheduled BBS / WS-BBS connections use `ConnectionClass::Protocol`; scheduled transfer / WS-transfer connections use `ConnectionClass::Bulk`. LAN/private peers identified by `is_private_network(peer_ip)` keep their writer on the connection task and bypass egress entirely. Production enables this bypass; tests can disable it through the connection params when loopback must exercise scheduler behavior.
- **Pre-login**: scheduled traffic flows into the single global pre-login flow `FlowId::Anon`, weight = `ANON_FLOW_WEIGHT`. The connection joins the flow's member set (no refcount). LAN/private connections bypass the scheduler from accept onward and never enter this flow.
- **Login transition (BBS port)**: after auth succeeds but **before sending `LoginResponse`**, the login handler calls `scheduler.transition_to_user(conn_id, user_id, weight)`. The scheduler atomically:
  - Swaps the conn's `FlowId` to `FlowId::User(user_id)`.
  - Removes the connection from the pre-login flow's member set (the pre-login flow is permanent — never reaped).
  - If this is the first session for that user, creates the user flow with the provided `weight`. If the user flow already exists (multi-session login — a shared account, or a concurrent transfer connection), the new connection joins it and inherits its current weight — sessions of the same `user_id` always resolve to the same weight, so this is consistent by construction.
  - Initializes the new flow's WF2Q+ virtual-time state at the current system V(t).

  The `LoginResponse` itself is then sent through the new user flow — so the response message is the first thing the user's flow dispatches at their proper weight. Protocol semantics guarantee the client sends nothing between `Login` and receiving `LoginResponse`, so this window is the safe transition point.

- **Login transition (transfer port)**: transfer auth is split so credential verification runs first, then `complete_transfer_login` takes the user-state read lock, refreshes the identity snapshot, resolves the current bandwidth weight, synchronously enqueues `transition_to_user(conn_id, user_id, weight)` on the settings queue, drops the lock, and then sends `LoginResponse`. The transfer connection joins the same user flow as the user's BBS connections (if any), sharing the WF2Q+ allocation. The login-requested nickname is **not** consulted for scheduling — it stays display-only (logging, `TransferRegistry`) — so a shared account requesting another user's nickname gains no scheduling identity or weight.
- **Username rename**: no scheduler action. The flow is keyed on the immutable `user_id`, so a username (or nickname) change never moves a connection between flows. Only a weight change matters — handled by the `update_user_weight` call in Cache invalidation above.
- **Unregister — graceful vs abort.** Behavior depends on the **reason**, not the connection's class:
  - **Graceful** (clean transfer completion, normal logout, `send_error_and_disconnect`): the scheduler keeps the connection in its dispatch loop and drains the remaining queue — both classes — to the writer until empty, then drops the sender. This is what `shutdown()` invokes. As long as the writer keeps making progress, this delivers the file tail **and** the final `TransferComplete` (the Bulk queue at clean completion holds up to `PER_CONNECTION_BULK_CAP` of un-dispatched file data plus the completion frame). Delivery is therefore bounded best-effort, not absolute: the drain is capped by `WRITER_STALL_TIMEOUT`, so a client that stops reading is reaped and the remainder dropped — a clean logout can never hang on a frozen socket.
  - **Abort** (ban termination, kick mid-transfer, writer death): the queue is **dropped** immediately — no value in pushing megabytes of stale transfer data at a connection being force-closed, and draining could stall teardown. Drop the sender; the writer drains whatever already reached the channel, flushes, exits.
- **Connection disconnect**: after unregister completes and the writer task exits, the scheduler removes the conn_id from `connections`. The flow stays alive if other conn_ids still reference it.
- **Last session disconnects**: when the last conn_id referencing a user flow disconnects, the user flow is reaped.
- **Pre-login flow membership**: a single permanent global flow (`FlowId::Anon`); connections join its member set at register and are removed on graduation (login) or disconnect. The flow itself is never reaped.
- **Writer task lifecycle**: the writer task runs a simple loop: `recv() → write buffer → (continue)`. **Stall timeout (`WRITER_STALL_TIMEOUT = 60s`, fixed const mirroring the inbound frame timeout):** it writes each buffer with a `write()` loop (partial writes), arming a `WRITER_STALL_TIMEOUT` deadline that **resets on any write returning > 0 bytes**. The timeout fires only when zero bytes are accepted for the full 60 s — a true TCP zero-window stall that neither retransmission timeout nor `SO_KEEPALIVE` ever reaps — so a slow-but-progressing socket is never killed, regardless of payload size or whether chunking is active. On stall the writer treats the client as dead and exits. On channel close (unregister path), it drains remaining items, flushes, and shuts down the socket. On write error **or stall timeout**, it exits immediately. The connection task detects writer death via the `JoinHandle` completing in its `select!` loop, breaks its read loop, and proceeds to session cleanup.

**Logging**

- `INFO`: cap changes (rate or chunk size — operator visibility), flow created/reaped (debugging fairness), scheduler-detected unrecoverable error.
- `DEBUG`: a connection stays blocked (channel full) > N seconds (slow-client signal), per-flow stats on demand. Off in production logs unless explicitly enabled.
- No `TRACE`-level per-dispatch logging (would flood logs at line rate).

**Testing**

- Unit tests for WF2Q+ math: virtual-time advance, eligibility checks, finish-time ordering for variable packet sizes. Reproducible without I/O.
- **Property-based fairness tests** (the mitigation for WF2Q+'s implementation subtlety): over randomized packet/weight sequences assert (a) per-flow throughput ratios converge to the configured weight ratios, and (b) no flow's cumulative service runs ahead of its GPS allocation by more than one max-packet — the worst-case-fairness invariant WF2Q+ is defined by. Deterministic, no I/O.
- Integration tests for fairness scenarios use `tokio::time::pause()` and explicit `advance(...)`, not real-time waits. This deterministically tests "user A with weight 10 and user B with weight 1 should see 10:1 throughput ratio over N dispatches."
- Tests for lifecycle edge cases: connection registered while flow was already alive (multi-session login), connection disconnect while in-flight packet hadn't finished, anon→user transition while bytes were queued.
- Tests for backpressure / unregister behavior: **blocked-connection wakeup** (a connection blocked on a full channel resumes once it drains below cap — via the `Notify` rescan, or the `Drained(c)` edge if that scheme is used); **oversized Protocol frame cap** (one frame may overshoot `PER_CONNECTION_PROTOCOL_CAP`, the next `send_frame` on that connection blocks until drain, no deadlock); **transfer-only weight update** (`update_user_weight` reaches a user flow whose only connection is a transfer, with no `UserManager` session); **graceful vs abort unregister** (a graceful `shutdown()` drains both classes to empty so a transfer's final `TransferComplete` is delivered; an abort drops the backlog immediately).
- Tests for **per-session isolation** within one user flow (regular-account multi-session, or shared-account sessions): a frozen session (full channel, backlog at `PER_CONNECTION_PROTOCOL_CAP`) does **not** stall dispatch to, or block `send_frame` on, a sibling session of the same user flow — the healthy session keeps draining at the flow's full weighted share while the frozen one is skipped. Includes the cross-class case (a frozen Protocol-class connection does not hold back the flow's Bulk-class connections).
- Work-conservation under blocking: when **every** member connection of a flow is blocked (all channels full), that flow reserves no rate — other flows receive the full cap until one of the blocked flow's connections drains below cap (the `Notify` rescan, or a matching `Drained(c)`) and becomes dispatchable again. (No phantom reservation for an entirely-stalled flow.)
- Blocked-flow reactivation: a flow whose connections were all blocked, then drain, resumes at its fair share with **no catch-up burst** — its first post-idle packet is tagged against current V(t), not a stale pre-idle tag.
- Writer stall timeout: a connection that accepts zero bytes while data is queued is disconnected within `WRITER_STALL_TIMEOUT`; a slow-but-progressing connection (accepts bytes each interval) is **not** reaped. Uses `tokio::time::pause()` + `advance()`.
- Writer-channel byte budget (cap = 0, no chunking): one oversized Protocol packet enters the byte-bounded writer channel (in-flight was below `WRITER_CHANNEL_BYTE_CAP` when it arrived); a second oversized packet stays scheduler-queued and counted until the writer **completes** the in-flight write that frees enough channel capacity to admit it.
- Cap enabled mid-flight: a large Bulk payload enqueued while `max_outbound_rate = 0` is sliced to the current `scheduler_chunk_size` when `set_rate` raises the cap before the payload finishes draining — no packet exceeds the chunk size and no burst exceeds one chunk.

**Docs (done)**

- `docs/server/02-configuration.md` documents the Bandwidth section, `max_outbound_rate`, `scheduler_chunk_size`, LAN bypass, Yggdrasil-as-WAN behavior, and chunk-size tuning guidance.
- `docs/server/05-user-management.md` explains `bandwidth_weight`, the user/group resolution rule, worked examples, and the shared-account / guest bandwidth collapse: all sessions of a shared account share one weighted flow, so N guests collectively get one user's share and one guest's heavy download can starve the others. This is intentional and tunable by raising that account's or group's weight.
- `docs/client/10-server-info.md` documents the Bandwidth fields in the Server Info panel.
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

**Implementation phasing**

1. ✅ **DONE** — Schema + plumbing (small, safe). DB migration, protocol additions, UI for weight, new Bandwidth section in Server Info (`max_outbound_rate` and `scheduler_chunk_size` fields), resolution helper cached on `UserSession`.
2. ✅ **DONE** — Queued session writer safety. `ConnectionWriter` now wraps a bounded 1024-message normal queue plus a one-slot slow-client control queue. Producers use non-blocking `try_send`: `Full` drops the attempted event, arms the priority slow-client disconnect, and returns `Ok(())`; `Closed` returns `Err` for stale-session cleanup. The connection task prioritizes the control queue, sends localized `Error { disconnect: true }`, then exits through normal cleanup. Direct and queued BBS writes also have a 30-minute per-message write timeout.
3. ✅ **DONE** — Writer surface split. `ConnectionWriter` is the queued cross-task session writer; `DirectWriter` is the immediate handler response writer. Handlers no longer expose raw `FrameWriter`, keeping BBS write policy centralized and making the scheduler integration point clearer.
4. ✅ **DONE** — WF2Q+ scheduler core. `nexus-server/src/scheduler/` hosts the clock-free WF2Q+ state, anon/user flows, per-user weights, per-connection classes, tag-preserving in-flight state, priority packet lane, block/unblock, transition, weight update, and lifecycle tests.
5. ✅ **DONE** — BBS egress manager/task. `nexus-server/src/egress/` owns frame staging, zero-copy `Arc<[u8]>` chunking, per-connection staged-frame admission (32 frames), dispatch/ack/write-failure cleanup, lossless `QueueFull`, priority frame staging, and token-bucket rate limiting.
6. ✅ **DONE** — BBS connection wiring. BBS connections register as anon, transition to user before `LoginResponse`, stage queued and direct frames through egress, preserve frame boundaries, drain before disconnect, handle priority `QueueFull` by drain-then-close, and apply live rate/chunk-size and weight-update commands.
7. ✅ **DONE** — Transfer integration. Transfer-port / WS-transfer connections register as Bulk, transition to the user flow before transfer `LoginResponse`, route control frames and streaming `FileData` through egress, preserve the one-`FileData`-frame wire protocol, and make user/group weight cascades cover transfer-only flows.
8. ✅ **DONE** — Transfer egress hardening. Tests cover Protocol-over-Bulk priority, no user-share multiplication from BBS+transfer sessions, shared-account flow sharing, cross-user Bulk weighted fairness, rate-limited `FileData`, small-file streaming, mid-stream ban/error cleanup, and re-register-after-stream-failure recovery.
9. ✅ **DONE** — Final routing policy. LAN/private BBS and transfer peers bypass egress, WAN/Yggdrasil peers remain scheduled, voice UDP stays exempt, and the current BBS/transfer egress surfaces are correct and tested. The future unified writer-task/raw-byte refactor remains optional.

**Optional speed-limiting follow-up**

- **Writer-task/raw-byte decision**: the current BBS/transfer egress surfaces are correct, tested, and perform well enough in real Yggdrasil transfer tests. Keep the larger unified writer-task/raw-byte API deferred unless future profiling shows a production-relevant bottleneck or the codebase needs the cleanup for other reasons.
- **Throughput tuning**: keep 8192 bytes as the conservative default for interactive latency. Transfer-heavy servers on fast links can raise `scheduler_chunk_size` (for example 16384 or 32768 bytes) before considering any deeper credit/raw-byte redesign.

**Broadcast avatar diet (companion — done)**

Standalone bandwidth fix the bounded-broadcast bound relies on. Avatars (≤176 KB data URIs) used to ride every `UserUpdated` — away/back/status, admin edit, group cascade, disconnect — though they never change within a session and the client hashes-and-discards unchanged ones; auto-away made that the dominant broadcast volume. Now the avatar = `UserManager::aggregate_avatar(live sessions)` — the most recent login that supplied one (regular accounts; shared are per-session) — carried only on **snapshots** (`UserConnected`, `UserListResponse`, `UserInfoResponse`) and on a **disconnect** `UserUpdated` that changes the aggregate (`Some(bytes)`, or `Some("")` = removed). Every other `UserUpdated` sends `avatar: None` and the client keeps its cache; a no-avatar login never blanks an existing avatar. Drops `UserUpdated` from ~176 KB to ~KB. Spec: `docs/protocol/04-users.md` → Avatar Handling.

**What the schema/plumbing phase delivered**

- **Shared resolver**: `nexus_common::validators::resolve_bandwidth_weight(user_override: Option<u16>, group_weight: Option<u16>, is_admin: bool) -> u16` is the single source of truth for the precedence rule (per-user override > admin default > group inherit > system default). The scheduler / egress implementation should use this when it needs to resolve weights in new integration paths or tests, rather than re-implementing the cascade.
- **Session cache**: `UserSession.bandwidth_weight: AtomicU16` (plain `AtomicU16`, not `Arc<AtomicU16>` — nothing holds a cross-task live reference to it, so the `Arc` was unnecessary; the `AtomicU16` itself remains because handler threads read and refresh it concurrently). The scheduler does **not** read this cache — it owns its own `u16` flow weights updated through explicit `SchedulerHandle` commands (see "Scheduler-owned flow weights"). This cache exists only for the handler layer's delegation checks (a non-admin can't set a weight above their own). Cloning a `UserSession` snapshots the atomic value; live updates flow through `UserManager::update_bandwidth_state` (per-user fan-out, writes override + resolved atomically) and `UserManager::update_bandwidth_weight_for_group_inheritors` (group cascade, filters by `bandwidth_weight_override.is_none()`). Multi-session invariant: every session of a given `user_id` holds the same value.
- **Typed update returns**: `db::UpdateUserResult::Updated { account: UserAccount, resolved_bandwidth_weight: u16 }` and `db::UpdateGroupResult { group: GroupRecord, previous_permissions: Permissions, final_permissions: Permissions }`. The resolved value is computed inside the same transaction as the write — no torn states, no follow-up read. Cache refreshes consume these directly, and any new code path that writes to bandwidth-relevant fields should follow the same shape. The group bandwidth cascade target set is no longer returned from `update_group`; the handler calls `update_bandwidth_weight_for_group_inheritors` post-commit to scan live session state.
- **DB clamp**: `db::util::clamp_db_bandwidth_weight(i64) -> u16` defends against corrupt rows and emits `warn!(raw, clamped, ...)` (constant `LOG_BANDWIDTH_WEIGHT_CLAMPED`) when it fires. Under normal operation it's the identity function.
- **Login disconnect**: `handle_login` disconnects with `err_database` if `get_resolved_bandwidth_weight` fails — seeding the session cache with a wrong value would silently demote/promote the user. Transfer login now mirrors this: `complete_transfer_login` refreshes the snapshot under the user-state read lock, returns a login error on snapshot failure, and only transitions after obtaining the current resolved weight.

**Out of scope**

- Inbound rate limiting.
- Voice UDP shaping.
- Per-channel / per-file-area limits.
- Trust-based rate-limit bypass.
- Burst budgets beyond what chunk size already bounds.
