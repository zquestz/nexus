# URI Scheme

Nexus supports the `nexus://` URI scheme for deep linking to servers and resources.

## Format

```
nexus://[user[:password]@]host[:port][/path]
```

| Component | Required | Description |
|-----------|----------|-------------|
| `user` | No | Username for authentication |
| `password` | No | Password (only valid with user) |
| `host` | Yes | Server hostname or IP address (IPv6 in brackets) |
| `port` | No | Server port (default: 7500) |
| `path` | No | Resource path (intent) |

## Connection Examples

| URI | Behavior |
|-----|----------|
| `nexus://server.com` | Connect using matching bookmark credentials, or guest login |
| `nexus://server.com:8500` | Connect to custom port |
| `nexus://[::1]:7500` | Connect to IPv6 address |
| `nexus://alice@server.com` | Connect as alice (uses bookmark password if saved) |
| `nexus://shared:pass@server.com` | Connect with explicit credentials |

## Path Intents

Paths specify what to open after connecting. They are intents, not commands — if already at the destination, the client focuses it.

| Path | Intent |
|------|--------|
| (none) | Connect only |
| `/chat` | Focus chat panel (no tab change) |
| `/chat/#general` | Join/focus #general channel |
| `/chat/alice` | Open/focus user message tab with alice |
| `/files` | Open Files panel |
| `/files/Music` | Open Files panel to Music folder |
| `/files/Music/song.mp3` | Navigate to Music folder and download song.mp3 |
| `/news` | Open News panel |
| `/info` | Open Server Info panel |

### Path Details

- `/chat/#name` — `#` prefix indicates channel (case insensitive)
- `/chat/name` — no `#` prefix indicates user message tab
- `/files/path` — navigates to parent directory, then handles target (file or folder)
- Path components are URL-decoded (e.g., `%20` → space)
- Invalid paths or insufficient permissions show error in console

## Connection Matching

When processing a URI, the client determines whether to reuse an existing connection or create a new one:

| URI Pattern | Behavior |
|-------------|----------|
| `nexus://server.com/...` | Reuse any existing connection to host:port, or connect using bookmark/guest |
| `nexus://user@server.com/...` | Reuse connection with matching host:port AND username |
| `nexus://user:pass@server.com/...` | Reuse or create connection with those credentials |

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

## Linkification

The `nexus://` scheme is recognized in chat messages and displayed as clickable links, similar to `http://` and `https://` URLs.

When clicked:
- `nexus://` links navigate internally (handled by the client)
- Other URLs open in the system browser

News posts render links via Markdown syntax.

## Single Instance

### macOS — Apple Events

On macOS, the OS handles single-instance routing natively for URL scheme clicks. When a user clicks a `nexus://` link in a browser or Finder:

1. macOS delivers the URL via Apple Events (`application:openURLs:`)
2. If the app is not running, macOS launches it and delivers the event after initialization
3. If the app is already running, macOS activates it and delivers the event immediately

The client registers a custom `NSApplicationDelegate` to receive these events and forwards URLs to the Iced event loop via a channel.

IPC (below) is still used on macOS for CLI invocations (e.g., `nexus "nexus://..."`).

### IPC (All Platforms)

When a `nexus://` URI is opened via command line and Nexus is already running, the URI is passed to the existing instance via IPC:

1. New instance attempts to connect to IPC socket/pipe
2. If successful: sends URI, waits for acknowledgment, exits
3. If no existing instance: becomes primary, creates IPC listener

### IPC Socket Paths

| Platform | Path |
|----------|------|
| Linux | `$XDG_RUNTIME_DIR/nexus.sock` |
| macOS | `$TMPDIR/nexus.sock` |
| Linux fallback | `/tmp/nexus-{username}.sock` |
| Windows | Named pipe `nexus-{username}` |

On Linux and macOS, the socket lives inside a per-user directory (`XDG_RUNTIME_DIR`, `TMPDIR`), providing user isolation without a username suffix. The `/tmp` fallback and Windows named pipe include `{username}` explicitly.

### IPC Protocol

1. Client sends URI as UTF-8 line (terminated with `\n`)
2. Server sends acknowledgment line
3. Connection closes

Timeout: 5 seconds (Unix only)

## OS Protocol Registration

Nexus registers as a handler for the `nexus://` scheme via cargo-bundle metadata:

| Platform | Method | Status |
|----------|--------|--------|
| Linux | `linux_mime_types = ["x-scheme-handler/nexus"]` | ✅ |
| macOS | `osx_url_schemes = ["nexus"]` | ✅ |
| Windows | Registry entries in MSI | ❌ TODO |

**Windows TODO:** `cargo-bundle` doesn't support `windows_url_schemes`. Need to either fork cargo-bundle to add support, or add registry entries directly in the MSI build workflow.

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

## Error Handling

| Condition | Behavior |
|-----------|----------|
| Invalid URI format | Parse error, not processed |
| Connection failed | Error shown in current console or connection form |
| Channel join failed | Server error shown in console |
| File not found | Server error shown in console |
| Permission denied | Server error shown in console |

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