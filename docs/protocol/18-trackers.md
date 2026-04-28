# Trackers

Trackers are a discovery service for Nexus servers. A tracker maintains a list
of registered servers and exposes that list to clients on request. Trackers do
not relay BBS traffic, mirror server state, or mediate connections — once a
client picks a server from the list it connects directly to that server's BBS
port.

A Nexus server may register with zero or more trackers concurrently. A Nexus
client may query zero or more trackers when populating its server list. The
two roles are independent: nothing in this protocol requires a server to be
listed, and nothing requires a client to use a tracker at all.

## Ports

| Port | Protocol | Description                                     |
| ---- | -------- | ----------------------------------------------- |
| 7510 | TCP      | Tracker (TLS, framed JSON)                      |
| 7511 | TCP      | Tracker over WebSocket (requires `--websocket`) |

Trackers run as a separate daemon with their own TLS certificate. They use
the same TLS protocol requirements (TLS 1.2 or higher, mandatory) and the
same `NX|...` frame format as the BBS port (see
[README](README.md#frame-format)). Per-message-type payload limits apply.

The WebSocket port is optional and follows the same adapter model as the BBS
WebSocket port: TLS handshake → WebSocket handshake → byte-stream of `NX|...`
frames. See [README](README.md#websocket-transport) for the transport details.

The port range is reserved as follows: 7500–7509 for the BBS server (chat,
voice, transfers, and their WebSocket twins), 7510–7519 for the tracker.

## Handshake

After TLS, the connection begins with the same handshake exchange defined in
[01-handshake.md](01-handshake.md): the peer sends `Handshake { version }` and
the tracker replies with `HandshakeResponse { success, version, fingerprint }`.
The `version` exchanged here is the tracker protocol version, not the BBS
protocol version (see [Protocol Version](#protocol-version)). Version
compatibility rules — major must match exactly, minor follows the pre-1.0 /
post-1.0 rule from chapter 01 — are identical to the BBS handshake; only the
namespace differs.

The two-stage fingerprint verification (TLS-observed vs. previously stored,
then vs. server-reported) applies to any peer connecting to a tracker —
both servers connecting to register and clients connecting to query. The
"previously stored" value is implementation-defined: a client bookmark, a
server config-file pin, or none on first connect (TOFU).

There is no `Login` step. Trackers do not authenticate either side; the role
of the connecting peer (server vs. client) is established by the first message
sent after the handshake.

## Protocol Version

The tracker protocol versions independently of the BBS protocol. A single
Nexus release may ship one BBS protocol version and a different tracker
protocol version; they are tracked as separate constants in `nexus-common`.
Tracker-only changes bump only the tracker protocol version, leaving the
BBS protocol version untouched.

Current tracker protocol version: `0.1.0`.

Pre-1.0 minor bumps are breaking (per the same rule used by the BBS
protocol during its pre-1.0 phase).

## Post-Handshake Flows

Two flows are defined, distinguished by the first message:

1. **Server registration** — a server announces itself for inclusion in the
   tracker's listing. The connection is long-lived; the server periodically
   refreshes its entry until it disconnects or is delisted.
2. **Client listing** — a client requests the current set of registered
   servers. The connection is short-lived: one request, one response, close.

Each flow is specified in its own section below.

## Server Registration

The server connects to the tracker and keeps the connection open for the
lifetime of the listing. It re-sends `TrackerRegister` on a fixed refresh
interval to keep its entry fresh and to update mutable fields (current user
count). The refresh doubles as application-level keepalive: there is no
separate Ping/Pong on the tracker port. When the server shuts down or the
connection drops for any reason, the tracker delists the entry.

**Multiple trackers.** A server registering with N trackers maintains N
independent long-lived connections. Each connection is registered,
refreshed, and torn down on its own; failure of one tracker has no effect
on the others. The server is responsible for retry / backoff per-connection
(see [Reconnect backoff](#trackerregisterresponse)).

```
Server                                          Tracker
   │                                               │
   │  ─────── TLS Connection ──────────────►       │
   │                                               │
   │  Handshake { version }                        │
   │ ─────────────────────────────────────────►    │
   │                                               │
   │         HandshakeResponse { ..., fingerprint }│
   │ ◄─────────────────────────────────────────    │
   │                                               │
   │  TrackerRegister { password?, ... }           │
   │ ─────────────────────────────────────────►    │
   │                                               │
   │         TrackerRegisterResponse { success,    │
   │                                  refresh_interval }│
   │ ◄─────────────────────────────────────────    │
   │                                               │
   │  ... ( refresh_interval elapses ) ...         │
   │                                               │
   │  TrackerRegister { password?, ... }           │
   │ ─────────────────────────────────────────►    │
   │                                               │
   │         TrackerRegisterResponse { success,    │
   │                                  refresh_interval }│
   │ ◄─────────────────────────────────────────    │
   │                                               │
   │  ... ( connection closes — any reason ) ...   │
   │                                               │
   │         Tracker delists entry                 │
   │                                               │
```

### Messages

| Message                   | Direction        | Purpose                          |
| ------------------------- | ---------------- | -------------------------------- |
| `TrackerRegister`         | Server → Tracker | Initial registration and refresh |
| `TrackerRegisterResponse` | Tracker → Server | Acknowledge registration         |

The same `TrackerRegister` message is used for both initial registration and
each subsequent refresh; the tracker replaces the stored entry idempotently.
There is no separate update message.

### `TrackerRegister`

Sent by the server to register or refresh its entry. The same structure is
used for the initial registration and every refresh.

| Field            | Type   | Required | Description                                                                  |
| ---------------- | ------ | -------- | ---------------------------------------------------------------------------- |
| `password`       | string | If gated | Registration password (omit if the tracker is open)                          |
| `locale`         | string | No       | BCP-47 language tag for translated text in responses (default: `"en"`)       |
| `name`           | string | Yes      | Server display name                                                          |
| `description`    | string | No       | Free-form description                                                        |
| `address`        | string | No       | Public hostname or IP; tracker uses the connecting IP if omitted (see below) |
| `port`           | u16    | Yes      | BBS TCP port                                                                 |
| `websocket_port` | u16    | No       | BBS WebSocket port (only if `--websocket` is enabled)                        |
| `version`        | string | Yes      | Server software version (e.g., `"0.8.1"`)                                    |
| `fingerprint`    | string | Yes      | TLS cert fingerprint, canonical form (see below)                             |
| `user_count`     | u32    | Yes      | Distinct online users (matches the user list)                                |
| `allows_guest`   | bool   | Yes      | Whether the guest account is enabled                                         |

**`user_count` semantics.** Equals the count of entries the server would
show in its user list. Multiple sessions of the same regular account
collapse to one (single nickname). Shared-account and guest sessions
count individually because each has its own nickname. Pre-login
connections are excluded.

**Fingerprint format.** The `fingerprint` field MUST follow the canonical
Nexus fingerprint format: 32 uppercase hex bytes separated by colons (95
bytes total — `AA:BB:CC:...`). This matches the output of
`format_certificate_fingerprint` in `nexus-common`, the single source of
truth for fingerprint shape across the workspace. Trackers MUST validate
this format on each `TrackerRegister` and reject non-canonical values
(wrong length, lowercase hex, missing colons, non-hex characters) via
`TrackerRegisterResponse { success: false, error }`. Strict validation
guarantees clients see a single canonical form and can compare byte-equal
without normalization.

**Address resolution.** If `address` is omitted, the tracker substitutes the
remote IP of the registering connection. This supports servers on dynamic
IP addresses without requiring `ServerInfo.public_address` to be configured.
Servers behind NAT or proxies that want a stable hostname should set
`address` explicitly.

**Address encoding.** When `address` is a hostname, it MAY be Unicode
(IDN) or Punycode. Trackers store the address as-typed and return it
unchanged in `TrackerListResponse`. Clients are responsible for IDN →
Punycode conversion at connection time, matching the handling used for
`ServerInfo.public_address` elsewhere in Nexus.

**Refresh semantics.** Every refresh re-sends the full structure; the
tracker replaces the stored entry in full. There is no per-field update or
delta protocol. `user_count` is typically the only field that changes
between refreshes.

**No deduplication.** Each open connection is one tracker entry. The tracker
does not merge entries by fingerprint, address, or any other field.
Fingerprint is metadata for client-side server verification, not a storage
key.

**Field length limits.** Tracker implementations MUST enforce the
following maximum lengths, matching the limits used by `nexus-server`
for analogous fields. All string lengths are measured in UTF-8 bytes
(matching `str::len()` in the Rust reference implementation), not
Unicode characters. Values exceeding the limit are rejected with
`error_kind: invalid`.

| Field         | Max length     | Source constant in `nexus-common`                      |
| ------------- | -------------- | ------------------------------------------------------ |
| `name`        | 64 bytes       | `MAX_SERVER_NAME_LENGTH`                               |
| `description` | 512 bytes      | `MAX_SERVER_DESCRIPTION_LENGTH`                        |
| `password`    | 256 bytes      | `MAX_PASSWORD_LENGTH`                                  |
| `address`     | 253 bytes      | `MAX_PUBLIC_ADDRESS_LENGTH` (RFC 1035 DNS octet limit) |
| `version`     | 32 bytes       | `MAX_VERSION_LENGTH`                                   |
| `locale`      | 16 bytes       | `MAX_LOCALE_LENGTH`                                    |
| `fingerprint` | 95 bytes exact | Canonical form (see Fingerprint format above)          |

Numeric fields (`port`, `websocket_port`, `user_count`) follow the
ranges of their declared types.

**Example (open tracker, server with explicit address):**

```json
{
  "locale": "en",
  "name": "My BBS",
  "description": "Welcome to my server!",
  "address": "bbs.example.com",
  "port": 7500,
  "websocket_port": 7502,
  "version": "0.8.1",
  "fingerprint": "AA:BB:CC:DD:EE:FF:00:11:22:33:44:55:66:77:88:99:AA:BB:CC:DD:EE:FF:00:11:22:33:44:55:66:77:88:99",
  "user_count": 12,
  "allows_guest": true
}
```

### `TrackerRegisterResponse`

Sent by the tracker in reply to each `TrackerRegister`.

| Field              | Type    | Required   | Description                                                              |
| ------------------ | ------- | ---------- | ------------------------------------------------------------------------ |
| `success`          | boolean | Yes        | Whether the registration was accepted                                    |
| `refresh_interval` | u32     | If success | Seconds the server should wait before sending the next `TrackerRegister` |
| `error`            | string  | If failure | Human-readable explanation, localized per request `locale`               |
| `error_kind`       | string  | If failure | Machine-readable error category (see [Errors](#errors))                  |

**Refresh interval.** The tracker dictates the refresh cadence so it can
pace load (longer interval at scale, shorter for small or debug trackers).
The server applies the most recent `refresh_interval` it received; the
tracker may adjust the cadence over time by returning a different value on
any subsequent response.

**Refresh interval bounds.** Trackers SHOULD return `300` (5 minutes,
matching the BBS port's keepalive interval — see
[Chapter 15](15-keepalive.md)). This value keeps NAT mappings alive on
typical consumer routers while keeping `user_count` reasonably fresh.
Trackers MAY return shorter values under load or longer values for
low-volume deployments, but exceeding ~600 seconds risks NAT-induced
disconnects. Servers SHOULD apply an implementation-defined floor of at
least 120 seconds (2 minutes) to defend against misbehaving or
compromised trackers — there is no scenario where the protocol requires
faster refreshes than that.

**Failure handling.** When `success` is false, the tracker closes the
connection after sending the response. The server should not retry on the
same TLS session — if it wants to retry it must reconnect from scratch
(handshake first). Common rejection reasons: missing or wrong password,
malformed `TrackerRegister`, rate-limited, tracker at capacity.

**Reconnect backoff.** Servers SHOULD apply exponential backoff before
reconnecting after a failure. A reasonable starting point: 5 seconds on
first failure, doubling up to a 5-minute cap, with jitter. Reconnecting
immediately after a rate-limit or capacity failure defeats the tracker's
back-pressure and risks the IP being banned at the framing layer.

**Success example:**

```json
{
  "success": true,
  "refresh_interval": 300
}
```

**Failure example:**

```json
{
  "success": false,
  "error": "Invalid registration password",
  "error_kind": "unauthorized"
}
```

### Delisting

A server's entry is removed from the tracker's listing when any of the
following occurs:

- The TLS connection closes cleanly (server shutdown, restart).
- The TLS connection drops uncleanly (network failure, peer reset).
- The tracker has not received a `TrackerRegister` within its stale-entry
  timeout (2× `refresh_interval` — e.g., 600 seconds at the recommended
  300s refresh).

The first two are immediate; the third is a backstop for half-open
connections that haven't been detected as broken at the TCP layer.

## Client Listing

A client connects to the tracker, sends a single `TrackerList` request,
receives the current set of registered servers in `TrackerListResponse`, and
closes the connection. Listings are not subscriptions — clients re-query if
they want fresh data.

```
Client                                          Tracker
   │                                               │
   │  ─────── TLS Connection ──────────────►       │
   │                                               │
   │  Handshake { version }                        │
   │ ─────────────────────────────────────────►    │
   │                                               │
   │         HandshakeResponse { ..., fingerprint }│
   │ ◄─────────────────────────────────────────    │
   │                                               │
   │  TrackerList { password? }                    │
   │ ─────────────────────────────────────────►    │
   │                                               │
   │         TrackerListResponse { success,        │
   │                              servers }        │
   │ ◄─────────────────────────────────────────    │
   │                                               │
   │  ─────── Connection Closed ────────────       │
   │                                               │
```

### Messages

| Message               | Direction        | Purpose                         |
| --------------------- | ---------------- | ------------------------------- |
| `TrackerList`         | Client → Tracker | Request the current server list |
| `TrackerListResponse` | Tracker → Client | Return the server list or error |

The tracker closes the connection after sending `TrackerListResponse`,
regardless of success or failure. There is no keepalive, refresh, or
follow-up message defined for this flow.

### `TrackerList`

| Field      | Type   | Required | Description                                                            |
| ---------- | ------ | -------- | ---------------------------------------------------------------------- |
| `password` | string | If gated | Listing password (omit if the tracker is open)                         |
| `locale`   | string | No       | BCP-47 language tag for translated text in responses (default: `"en"`) |

`TrackerList` carries no filter, search, or pagination parameters in this
version of the protocol. The tracker returns the full current set of
registered servers; clients filter locally and may resort for views other
than the default name ordering (see [`TrackerListResponse`](#trackerlistresponse)).

**Example (open tracker):**

```json
{
  "locale": "en"
}
```

### `TrackerListResponse`

Sent by the tracker in reply to `TrackerList`.

| Field        | Type             | Required   | Description                                                |
| ------------ | ---------------- | ---------- | ---------------------------------------------------------- |
| `success`    | boolean          | Yes        | Whether the listing request was accepted                   |
| `servers`    | Server Entry [ ] | If success | Array of registered servers (see Server Entry below)       |
| `error`      | string           | If failure | Human-readable explanation, localized per request `locale` |
| `error_kind` | string           | If failure | Machine-readable error category (see [Errors](#errors))    |

**Empty list.** A tracker with zero registered servers responds with
`success: true` and `servers: []`. An empty list is not a failure.

**Listing size.** `TrackerListResponse` has no per-message-type payload
limit. The tracker always returns the full set of registered servers in
a single response.

**Ordering.** The tracker returns entries sorted alphabetically by `name`,
case-insensitive ascending. Clients may resort locally for other views,
but naive clients get a usable default presentation directly from the wire
order.

**Failure handling.** When `success` is false, the tracker closes the
connection after sending the response. Common rejection reasons: missing
or wrong password, malformed `TrackerList`, rate-limited.

### Server Entry

Each element of `TrackerListResponse.servers` has the following structure.
The fields mirror `TrackerRegister` with two differences: `password` is
never advertised, and `address` is always populated (the tracker resolves
the connecting-IP fallback before listing).

| Field            | Type   | Required | Description                                                                      |
| ---------------- | ------ | -------- | -------------------------------------------------------------------------------- |
| `name`           | string | Yes      | Server display name                                                              |
| `description`    | string | No       | Free-form description                                                            |
| `address`        | string | Yes      | Public hostname or IP (resolved by tracker)                                      |
| `port`           | u16    | Yes      | BBS TCP port                                                                     |
| `websocket_port` | u16    | No       | BBS WebSocket port (if the server has `--websocket`)                             |
| `version`        | string | Yes      | Server software version (e.g., `"0.8.1"`)                                        |
| `fingerprint`    | string | Yes      | TLS cert fingerprint, canonical form (see [`TrackerRegister`](#trackerregister)) |
| `user_count`     | u32    | Yes      | Distinct online users (see [`TrackerRegister`](#trackerregister))                |
| `allows_guest`   | bool   | Yes      | Whether the guest account is enabled                                             |

**Trust note.** The listed `fingerprint` is a display aid, not a trust
assertion. Clients SHOULD perform their own TOFU fingerprint check on
first connect to a server discovered via a tracker.

**`TrackerListResponse` success example:**

```json
{
  "success": true,
  "servers": [
    {
      "name": "My BBS",
      "description": "Welcome to my server!",
      "address": "bbs.example.com",
      "port": 7500,
      "websocket_port": 7502,
      "version": "0.8.1",
      "fingerprint": "AA:BB:CC:DD:EE:FF:00:11:22:33:44:55:66:77:88:99:AA:BB:CC:DD:EE:FF:00:11:22:33:44:55:66:77:88:99",
      "user_count": 12,
      "allows_guest": true
    },
    {
      "name": "Other BBS",
      "address": "other.example.com",
      "port": 7500,
      "version": "0.8.1",
      "fingerprint": "11:22:33:44:55:66:77:88:99:AA:BB:CC:DD:EE:FF:00:11:22:33:44:55:66:77:88:99:AA:BB:CC:DD:EE:FF:00",
      "user_count": 3,
      "allows_guest": false
    }
  ]
}
```

**`TrackerListResponse` failure example:**

```json
{
  "success": false,
  "error": "Invalid listing password",
  "error_kind": "unauthorized"
}
```

## Errors

Errors fall into two layers:

1. **Typed flow responses.** Validation, authentication, and rate-limit
   failures that the tracker can attribute to a specific request are
   returned via `TrackerRegisterResponse { success: false, error }` or
   `TrackerListResponse { success: false, error }`.
2. **The `Error` message.** Frame-level and role-level violations that
   don't map to a flow response — unknown message types, malformed frames,
   role mismatches — are reported via the generic `Error` message defined
   below.

In both cases the tracker closes the TLS connection immediately after
sending the response. There is no recovery on the same TLS session; the
peer must reconnect from scratch (handshake first).

All responses are best-effort. If the TLS write fails — for example,
because the peer has already closed — the connection is torn down without
further attempts.

### Localization

All human-readable error strings — the `error` field in typed responses
and the `message` field in `Error` — are translated server-side using
the `locale` provided in the request. Clients receive pre-translated
strings and can display them directly without further translation.

When the tracker sends `Error` before the request's `locale` is known —
for example, on a frame format violation that prevents `TrackerRegister`
or `TrackerList` from being parsed — the message is rendered in the
implementation's default locale (`"en"` for the reference tracker).

### `Error`

Sent by the tracker when it must abort the connection without a typed flow
response. Always followed by an immediate disconnect.

| Field     | Type   | Required | Description                                 |
| --------- | ------ | -------- | ------------------------------------------- |
| `message` | string | Yes      | Human-readable description of the violation |
| `command` | string | No       | The offending message type, if known        |

The `Error` frame echoes the `message_id` of the offending frame when one
was received; otherwise the tracker generates a fresh ID.

**Example (role violation):**

```json
{
  "message": "Role violation: connection is in client mode",
  "command": "TrackerRegister"
}
```

### Role Locking

The first valid post-handshake message determines the connection's role
and locks it in for the lifetime of the connection:

- A connection whose first post-handshake message was `TrackerRegister` is
  a **server connection**. Only subsequent `TrackerRegister` refreshes are
  valid on it.
- A connection whose first post-handshake message was `TrackerList` is a
  **client connection**. The tracker responds with `TrackerListResponse`
  and closes the connection; no further messages are expected.

Sending `TrackerList` on a server connection, or `TrackerRegister` on a
client connection, is a role violation. The tracker responds with `Error`
and disconnects.

A first post-handshake message that is neither `TrackerRegister` nor
`TrackerList` — for example, a BBS-port message — receives an `Error` and
disconnect.

### Failure Conditions

| Condition                                       | Tracker Response                                 | `error_kind`   |
| ----------------------------------------------- | ------------------------------------------------ | -------------- |
| Frame format violation (bad magic, bad framing) | `Error`, disconnect                              | —              |
| Payload exceeds per-message-type limit          | `Error`, disconnect                              | —              |
| Unknown message type                            | `Error`, disconnect                              | —              |
| Malformed JSON in a known message               | `Error`, disconnect                              | —              |
| Role violation                                  | `Error`, disconnect                              | —              |
| Missing or wrong password                       | Typed response with `success: false`, disconnect | `unauthorized` |
| Field validation failure                        | Typed response with `success: false`, disconnect | `invalid`      |
| Rate-limited                                    | Typed response with `success: false`, disconnect | `rate_limited` |
| Tracker at capacity                             | Typed response with `success: false`, disconnect | `capacity`     |

The generic `Error` message does not carry an `error_kind` — its
violation space is exclusively protocol-level, and clients do not branch
on it for happy-path logic.

## Timeouts

The tracker enforces low-level timeouts to bound resource use from
unauthenticated and half-broken connections.

| Phase                                                    | Timeout                                                                   | Behavior on expiry                |
| -------------------------------------------------------- | ------------------------------------------------------------------------- | --------------------------------- |
| TLS accepted, awaiting `Handshake`                       | 30 seconds                                                                | Disconnect, no response           |
| Frame completion (any frame, mid-read)                   | 60 seconds                                                                | `Error` (best-effort), disconnect |
| Awaiting first role-establishing message after handshake | 30 seconds                                                                | `Error`, disconnect               |
| Server connection idle between refreshes (stale entry)   | 2× `refresh_interval` (e.g., 600 seconds at the recommended 300s refresh) | Disconnect, delist (no response)  |

The first three timeouts mirror the BBS port and protect against
slowloris and resource-holding attacks. The last is the dominant liveness
discipline for long-lived server registrations; client connections close
promptly after the response and have no idle phase to time out.

## Security and Privacy

Trackers are a public discovery service. Operators and users should
understand the following properties before deciding to host or use one.

### Opt-In Visibility

A Nexus server is never listed without an explicit registration. Defaults
favor invisibility: a server with no trackers configured advertises
nothing and can only be reached by people who already know its address.

### Public Listing Contents

Every field in a `TrackerRegister` is visible to anyone with listing
access. There is no per-entry visibility control and no encryption of
individual entries; a listing password gates access to the whole list at
once.

### Passwords Are Coarse Gates

Both passwords are access gates, not identity or privacy guarantees:

- A **registration password** restricts who can submit entries. It does
  not authenticate which server is registering — anyone who knows the
  password can register any `name`, `address`, or `fingerprint`.
- A **listing password** restricts who can read the list. There is no
  key rotation, no per-user credential, and no audit trail on the wire.

Trackers wanting accountable access should layer additional controls
out-of-band; the protocol does not provide them.

### Fingerprint Trust Is Local

The `fingerprint` advertised in a registration is informational. Trackers
do not verify that a registering server's TLS certificate matches the
advertised fingerprint, and clients SHOULD NOT skip TOFU verification on
the basis of a tracker listing. The tracker itself is verified via the
same TLS fingerprint TOFU as any other Nexus endpoint.

### Plaintext Credentials Within TLS

Passwords are sent as plaintext JSON fields, protected only by the TLS
channel. Trackers are responsible for storing passwords hashed at rest.
The protocol does not prescribe a hashing scheme; Argon2id (the existing
Nexus practice) is recommended.

### Rate Limiting

Trackers SHOULD rate-limit:

- Failed authentication attempts per IP, to deter brute-force guessing.
- Connection rate per IP, to bound resource usage.
- `TrackerList` requests per IP, separately from connection rate, to
  deter scraping.

The protocol does not prescribe specific limits. Trackers are free to
respond with typed-response rate-limit errors or to drop connections at
the framing layer.

### Operator Visibility

Tracker operators see every registration and every listing query,
including source IPs. Operating a tracker is a trusted role. Servers and
clients that need to keep their network presence private from tracker
operators should reach each other through other channels.

### No Cross-Tracker Coordination

The protocol defines no communication between trackers. Each tracker is
an independent island. Servers register with each tracker independently;
clients aggregate results across trackers themselves. There is no
replication, no shared identity, and no quorum.

Clients aggregating results from multiple trackers will see the same
server appear in multiple listings. If clients choose to dedup, they
SHOULD do so on a key built from all entry fields except `user_count` —
the only volatile field, which differs across trackers reporting the
same server at different snapshot instants. Dedup must include
`address` and `port`: a malicious registrant could submit a known
`fingerprint` with a different network endpoint to silently suppress
the legitimate entry from a merged view. Excluding only `user_count`
means dedup fires only when entries agree on every field that should
match for the same server.
