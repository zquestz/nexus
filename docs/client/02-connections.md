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

Nexus uses Trust On First Use (TOFU) for certificate verification:

- **First connection**: The certificate fingerprint is saved automatically
- **Subsequent connections**: The fingerprint is verified against the saved value
- **Mismatch**: A warning dialog appears if the fingerprint changes

### Accepting a New Certificate

If a server's certificate changes (e.g., after server reinstall):

1. A fingerprint mismatch dialog will appear
2. Verify with the server operator that the change is legitimate
3. Click **Accept** to save the new fingerprint, or **Cancel** to disconnect

The new fingerprint replaces the old one in your bookmark.

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

| Setting | Default | Description |
|---------|---------|-------------|
| Address | `127.0.0.1` | Proxy server hostname or IP |
| Port | `9050` | Proxy server port (Tor default) |
| Username | (empty) | Optional authentication |
| Password | (empty) | Optional authentication |

### Proxy Bypass

Some addresses automatically bypass the proxy:

- **Localhost**: `127.0.0.1`, `::1`, `localhost`
- **Yggdrasil**: Addresses in the `0200::/7` range

This ensures local connections and Yggdrasil mesh traffic are not routed through the proxy.

### Using with Tor

To route Nexus traffic through Tor:

1. Install and start the Tor service
2. In Nexus Settings > Network, enable proxy
3. Use address `127.0.0.1` and port `9050` (default Tor SOCKS port)
4. Connect to servers using their `.onion` addresses or regular addresses

Note: The server operator must also be reachable through Tor for this to work.

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

- [Chat](03-chat.md) — Server chat and private messages
- [Settings](07-settings.md) — More configuration options