# URI Scheme

Nexus supports the `nexus://` URI scheme for deep linking to servers and resources.

## Format

```
nexus://[user[:password]@]host[:port][/path]
```

| Component  | Required | Description                                      |
| ---------- | -------- | ------------------------------------------------ |
| `user`     | No       | Username for authentication                      |
| `password` | No       | Password (only valid with user)                  |
| `host`     | Yes      | Server hostname or IP address (IPv6 in brackets) |
| `port`     | No       | Server port (default: 7500)                      |
| `path`     | No       | Resource path (intent)                           |

## WebSocket Variant

WebSocket-based Nexus clients use the `nexus-ws://` scheme instead of
`nexus://`. The two schemes share identical syntax, path intents, credential
resolution, connection matching rules, and URL encoding — they differ only
in transport:

| Scheme        | Transport | Default Port |
| ------------- | --------- | ------------ |
| `nexus://`    | TCP       | 7500         |
| `nexus-ws://` | WebSocket | 7502         |

A TCP session and a WebSocket session to the same `host:port` are distinct
connections; clients that support both schemes match them separately.

The reference Nexus client is TCP-only and does not handle `nexus-ws://`
URIs. This section exists so WebSocket-capable clients have a documented
URI form to use and share.

## Connection Examples

| URI                              | Behavior                                                    |
| -------------------------------- | ----------------------------------------------------------- |
| `nexus://server.com`             | Connect using matching bookmark credentials, or guest login |
| `nexus://server.com:8500`        | Connect to custom port                                      |
| `nexus://[::1]:7500`             | Connect to IPv6 address                                     |
| `nexus://alice@server.com`       | Connect as alice (uses bookmark password if saved)          |
| `nexus://shared:pass@server.com` | Connect with explicit credentials                           |

## Path Intents

Paths specify what to open after connecting. They are intents, not commands — if already at the destination, the client focuses it.

| Path                    | Intent                                         |
| ----------------------- | ---------------------------------------------- |
| (none)                  | Connect only                                   |
| `/chat`                 | Focus chat panel (no tab change)               |
| `/chat/#general`        | Join/focus #general channel                    |
| `/chat/alice`           | Open/focus user message tab with alice         |
| `/files`                | Open Files panel                               |
| `/files/Music`          | Open Files panel to Music folder               |
| `/files/Music/song.mp3` | Navigate to Music folder and download song.mp3 |
| `/news`                 | Open News panel                                |
| `/info`                 | Open Server Info panel                         |

### Path Details

- `/chat/#name` — `#` prefix indicates channel (case insensitive)
- `/chat/name` — no `#` prefix indicates user message tab
- `/files/path` — navigates to parent directory, then handles target (file or folder)
- Path components are URL-decoded (e.g., `%20` → space)
- Invalid paths or insufficient permissions show error in console

## Connection Matching

When processing a URI, the client determines whether to reuse an existing connection or create a new one:

| URI Pattern                        | Behavior                                                                    |
| ---------------------------------- | --------------------------------------------------------------------------- |
| `nexus://server.com/...`           | Reuse any existing connection to host:port, or connect using bookmark/guest |
| `nexus://user@server.com/...`      | Reuse connection with matching host:port AND username                       |
| `nexus://user:pass@server.com/...` | Reuse or create connection with those credentials                           |

Matching is case-insensitive for host and username.

## Credential Resolution

When connecting from a URI, the client resolves credentials in this order:

### URI without credentials (`nexus://server.com`)

1. Find bookmark matching host:port
2. If found: use bookmark's username, password, nickname
3. If not found: guest login with client's default nickname

### URI with username only (`nexus://alice@server.com`)

1. Find bookmark matching host:port AND username
2. If found: use bookmark's password and nickname
3. If not found: use username with empty password

### URI with full credentials (`nexus://alice:secret@server.com`)

1. Find bookmark matching host:port AND username
2. If found: use URI password (overrides bookmark), use bookmark's nickname
3. If not found: use URI credentials with client's default nickname

## Client Behavior

- **Nickname**: Comes from bookmark or client settings, never from URI
- **Transport**: Always TCP (not WebSocket)
- **Locale**: Uses client's configured locale
- **Avatar**: Uses client's configured avatar
- **Proxy**: Uses client's proxy settings if enabled
- **IDN**: Unicode hostnames (IDN) are accepted and encoded to Punycode before DNS resolution; the client preserves the displayed form in the URI

## Shareable URI Host

When a client builds a shareable `nexus://` URI (the URI shown in the Server Info panel, or the link generated by the "Share" action on a file), it picks the host as follows:

1. **If `ServerInfo.public_address` is set**, use it verbatim. This is the admin-advertised hostname or IP — preferred because it's the operator's canonical public form and survives the user having connected via IP, LAN hostname, or a proxy.
2. **Otherwise**, fall back to the address the user is currently connected to (`connection_info.address`).

