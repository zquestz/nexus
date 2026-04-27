# Connections

This guide covers managing server connections, bookmarks, and proxy configuration.

## Bookmarks

Bookmarks save server connection details for quick access. Each bookmark stores:

- Server name (display name)
- Server address and port
- Username and password (optional)
- Nickname (for shared/guest accounts)
- Certificate fingerprint (saved automatically)
- Auto-connect setting

### Creating Bookmarks

**Method 1: While connecting**

1. Fill out the connection form
2. Check **Add to bookmarks**
3. Click **Connect**

**Method 2: From the server list**

1. Click the bookmark icon in the server list header
2. Fill out the server details
3. Click **Save**

### Editing Bookmarks

1. Hover over the bookmark in the server list
2. Click the gear icon
3. Modify the details
4. Click **Save**

### Deleting Bookmarks

1. Hover over the bookmark in the server list
2. Click the gear icon
3. Click **Delete**

### Bookmark Order

Bookmarks are sorted alphabetically by name.

## Auto-Connect

Auto-connect automatically connects to selected servers when Nexus starts.

### Enabling Auto-Connect

1. Edit the bookmark (click the gear icon)
2. Enable **Auto-connect**
3. Click **Save**

Multiple bookmarks can have auto-connect enabled — Nexus will connect to all of them on startup.

### Disabling Auto-Connect

1. Edit the bookmark
2. Disable **Auto-connect**
3. Click **Save**

## Multiple Connections

Nexus supports connecting to multiple servers simultaneously:

- Each connection appears in the server list
- Click a connection to switch to it
- The active connection is highlighted
- Chat tabs and panels are per-connection

## Certificate Management

Nexus verifies the server's TLS certificate fingerprint **in two stages**, both of which run before your password is sent. If either stage fails, the connection aborts before any credentials leave your machine.

**Stage 1 — Trust On First Use (TOFU):** As soon as the TLS connection is established, the client compares the observed certificate fingerprint to the value stored in your bookmark.

- **First connection (no stored fingerprint yet):** stage 1 has nothing to compare against and is skipped. Stage 2 still runs.
- **Stored fingerprint matches:** stage 1 passes silently and the handshake proceeds.
- **Stored fingerprint differs:** the connection is dropped immediately and a mismatch dialog appears (see below).

**Stage 2 — TLS Interception Detection:** After the protocol handshake, the server self-reports its fingerprint. The client compares it to the TLS-observed value.

- **Match:** the connection proceeds to login. If this was a first-time connection, the bookmark commits the fingerprint now (TOFU save) — only after the server has confirmed it agrees with itself.
- **Mismatch:** active interception is happening. The connection aborts with an informational warning dialog. There is no accept path; credentials are never sent.

### Accepting a New Certificate (stage 1 mismatch)

If a server's certificate changes (e.g., after server reinstall) and the stored bookmark fingerprint no longer matches:

1. A fingerprint mismatch dialog appears showing the expected and received values.
2. Verify with the server operator out-of-band that the change is legitimate.
3. Click **Accept** to update the bookmark and reconnect, or **Cancel** to abandon the attempt.

When you click Accept, Nexus does the following automatically:

- The new fingerprint replaces the old one in the bookmark.
- Your encrypted chat history files are re-keyed for the new fingerprint.
- Queued and in-flight transfers for that bookmark pick up the new fingerprint.
- A fresh connection attempt is dispatched preserving your **original intent** — whether you launched from a bookmark, the manual connect form, or a `nexus://` URI (URI path navigation is preserved across the retry).

If stage 2 also fails on the retry, the interception dialog appears and the connection is denied — the bookmark is **not** modified in this case.

### Why no accept path for stage 2?

Stage 2 mismatch means the server is actively presenting a different certificate over TLS than it self-reports over the protocol. The most plausible cause is an attacker terminating TLS between you and the real server. Accepting that situation would defeat the purpose of the check, so the dialog is informational only and the connection is denied. Investigate the network path (corporate proxy, captive portal, compromised router) before retrying.

## Proxy Support

Route connections through a SOCKS5 proxy (e.g., Tor, SSH tunnel).

### Configuring a Proxy

