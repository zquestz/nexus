# Troubleshooting

This guide covers common issues and their solutions when using the Nexus BBS client.

## Connection Issues

### "Connection refused" error

**Cause:** The server isn't accepting connections on the specified address/port.

**Solutions:**
1. Verify the server address and port are correct
2. Ensure the server is running
3. Check if a firewall is blocking the connection
4. Try connecting with the IP address instead of hostname

### "Connection timed out" error

**Cause:** The server is unreachable or too slow to respond.

**Solutions:**
1. Check your internet connection
2. Verify the server address is correct
3. The server may be overloaded — try again later
4. If using a proxy, verify it has network access

### "Certificate fingerprint mismatch" warning

**Cause:** The server's TLS certificate has changed since your last connection.

**What to do:**
1. **If expected** (server reinstall, new certificate): Click **Accept** to save the new fingerprint
2. **If unexpected**: Contact the server operator to verify — this could indicate a security issue
3. Click **Cancel** to disconnect without saving

### Disconnected immediately after login

**Possible causes:**
- Your account may be disabled
- You may have been kicked or banned
- The server may have connection limits

**Solutions:**
1. Check if you received an error message before disconnect
2. Contact the server administrator
3. Try a different account if available

## Authentication Issues

### "Invalid username or password" error

**Solutions:**
1. Verify your username and password are correct
2. Usernames are case-insensitive, but passwords are case-sensitive
3. Check if your account exists on this server
4. Contact the server admin if you've forgotten your password

### "User disabled" error

**Cause:** Your account has been disabled by an administrator.

**Solution:** Contact the server administrator to have your account re-enabled.

### Shared account "Nickname already in use" error

**Cause:** Another user is already connected with that nickname.

**Solution:** Choose a different nickname when connecting.

### Guest login not working

**Cause:** The server may not have guest access enabled.

**Solutions:**
1. Leave username and password empty, enter only a nickname
2. Contact the server admin to verify guest access is available
3. Create a regular account if guest access isn't enabled

## Display Issues

### Text appears garbled or boxes show instead of characters

**Cause:** Missing font support for certain characters.

**Solutions:**
1. Nexus includes fonts for most languages, but some characters may not render
2. Ensure your system has fonts installed for the language in question
3. Report the issue if common characters aren't displaying correctly

### Theme colors look wrong

**Solutions:**
1. Try a different theme in Settings > General
2. Some themes may not work well with certain display configurations
3. The Dark or Light themes are the most widely tested

### Window is too large/small

**Solutions:**
1. Resize the window by dragging its edges
2. Window size is saved automatically when you close the app
3. If the window is off-screen, delete `~/.config/nexus/config.json` to reset

## Chat Issues

### Messages not appearing

**Solutions:**
1. Check that you're viewing the correct tab (Server vs. PM tabs)
2. Scroll down — you may be viewing older messages
3. Verify you're connected (check the server list)

### Can't send messages

**Possible causes:**
- You may not have `chat_send` permission
- The message may be too long (max 4096 characters)
- Connection may have been lost

**Solutions:**
1. Check for error messages in chat
2. Verify your connection status
3. Contact the server admin about permissions

### Notifications not working

**Solutions:**
1. Check Settings > Events > Enable notifications is on
2. Verify the specific event has notifications enabled
3. Check your system notification settings
4. On Linux, ensure a notification daemon is running (e.g., `dunst`, `mako`)

### Sounds not playing

**Solutions:**
1. Check Settings > Events > Enable sound is on
2. Verify volume is above 0%
3. Check your system audio settings
4. On Linux, ensure ALSA or PulseAudio is working

## File Transfer Issues

### Downloads fail immediately

**Possible causes:**
- Transfer port (7501) may be blocked
- You may not have `file_download` permission

**Solutions:**
1. Verify you have download permission (check with server admin)
2. Check if port 7501 is accessible
3. Try a different file

### Uploads fail or are rejected

**Possible causes:**
- Not in an upload-enabled folder
- File too large
- Missing `file_upload` permission

**Solutions:**
1. Navigate to a folder that allows uploads (look for upload indicator)
2. Check server's file size limits
3. Verify you have upload permission

### Transfer stuck at "Connecting"

**Solutions:**
1. Check your internet connection
2. The transfer port (7501) may be blocked by firewall
3. Try pausing and resuming the transfer
4. Cancel and restart the transfer

### Transfer fails with "Hash mismatch"

**Cause:** The file was modified during transfer or data was corrupted.

**Solutions:**
1. Cancel the transfer and try again
2. If it persists, the file may be changing on the server
3. Contact the server admin

### Resumed transfer fails

**Solutions:**
1. Delete the partial `.part` file and start fresh
2. The server file may have changed since you started
3. Disable resume by deleting the transfer and starting over

## Proxy Issues

### Proxy connection fails

**Solutions:**
1. Verify the proxy server is running
2. Check the address and port are correct
3. Test the proxy with another application
4. Try disabling authentication if enabled

### Slow connections through proxy

**Cause:** Proxy adds latency to all connections.

**Solutions:**
1. This is normal behavior for proxy connections
2. Use a faster proxy server if available
3. Disable proxy for local/trusted servers

### Some servers work, others don't through proxy

**Cause:** The destination server may block proxy connections, or the proxy may not support certain addresses.

**Solutions:**
1. Try connecting directly (disable proxy temporarily)
2. Some servers block known proxy/Tor exit nodes
3. Contact the server operator

## Configuration Issues

### Settings not saving

**Cause:** Cannot write to config directory.

**Solutions:**
1. Check write permissions for config directory:
   - Linux/macOS: `~/.config/nexus/`
   - Windows: `%APPDATA%\nexus\`
2. Ensure disk has free space
3. Check if antivirus is blocking file writes

### Config file corrupted

**Solution:** Delete the config file to reset to defaults:

```
# Linux/macOS
rm ~/.config/nexus/config.json

# Windows
del %APPDATA%\nexus\config.json
```

You'll lose your settings and bookmarks, but the client will create a fresh configuration.

### Bookmarks disappeared

**Possible causes:**
- Config file was deleted or corrupted
- Config directory permissions changed

**Solutions:**
1. Check if `config.json` exists in the config directory
2. If corrupted, you may need to re-create bookmarks
3. Bookmarks are stored in the config file — back it up periodically

## Platform-Specific Issues

### Linux: "ALSA lib" errors on startup

**Cause:** Missing ALSA development library.

**Solution:** Install the ALSA library:

```bash
# Debian/Ubuntu
sudo apt-get install libasound2-dev

# Fedora
sudo dnf install alsa-lib-devel

# Arch
sudo pacman -S alsa-lib
```

### Linux: No sound

**Solutions:**
1. Ensure ALSA or PulseAudio is working
2. Check that sound isn't muted at system level
3. Verify with `aplay -l` that audio devices are detected

### macOS: "App is damaged" error

**Cause:** macOS Gatekeeper blocking unsigned app.

**Solution:**
1. Right-click the app and select **Open**
2. Or run: `xattr -cr /path/to/Nexus.app`

### Windows: Firewall prompt

**Solution:** Allow Nexus through Windows Firewall when prompted. The client needs network access to connect to servers.

## Getting Help

If your issue isn't covered here:

1. Check the [GitHub Issues](https://github.com/zquestz/nexus/issues) for similar reports
2. Open a new issue with:
   - Your operating system and version
   - Steps to reproduce the problem
   - Any error messages you see
   - Relevant log output (if available)

## Next Steps

- [Getting Started](01-getting-started.md) — Installation and first connection
- [Settings](07-settings.md) — Configuration options
- [Commands](04-commands.md) — Chat commands reference