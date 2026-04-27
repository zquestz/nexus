# Handshake

The handshake is the first message exchange after TLS connection. It establishes protocol version compatibility before authentication.

## Flow

```
Client                                        Server
   │                                             │
   │  ─────── TLS Connection ──────────────►     │
   │     (client observes cert fingerprint)      │
   │                                             │
   │  Handshake { version }                      │
   │ ───────────────────────────────────────►    │
   │                                             │
   │  HandshakeResponse { version, fingerprint } │
   │ ◄───────────────────────────────────────    │
   │                                             │
   │  (client verifies server-reported           │
   │   fingerprint matches TLS-observed)         │
   │                                             │
```

## Messages

### Handshake (Client → Server)

Sent immediately after TLS connection is established.

| Field     | Type   | Required | Description                                 |
| --------- | ------ | -------- | ------------------------------------------- |
| `version` | string | Yes      | Client's protocol version (e.g., `"0.8.0"`) |

**Example:**

```json
{
  "version": "0.8.0"
}
```

**Full frame:**

```
NX|9|Handshake|a1b2c3d4e5f6|20|{"version":"0.8.0"}
```

### HandshakeResponse (Server → Client)

Server's response indicating whether the handshake succeeded.

| Field         | Type    | Required   | Description                                                         |
| ------------- | ------- | ---------- | ------------------------------------------------------------------- |
| `success`     | boolean | Yes        | Whether the handshake succeeded                                     |
| `version`     | string  | If success | Server's protocol version                                           |
| `fingerprint` | string  | Yes        | Server's TLS certificate fingerprint (SHA-256, colon-separated hex) |
| `error`       | string  | If failure | Error message explaining the failure                                |

The `fingerprint` field is sent on **every** response — both success and failure — so the client can detect TLS interception even when the handshake itself errors out (e.g., a MITM forging a "version mismatch" response). The client compares this server-reported value to the TLS-observed fingerprint before trusting the connection. See [Connection Flow](README.md#connection-flow) for details.

**Success example:**

```json
{
  "success": true,
  "version": "0.8.0",
  "fingerprint": "AA:BB:CC:DD:EE:FF:00:11:22:33:44:55:66:77:88:99:AA:BB:CC:DD:EE:FF:00:11:22:33:44:55:66:77:88:99"
}
```

**Failure example:**

```json
{
  "success": false,
  "version": "0.8.0",
  "fingerprint": "AA:BB:CC:DD:EE:FF:00:11:22:33:44:55:66:77:88:99:AA:BB:CC:DD:EE:FF:00:11:22:33:44:55:66:77:88:99",
  "error": "Unsupported protocol version. Server: 0.8.0, Client: 0.3.0"
}
```

## Version Compatibility

The protocol uses [Semantic Versioning](https://semver.org/) for compatibility checks:

| Component | Rule                                                                   |
| --------- | ---------------------------------------------------------------------- |
| Major     | Must match exactly                                                     |
| Minor     | **Pre-1.0 (`0.x`):** Must match exactly. **Post-1.0:** Client ≤ Server |
| Patch     | Ignored                                                                |

During pre-1.0 development, each minor version bump can introduce breaking protocol changes (per semver convention). After 1.0, minor bumps are backward-compatible and only major bumps break compatibility.

**Examples (pre-1.0):**

| Client | Server | Compatible | Reason                   |
| ------ | ------ | ---------- | ------------------------ |
| 0.8.0  | 0.8.0  | ✅ Yes     | Exact match              |
| 0.8.0  | 0.8.5  | ✅ Yes     | Patch difference ignored |
| 0.7.0  | 0.8.0  | ❌ No      | Minor mismatch (pre-1.0) |
| 0.8.0  | 0.7.0  | ❌ No      | Minor mismatch (pre-1.0) |
| 1.0.0  | 0.8.0  | ❌ No      | Major version mismatch   |
| 0.8.0  | 1.0.0  | ❌ No      | Major version mismatch   |

## Error Handling

If the handshake fails:

1. Server sends `HandshakeResponse` with `success: false` and an `error` message
2. Server closes the connection
3. Client should display the error to the user

Common errors:

| Error                        | Cause                               |
| ---------------------------- | ----------------------------------- |
| Unsupported protocol version | Version incompatibility             |
| Invalid handshake            | Malformed message or missing fields |

## Timeout

The server expects the `Handshake` message within 30 seconds of TLS connection. If not received, the connection is closed.

Once handshake and login complete, authenticated users can idle indefinitely. The 30-second timeout only applies to unauthenticated connections.

**Timeout behavior:**

| State        | First Byte Timeout        | Frame Completion Timeout |
| ------------ | ------------------------- | ------------------------ |
| Before login | 30 seconds                | 60 seconds               |
| After login  | Indefinite (idle allowed) | 60 seconds               |

This prevents resource exhaustion from unauthenticated connections while allowing legitimate users to idle in chat.

## Notes

- The handshake must be the first message after TLS connection
- No other messages can be sent until handshake completes successfully
- After successful handshake, the client must send `Login` to authenticate
- The same handshake flow is used on both port 7500 (BBS) and port 7501 (transfers)

## Next Step

After a successful handshake, proceed to [Login](02-login.md).