1. Open **Settings** (gear icon in toolbar)
2. Go to the **Network** tab
3. Enable **Use proxy**
4. Enter the proxy address (default: `127.0.0.1`)
5. Enter the proxy port (default: `9050` for Tor)
6. Optionally enter username and password for authentication
7. Click **Save**

### Default Proxy Settings

| Setting  | Default     | Description                     |
| -------- | ----------- | ------------------------------- |
| Address  | `127.0.0.1` | Proxy server hostname or IP     |
| Port     | `9050`      | Proxy server port (Tor default) |
| Username | (empty)     | Optional authentication         |
| Password | (empty)     | Optional authentication         |

### Proxy Bypass

Some addresses automatically bypass the proxy:

- **Localhost**: `127.0.0.1`, `::1`, `localhost`
- **Private/LAN**: `10.0.0.0/8`, `172.16.0.0/12`, `192.168.0.0/16` (RFC 1918)
- **IPv6 ULA**: `fc00::/7` (Unique Local Addresses)
- **Yggdrasil**: Addresses in the `0200::/7` range

This ensures local connections, LAN servers, and Yggdrasil mesh traffic are not routed through the proxy.

**Limitation:** Voice chat requires a direct UDP connection and cannot be routed through a SOCKS5 proxy. If you connect via proxy, voice chat will be unavailable on that connection.

### Using with Tor

To route Nexus traffic through Tor:

1. Install and start the Tor service
2. In Nexus Settings > Network, enable proxy
3. Use address `127.0.0.1` and port `9050` (default Tor SOCKS port)
4. Connect to servers using their `.onion` addresses or regular addresses

Note: The server operator must also be reachable through Tor for this to work.

## URI Links

Nexus supports `nexus://` URIs for deep linking to servers and resources. Click a link in chat, email, or a web page to connect directly.

### URI Format

```
nexus://[user[:password]@]host[:port][/path]
```

### Examples

| URI                                | Action                                             |
| ---------------------------------- | -------------------------------------------------- |
| `nexus://server.com`               | Connect as guest                                   |
| `nexus://server.com:8500`          | Connect to custom port                             |
| `nexus://alice@server.com`         | Connect as alice (uses bookmark password if saved) |
| `nexus://shared:pass@server.com`   | Connect with shared account credentials            |
| `nexus://server.com/chat`          | Connect and focus chat panel                       |
| `nexus://server.com/chat/#general` | Connect and join #general channel                  |
| `nexus://server.com/chat/alice`    | Connect and open user message tab with alice       |
| `nexus://server.com/files/Music`   | Connect and open Files to Music folder             |
| `nexus://server.com/news`          | Connect and open News panel                        |
| `nexus://server.com/info`          | Connect and open Server Info panel                 |

### Connection Behavior

- **Existing connection**: If already connected to the server, Nexus switches to that connection and navigates to the path
- **No credentials in URI**: Looks for a matching bookmark, otherwise connects as guest
- **Username without password**: Looks for matching bookmark to get saved password
- **Full credentials**: Uses the provided username and password (intended for shared accounts)

### Shareable URIs from the Server Info panel

When you copy a `nexus://` URI from the Server Info panel or the "Share" action on a file, the host comes from the admin-advertised **Public Address** if the server has one set. Otherwise it falls back to the address you used to connect. This means the URI you share may differ from the address you typed — especially if you connected via IP or a LAN hostname while the server advertises a public DNS name.

### Command Line

Launch Nexus with a URI to connect on startup:

```bash
nexus "nexus://server.com/chat/#general"
```

If Nexus is already running, the URI is sent to the existing instance.

## Connection Troubleshooting

### Connection Refused

- Verify the server address and port are correct
- Ensure the server is running
- Check firewall settings on both client and server

### Certificate Errors

- If you see a fingerprint mismatch, verify with the server operator
- Click **Accept** to save the new fingerprint, or **Cancel** to disconnect

### Proxy Errors

- Verify the proxy server is running
- Check the proxy address and port
- If using authentication, verify credentials
- Try disabling the proxy to test direct connectivity

### Timeout Errors

- Check your network connection
- The server may be overloaded or unreachable
- If using a proxy, verify the proxy has network access

## Next Steps

- [Chat](03-chat.md) — Channels, user messages, and mentions
- [Settings](11-settings.md) — More configuration options