The port is the server's BBS port; it is omitted from the URI when it equals the default (`7500`). IPv6 hosts are bracketed at render time. Share URIs are credential-less — no `user[:password]@` is included.

## Linkification

The `nexus://` scheme is recognized in chat messages and displayed as clickable links, similar to `http://` and `https://` URLs.

When clicked:

- `nexus://` links navigate internally (handled by the client)
- `http://`, `https://`, `ftp://`, `ftps://`, `sftp://`, and
  `mailto:` links open in the system browser or registered OS handler
- Other schemes are ignored

News posts render links via Markdown syntax.

## Single Instance

Single-instance is always enforced. Every launch of the client checks for an existing instance via IPC, regardless of whether a `nexus://` URI is present.

### macOS — Apple Events

On macOS, the OS handles single-instance routing natively for URL scheme clicks. When a user clicks a `nexus://` link in a browser or Finder:

1. macOS delivers the URL via Apple Events (`kInternetEventClass` / `kAEGetURL`)
2. If the app is not running, macOS launches it and delivers the event after initialization
3. If the app is already running, macOS activates it and delivers the event immediately

Clients register a handler with `NSAppleEventManager` to receive these events and route them into the client's UI event loop.

IPC (below) is still used on macOS for CLI invocations (e.g., `nexus "nexus://..."`).

### IPC (All Platforms)

On every launch, the client attempts to connect to the IPC socket/pipe:

1. New instance attempts to connect to IPC socket/pipe
2. If successful: sends URI or `FOCUS` command, waits for acknowledgment, exits
3. If no existing instance: becomes primary, creates IPC listener

If the existing instance's window is hidden to the system tray, it is restored and focused.

### IPC Socket Paths

| Platform       | Path                          |
| -------------- | ----------------------------- |
| Linux          | `$XDG_RUNTIME_DIR/nexus.sock` |
| macOS          | `$TMPDIR/nexus.sock`          |
| Linux fallback | `/tmp/nexus-{username}.sock`  |
| Windows        | Named pipe `nexus-{username}` |

On Linux and macOS, the socket lives inside a per-user directory (`XDG_RUNTIME_DIR`, `TMPDIR`), providing user isolation without a username suffix. The `/tmp` fallback and Windows named pipe include `{username}` explicitly.

### IPC Protocol

1. Client sends message as UTF-8 line (terminated with `\n`):
   - A `nexus://` URI to open, **or**
   - `FOCUS` to bring the existing instance's window to the front
2. Server sends acknowledgment line (`ok\n`)
3. Connection closes

Timeout: 5 seconds (Unix only)

## OS Protocol Registration

Clients register themselves as the handler for the `nexus://` scheme
through the platform's standard URL-scheme / MIME-type mechanism:

| Platform | Registration target                |
| -------- | ---------------------------------- |
| Linux    | MIME type `x-scheme-handler/nexus` |
| macOS    | URL scheme `nexus`                 |
| Windows  | URL scheme `nexus`                 |

### Desktop File (Linux)

The `.desktop` file includes:

```ini
MimeType=x-scheme-handler/nexus;
Exec=nexus %u
```

The `%u` placeholder is replaced with the URI by the desktop environment.

## URL Encoding

Standard percent-encoding applies:

- Host: not encoded
- User/password: special characters encoded (`:`, `@`, `/`, etc.)
- Path: special characters encoded

The client decodes these when parsing.

## Security Considerations

- **Passwords in URIs**: Only use for shared account invites where credentials are intentionally public
- **Private accounts**: Use `user@host` format; client will use saved bookmark password
- **No command execution**: URIs only navigate; they cannot execute commands or modify settings
- **Bookmark isolation**: URI credentials don't modify saved bookmarks
- **`public_address` trust**: The admin-advertised `public_address` is displayed to all connected users as the shareable URI host. Clients should treat it the same as any other server-advertised metadata (name, description) — it's under operator control and is not independently verified against the address the user connected to

## Error Handling

| Condition           | Behavior                                          |
| ------------------- | ------------------------------------------------- |
| Invalid URI format  | Parse error, not processed                        |
| Connection failed   | Error shown in current console or connection form |
| Channel join failed | Server error shown in console                     |
| File not found      | Server error shown in console                     |
| Permission denied   | Server error shown in console                     |

## Examples

### Invite to a channel

```
nexus://shared:welcome@bbs.example.com/chat/#lobby
```

Connects with shared account credentials and joins #lobby.

### Link to a file

```
nexus://bbs.example.com/files/Public/readme.txt
```

Uses existing connection or bookmark, opens Files panel to the Public folder and downloads readme.txt.

### Simple server link

```
nexus://bbs.example.com
```

Connects using matching bookmark or guest login.

## IANA Registration

The `nexus://` scheme is not registered with IANA. It is a custom scheme used exclusively by Nexus BBS clients.
