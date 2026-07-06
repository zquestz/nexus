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
protocol version. Tracker-only changes bump only the tracker protocol
version, leaving the BBS protocol version untouched.

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
lifetime of the listing. It re-sends `TrackerServerRegister` on a fixed refresh
interval to keep its entry fresh and to update mutable fields (current user
count). The refresh doubles as application-level keepalive: there is no
separate Ping/Pong on the tracker port. When the server shuts down or the
connection drops for any reason, the tracker delists the entry.

**Multiple trackers.** A server registering with N trackers maintains N
independent long-lived connections. Each connection is registered,
refreshed, and torn down on its own; failure of one tracker has no effect
on the others. The server is responsible for retry / backoff per-connection
(see [Reconnect backoff](#trackerserverregisterresponse)).

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
   │  TrackerServerRegister { password?, ... }     │
   │ ─────────────────────────────────────────►    │
   │                                               │
   │         TrackerServerRegisterResponse {       │
   │           success, refresh_interval }         │
   │ ◄─────────────────────────────────────────    │
   │                                               │
   │  ... ( refresh_interval elapses ) ...         │
   │                                               │
   │  TrackerServerRegister { password?, ... }     │
   │ ─────────────────────────────────────────►    │
   │                                               │
   │         TrackerServerRegisterResponse {       │
   │           success, refresh_interval }         │
   │ ◄─────────────────────────────────────────    │
   │                                               │
   │  ... ( connection closes — any reason ) ...   │
   │                                               │
   │         Tracker delists entry                 │
   │                                               │
```

### Messages

| Message                         | Direction        | Purpose                          |
| ------------------------------- | ---------------- | -------------------------------- |
| `TrackerServerRegister`         | Server → Tracker | Initial registration and refresh |
| `TrackerServerRegisterResponse` | Tracker → Server | Acknowledge registration         |

The same `TrackerServerRegister` message is used for both initial registration and
each subsequent refresh; the tracker replaces the stored entry idempotently.
There is no separate update message.

### `TrackerServerRegister`

Sent by the server to register or refresh its entry. The same structure is
used for the initial registration and every refresh.

| Field            | Type   | Required | Description                                                                  |
| ---------------- | ------ | -------- | ---------------------------------------------------------------------------- |
| `password`       | string | If gated | Registration password (omit if the tracker is open)                          |
| `locale`         | string | No       | BCP-47 language tag for localized text in responses (default: `"en"`)        |
| `name`           | string | Yes      | Server display name (non-empty after trim, no newlines or control chars)     |
| `description`    | string | No       | Free-form description (no newlines or control chars)                         |
| `address`        | string | No       | Public hostname or IP; tracker uses the connecting IP if omitted (see below) |
| `port`           | u16    | Yes      | BBS TCP port (must be non-zero)                                              |
| `websocket_port` | u16    | No       | BBS WebSocket port (only if `--websocket` is enabled; must be non-zero)      |
| `version`        | string | Yes      | Server software version, valid semver (e.g., `"0.9.7"`)                      |
| `fingerprint`    | string | Yes      | TLS cert fingerprint, canonical form (see below)                             |
| `user_count`     | u32    | Yes      | Distinct online users (matches the user list)                                |
| `allows_guest`   | bool   | Yes      | Whether the guest account is enabled                                         |

**Field validation.** Where a field is shared with the BBS protocol's
`ServerInfoUpdate`, the tracker applies the same rule the BBS server
applies — `name`, `description`, `locale`, and `version` use the same
validation rules verbatim, so a value that registers cleanly is also
one the BBS server would have accepted on its own configuration.
Tracker-specific rules:

- `port` must be non-zero. `websocket_port`, when present, must be
  non-zero.
- `fingerprint` must match the canonical 95-byte uppercase form
  (see Fingerprint format below).
- `address`, when present, is validated by the tracker's
  address-classification rules ([Address validation](#address-validation)
  below).

All validation failures land as
`TrackerServerRegisterResponse { success: false, ... }` with
`error_kind: "invalid"`.

**`user_count` semantics.** Equals the count of entries the server would
show in its user list. Multiple sessions of the same regular account
collapse to one (single nickname). Shared-account and guest sessions
count individually because each has its own nickname. Pre-login
connections are excluded.

**Fingerprint format.** The `fingerprint` field MUST be the SHA-256
digest of the server's TLS certificate, encoded as 32 uppercase hex
bytes separated by colons (95 bytes total — `AA:BB:CC:...`). This is
the same canonical form used elsewhere in Nexus (see
[01-handshake.md](01-handshake.md)). Trackers MUST validate this
format on each `TrackerServerRegister` and reject non-canonical values
(wrong length, lowercase hex, missing colons, non-hex characters) via
`TrackerServerRegisterResponse { success: false, error }`. Strict validation
guarantees clients see a single canonical form and can compare byte-equal
without normalization.

**Address resolution.** If `address` is omitted (or empty), the tracker
substitutes the remote IP of the registering connection without further
checks — the peer IP is kernel-supplied evidence that the registrant
controls that endpoint. This supports servers on dynamic IP addresses
without requiring `ServerInfo.public_address` to be configured. Servers
behind NAT or proxies that want a stable hostname should set `address`
explicitly, in which case the tracker validates it as described below.

#### Address validation

When `address` is provided (non-empty), trackers MUST validate it before
accepting the registration. Validation runs in the order below; the
first failure produces a typed
`TrackerServerRegisterResponse { success: false, error_kind: "invalid" }`
and closes the connection.

1. **Structural.** Reject schemes (`://`), URL brackets (`[`, `]`),
   path components (`/`), userinfo separators (`@`), embedded ports
   (e.g., `host:port`), IPv6 zone identifiers (`fe80::1%eth0`), and
   whitespace. Hostnames must pass IDNA 2008 well-formedness, with
   per-label and total length caps matching DNS wire-form (63 octets
   per label, 253 octets total).
2. **Hard-reject classification.** When `address` parses as an IP
   literal, reject the following categories regardless of the peer's
   source IP — they are never valid public unicast endpoints:

   | Category      | IPv4                                                | IPv6            |
   | ------------- | --------------------------------------------------- | --------------- |
   | Loopback      | `127.0.0.0/8`                                       | `::1`           |
   | Unspecified   | `0.0.0.0/8` ("this network", RFC 1122)              | `::`            |
   | Link-local    | `169.254.0.0/16`                                    | `fe80::/10`     |
   | Multicast     | `224.0.0.0/4`                                       | `ff00::/8`      |
   | Documentation | `192.0.2.0/24`, `198.51.100.0/24`, `203.0.113.0/24` | `2001:db8::/32` |
   | Broadcast     | `255.255.255.255`                                   | (n/a)           |

3. **LAN-peer bypass.** When the peer's source IP is on a private
   network — RFC 1918 (`10.0.0.0/8`, `172.16.0.0/12`,
   `192.168.0.0/16`), IPv6 ULA (`fc00::/7`), or IPv4/IPv6 loopback —
   the tracker accepts `address` without further checks. Local DNS
   may not resolve advertised hostnames; an operator on a LAN can't
   make a public address match their RFC 1918 source IP. Yggdrasil
   mesh (`0200::/7`) is _not_ in the bypass set: those addresses
   behave like public addresses within the mesh, so the same
   address-vs-peer binding applies. CGNAT (`100.64.0.0/10`,
   RFC 6598) is also not in the bypass set: a CGNAT-fronted operator
   should register a hostname (e.g., dynamic DNS) so the
   hostname-vs-peer match still applies.
4. **IP-literal match.** For peers not in the bypass set, an IP-
   literal `address` MUST equal the peer's source IP exactly. Family
   mismatch (IPv4 literal advertised by an IPv6-connected peer or
   vice versa) is rejected; see the dual-stack note below.
5. **Hostname resolution.** For peers not in the bypass set, the
   tracker resolves the address (after IDN → Punycode conversion) via
   the host's resolver and accepts the registration only if the
   peer's source IP appears in the result set. The DNS lookup is
   bounded by a tracker-side timeout (15 seconds in the reference
   implementation). Resolver outcomes:

   | Outcome                            | Initial register    | Refresh            |
   | ---------------------------------- | ------------------- | ------------------ |
   | Peer IP in result set              | Accept              | Accept             |
   | Peer IP not in result set          | Reject (no match)   | Reject (no match)  |
   | NXDOMAIN / empty result            | Reject (not found)  | Reject (not found) |
   | Transient (timeout, network error) | Reject (DNS failed) | Soft-pass          |

   Initial register hard-rejects on transient resolver failures so a
   brand-new entry can't slip in unverified during a DNS blip.
   Refresh soft-passes the same conditions so an established entry
   isn't evicted by a brief blip — the next refresh will re-validate.

**Dual-stack registration.** A registrant reachable on both IPv4 and
IPv6 SHOULD register a hostname with both A and AAAA records rather
than an IP literal. The literal-match check binds to a single address
family, so a peer connected via IPv6 advertising an IPv4 literal (or
vice versa) is rejected. The hostname path matches whichever family
the kernel routed the registration over.

**Address encoding.** When `address` is a hostname, it MAY be Unicode
(IDN) or Punycode. Trackers store the address as-typed and return it
unchanged in `TrackerServerListResponse`; the Punycode form is used
internally only for the resolution step in validation. Clients are
responsible for IDN → Punycode conversion at connection time, matching
the handling used for `ServerInfo.public_address` elsewhere in Nexus.

**Refresh semantics.** Every refresh re-sends the full structure; the
tracker replaces the stored entry in full. There is no per-field update or
delta protocol. `user_count` is typically the only field that changes
between refreshes.

**No deduplication.** Each open connection is one tracker entry. The tracker
does not merge entries by fingerprint, address, or any other field.
Fingerprint is metadata for client-side server verification, not a storage
key.

**Field length limits.** Tracker implementations MUST enforce the
following maximum lengths, which match the limits the BBS protocol
applies to analogous fields. The unit per field is listed in the
table below — some fields are measured in **characters** (Unicode
scalar values) so non-ASCII users aren't penalized
for their UTF-8 byte length; others are measured in **bytes** where
the underlying constraint is byte-based (DNS octet limits, ASCII-only
identifiers, opaque hashes). Values exceeding the limit are rejected
with `error_kind: invalid`.

| Field         | Max length     | Notes                                         |
| ------------- | -------------- | --------------------------------------------- |
| `name`        | 64 characters  |                                               |
| `description` | 512 characters |                                               |
| `password`    | 256 bytes      |                                               |
| `address`     | 253 bytes      | RFC 1035 DNS octet limit                      |
| `version`     | 32 bytes       |                                               |
| `locale`      | 16 bytes       |                                               |
| `fingerprint` | 95 bytes exact | Canonical form (see Fingerprint format above) |

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
  "version": "0.9.7",
  "fingerprint": "AA:BB:CC:DD:EE:FF:00:11:22:33:44:55:66:77:88:99:AA:BB:CC:DD:EE:FF:00:11:22:33:44:55:66:77:88:99",
  "user_count": 12,
  "allows_guest": true
}
```

### `TrackerServerRegisterResponse`

Sent by the tracker in reply to each `TrackerServerRegister`.

| Field              | Type    | Required   | Description                                                |
| ------------------ | ------- | ---------- | ---------------------------------------------------------- |
| `success`          | boolean | Yes        | Whether the registration was accepted                      |
| `refresh_interval` | u32     | If success | Seconds before sending the next `TrackerServerRegister`    |
| `error`            | string  | If failure | Human-readable explanation, localized per request `locale` |
| `error_kind`       | string  | If failure | Machine-readable error category (see [Errors](#errors))    |

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
malformed `TrackerServerRegister`, rate-limited, tracker at capacity.

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
- The tracker has not received a `TrackerServerRegister` within its stale-entry
  timeout (2× `refresh_interval` — e.g., 600 seconds at the recommended
  300s refresh).

The first two are immediate; the third is a backstop for half-open
connections that haven't been detected as broken at the TCP layer.

## Client Listing

A client connects to the tracker, sends a single `TrackerServerList` request,
receives the current set of registered servers in `TrackerServerListResponse`, and
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
   │  TrackerServerList { password?, version }     │
   │ ─────────────────────────────────────────►    │
   │                                               │
   │         TrackerServerListResponse {           │
   │           success, servers }                  │
   │ ◄─────────────────────────────────────────    │
   │                                               │
   │  ─────── Connection Closed ────────────       │
   │                                               │
```

### Messages

| Message                     | Direction        | Purpose                         |
| --------------------------- | ---------------- | ------------------------------- |
| `TrackerServerList`         | Client → Tracker | Request the current server list |
| `TrackerServerListResponse` | Tracker → Client | Return the server list or error |

The tracker closes the connection after sending `TrackerServerListResponse`,
regardless of success or failure. There is no keepalive, refresh, or
follow-up message defined for this flow.

### `TrackerServerList`

| Field      | Type   | Required | Description                                                           |
| ---------- | ------ | -------- | --------------------------------------------------------------------- |
| `password` | string | If gated | Listing password (omit if the tracker is open)                        |
| `locale`   | string | No       | BCP-47 language tag for localized text in responses (default: `"en"`) |
| `version`  | string | Yes      | Sender's `CARGO_PKG_VERSION` for compat filtering (semver)            |

`TrackerServerList` carries no filter, search, or pagination parameters in this
version of the protocol — but the tracker filters the response set to entries
the requesting client can actually speak with (see
[Compatibility filter](#compatibility-filter) below). Beyond that, clients
filter locally and may resort for views other than the default name ordering
(see [`TrackerServerListResponse`](#trackerserverlistresponse)).

**Example (open tracker):**

```json
{
  "locale": "en",
  "version": "0.9.7"
}
```

**Field validation.** `locale` and `version` use the same rules as
the BBS server applies elsewhere; rules are identical wherever those
fields are accepted in Nexus. `version`-specific rules and the
post-validation filter behavior are detailed below.

#### Compatibility filter

The tracker validates `version` and uses it to filter `servers` to entries
the requesting client can speak with. The same semver compatibility rule
the BBS handshake uses applies — same major; pre-1.0 same minor; post-1.0
client minor ≤ server minor; patch ignored.

- **Validation.** `version` is required. Empty / over-cap (32 bytes)
  / unparseable as semver → typed `TrackerServerListResponse` with
  `success: false`, `error_kind: "invalid"`. A _missing_ `version`
  field is a
  deserialization failure handled at the framing layer — the tracker
  emits a generic `Error` message and closes the connection, same as
  any required field on any message in the protocol. (List connections
  always close after one response — see
  [Client Listing](#client-listing) above — so the connection ends
  regardless of success or failure.)
- **Filtering.** On success, the tracker drops entries whose registered
  `version` is not `Compatible` with the client's `version`. Entries whose
  own `version` doesn't parse as semver are dropped silently with a
  tracker-side log; the registration-side validator should already reject
  those at register time, so this is defense-in-depth and is not expected
  to fire in normal operation.
- **No backstop on the client.** Trackers always filter; clients do not
  re-filter the returned set.

### `TrackerServerListResponse`

Sent by the tracker in reply to `TrackerServerList`.

| Field        | Type             | Required | Description                                                                                       |
| ------------ | ---------------- | -------- | ------------------------------------------------------------------------------------------------- |
| `success`    | boolean          | Yes      | Whether the listing request was accepted                                                          |
| `servers`    | Server Entry [ ] | Always   | Array of registered servers (see Server Entry below). Empty `[]` on the error path or no servers. |
| `error`      | string           | Failure  | Human-readable explanation, localized per request `locale`                                        |
| `error_kind` | string           | Failure  | Machine-readable error category (see [Errors](#errors))                                           |

**Empty list.** A tracker with zero registered servers responds with
`success: true` and `servers: []`. An empty list is not a failure.
The error path also returns `servers: []`; clients distinguish empty-
success from failure via the `success` field, not the `servers`
length.

**Listing size.** `TrackerServerListResponse` carries registered
servers in a single response. The per-message-type payload limit is
**32 MiB** — a defense-in-depth ceiling on client allocation and JSON
parse cost. The reference tracker sizes successful list responses by
actual serialized entry bytes after compatibility filtering and
truncates before this cap, so its default 10,000-entry registry fits
while larger custom registries remain wire-safe. Clients filter / sort
large lists locally (search box, column sort) so a large result set is
usable, not unwieldy.

**Ordering.** The tracker returns entries sorted alphabetically by `name`,
case-insensitive ascending. Clients may resort locally for other views,
but naive clients get a usable default presentation directly from the wire
order.

**Failure handling.** When `success` is false, the tracker closes the
connection after sending the response. Common rejection reasons: missing
or wrong password, malformed `TrackerServerList`, rate-limited.

### Server Entry

Each element of `TrackerServerListResponse.servers` has the following structure.
The fields mirror `TrackerServerRegister` with two differences: `password` is
never advertised, and `address` is always populated — either the registrant's
validated as-typed input (preserving any Unicode IDN form) or, when the
registration omitted `address`, the connecting-IP substitution.

| Field            | Type   | Required | Description                                                                 |
| ---------------- | ------ | -------- | --------------------------------------------------------------------------- |
| `name`           | string | Yes      | Server display name                                                         |
| `description`    | string | No       | Free-form description                                                       |
| `address`        | string | Yes      | Public hostname or IP (validated registrant input, or peer-IP substitution) |
| `port`           | u16    | Yes      | BBS TCP port                                                                |
| `websocket_port` | u16    | No       | BBS WebSocket port (if the server has `--websocket`)                        |
| `version`        | string | Yes      | Server software version (e.g., `"0.9.7"`)                                   |
| `fingerprint`    | string | Yes      | TLS cert fingerprint, canonical form                                        |
| `user_count`     | u32    | Yes      | Distinct online users (matches the user list)                               |
| `allows_guest`   | bool   | Yes      | Whether the guest account is enabled                                        |

**Trust note.** The listed `fingerprint` is a display aid, not a trust
assertion. Clients SHOULD perform their own TOFU fingerprint check on
first connect to a server discovered via a tracker.

**`TrackerServerListResponse` success example:**

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
      "version": "0.9.7",
      "fingerprint": "AA:BB:CC:DD:EE:FF:00:11:22:33:44:55:66:77:88:99:AA:BB:CC:DD:EE:FF:00:11:22:33:44:55:66:77:88:99",
      "user_count": 12,
      "allows_guest": true
    },
    {
      "name": "Other BBS",
      "address": "other.example.com",
      "port": 7500,
      "version": "0.9.7",
      "fingerprint": "11:22:33:44:55:66:77:88:99:AA:BB:CC:DD:EE:FF:00:11:22:33:44:55:66:77:88:99:AA:BB:CC:DD:EE:FF:00",
      "user_count": 3,
      "allows_guest": false
    }
  ]
}
```

**`TrackerServerListResponse` failure example:**

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
   returned via `TrackerServerRegisterResponse { success: false, error }` or
   `TrackerServerListResponse { success: false, error }`.
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
and the `message` field in `Error` — are pre-localized to the `locale`
provided in the request before being sent on the wire. Clients receive
pre-localized strings and SHOULD display them directly.

When the tracker sends `Error` before the request's `locale` is known —
for example, on a frame format violation that prevents `TrackerServerRegister`
or `TrackerServerList` from being parsed — the message is rendered in
the implementation's default locale (English).

### `Error`

Sent by the tracker when it must abort the connection without a typed flow
response. Always followed by an immediate disconnect.

| Field     | Type   | Required | Description                                           |
| --------- | ------ | -------- | ----------------------------------------------------- |
| `message` | string | Yes      | Human-readable description of the violation           |
| `command` | string | No       | The offending message type, if available from context |

The `Error` frame echoes the `message_id` of the offending frame when one
was received; otherwise the tracker generates a fresh ID.

**Example (role violation):**

```json
{
  "message": "Role violation: connection is in client mode",
  "command": "TrackerServerRegister"
}
```

### Role Locking

The first valid post-handshake message determines the connection's role
and locks it in for the lifetime of the connection:

- A connection whose first post-handshake message was `TrackerServerRegister` is
  a **server connection**. Only subsequent `TrackerServerRegister` refreshes are
  valid on it.
- A connection whose first post-handshake message was `TrackerServerList` is a
  **client connection**. The tracker responds with `TrackerServerListResponse`
  and closes the connection; no further messages are expected.

Sending `TrackerServerList` on a server connection, or `TrackerServerRegister` on a
client connection, is a role violation. The tracker responds with `Error`
and disconnects.

A first post-handshake message that is neither `TrackerServerRegister` nor
`TrackerServerList` — for example, a BBS-port message — receives an `Error` and
disconnect.

### Failure Conditions

| Condition                                                        | Tracker Response                                 | `error_kind`   |
| ---------------------------------------------------------------- | ------------------------------------------------ | -------------- |
| Frame format violation (bad magic, bad framing)                  | `Error`, disconnect                              | —              |
| Payload exceeds per-message-type limit                           | `Error`, disconnect                              | —              |
| Unknown message type                                             | `Error`, disconnect                              | —              |
| Known message type sent in the wrong protocol phase or direction | `Error`, disconnect                              | —              |
| Malformed JSON in a known message                                | `Error`, disconnect                              | —              |
| Role violation                                                   | `Error`, disconnect                              | —              |
| Missing or wrong password                                        | Typed response with `success: false`, disconnect | `unauthorized` |
| Field validation failure                                         | Typed response with `success: false`, disconnect | `invalid`      |
| Rate-limited                                                     | Typed response with `success: false`, disconnect | `rate_limited` |
| Tracker at capacity                                              | Typed response with `success: false`, disconnect | `capacity`     |

The generic `Error` message does not carry an `error_kind` — its
violation space is exclusively protocol-level, and clients do not branch
on it for happy-path logic.

**Field-validation sub-categories.** A typed response with
`error_kind: "invalid"` covers every field-level validation failure,
including the address-validation step described in
[Address validation](#address-validation).
The reference tracker emits a structured `reason` field in its rejection
log — `address_loopback`, `address_hostname_no_match`,
`address_hostname_dns_failed`, etc. — which operators can use to
distinguish sub-cases without parsing the human-readable `error` string.

## Timeouts

The tracker enforces low-level timeouts to bound resource use from
unauthenticated and half-broken connections.

| Phase                                                    | Timeout                                                                   | Behavior on expiry                |
| -------------------------------------------------------- | ------------------------------------------------------------------------- | --------------------------------- |
| TLS accepted, awaiting `Handshake`                       | 30 seconds                                                                | Disconnect, no response           |
| Frame completion (any frame, mid-read)                   | 60 seconds                                                                | `Error` (best-effort), disconnect |
| Awaiting first role-establishing message after handshake | 30 seconds                                                                | `Error`, disconnect               |
| Response write progress                                  | 60 seconds per chunk                                                      | Disconnect                        |
| Server connection idle between refreshes (stale entry)   | 2× `refresh_interval` (e.g., 600 seconds at the recommended 300s refresh) | Disconnect, delist (no response)  |

The first three read-side timeouts mirror the BBS port and protect
against slowloris and resource-holding attacks. Response writes use a
progress timeout rather than a whole-frame deadline, so slow clients can
receive large listings as long as each chunk continues to drain. The
stale-entry timeout is the dominant liveness discipline for long-lived
server registrations; client connections close promptly after the
response and have no idle phase to time out.

## Security and Privacy

Trackers are a public discovery service. Operators and users should
understand the following properties before deciding to host or use one.

### Opt-In Visibility

A Nexus server is never listed without an explicit registration. Defaults
favor invisibility: a server with no trackers configured advertises
nothing and can only be reached by people who already know its address.

### Public Listing Contents

Every field in a `TrackerServerRegister` is visible to anyone with listing
access. There is no per-entry visibility control and no encryption of
individual entries; a listing password gates access to the whole list at
once.

### Passwords Are Coarse Gates

Both passwords are access gates, not identity or privacy guarantees:

- A **registration password** restricts who can submit entries. It does
  not authenticate which server is registering — anyone who knows the
  password can register any `name` or `fingerprint`. The `address`
  field is bound to the registrant's source IP via the address-
  validation step (see
  [Address validation](#address-validation)),
  so a remote attacker can't claim arbitrary network endpoints — but a
  LAN-coresident attacker can, by virtue of the LAN-peer bypass.
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

- Failed authentication attempts per IPv4 address or IPv6 `/64`, to deter brute-force guessing.
- Connection rate per IPv4 address or IPv6 `/64`, to bound resource usage.
- `TrackerServerList` requests per IPv4 address or IPv6 `/64`, separately from connection rate, to
  deter scraping.

The protocol does not prescribe specific limits. Trackers are free to
respond with typed-response rate-limit errors or to drop connections at
the framing layer.

**Reference implementation.** Two token-bucket limiters keyed by IPv4
address or IPv6 `/64`, plus a per-entry refresh floor:

- **Connection rate** (`--rate-connections`, default `20`/min): bucket
  drained at TCP accept; over-limit peers have their connection dropped
  silently at the framing layer.
- **Failed auth attempts** (`--rate-auth-failures`, default `5`/min):
  successful authentications don't debit the bucket; only failed
  password verifications do. Once the bucket is empty, further attempts
  from that IPv4 address or IPv6 `/64` — including correct passwords — are rejected with
  `error_kind: rate_limited` until the bucket refills, so an attacker
  who triggered the limit can't sneak through with a guess.
- **Refresh floor** (60s, hardcoded): a registered server's
  `TrackerServerRegister` refreshes are rejected with
  `error_kind: rate_limited` _and the connection is closed_ if they
  arrive less than 60 seconds after the previous accepted refresh.
  Half the protocol-level minimum `refresh_interval` (120s), so any
  well-behaved server is well clear — hitting this floor means the
  client is going at least 2× too fast, which is broken or malicious
  rather than over-eager. The drop guard unregisters the entry on
  disconnect. The floor is checked _before_ password verification so
  a misbehaving long-lived connection can't pin CPU on Argon2 hashing.

A separate `TrackerServerList`-rate limiter is _not_ implemented in
v0.1.0. List requests are one-shot per connection in this protocol, so
the connection-rate limiter already bounds list-scraping at the same
rate. A dedicated list limiter would only matter if it were stricter
than the connection limiter.

### Per-Source-IP Entry Cap

Rate limits cap _frequency_ and the global capacity setting caps
_total_ entries, but neither prevents a single operator from quietly
filling a sizable share of the listing one slow refresh at a time.
Trackers SHOULD cap the number of _currently-registered_ entries per
source IP.

The reference implementation defaults this cap to **1**: one IP, one
listed server. The reasoning is that during a hard-crash → reconnect
window the old entry briefly survives in the registry until stale
eviction; with the cap at 1, the listing shows the still-correct
`address` / `port` / `fingerprint` with a slightly outdated
`user_count` for that window — a milder failure than the duplicate
entries cap=2 would produce. Operators on shared NAT'd networks
(office, household, school) where multiple distinct operators
register from the same egress IP can raise the cap explicitly.

When the cap is hit, the tracker responds with a typed
`TrackerServerRegisterResponse { success: false, error_kind: "capacity" }`.
The same `error_kind` is used for both the global and per-IP caps;
the human-readable `error` message is what distinguishes them.

A coordinated multi-IP attacker still wins against this single
control — they can rent IPs cheaply enough to exhaust whatever
registration password the tracker has set. The per-IP cap is a
**price-raising measure**, not a hard barrier; gated trackers and
operator-side moderation are the controls that handle deliberate
abuse.

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
