# Nexus BBS Client - English Translations

# =============================================================================
# Buttons
# =============================================================================

button-cancel = Cancel
button-send = Send
button-delete = Delete
button-connect = Connect
button-save = Save
button-create = Create
button-edit = Edit
button-update = Update
button-accept-new-certificate = Accept New Certificate
button-close = Close
button-choose-avatar = Choose Avatar
button-clear-avatar = Clear

# =============================================================================
# Titles
# =============================================================================

title-nexus-bbs = Nexus BBS
title-connect-to-server = Connect to Server
title-add-bookmark = Add Bookmark
title-edit-server = Edit Server
title-broadcast-message = Broadcast Message
title-user-create = User Create
title-user-edit = User Edit
title-update-user = Update User
title-connected = Connected
title-settings = Settings
title-bookmarks = Bookmarks
title-users = Users
title-edit-server-info = Edit Server Info
title-fingerprint-mismatch = Certificate Fingerprint Mismatch!
title-server-info = Server Info
title-user-info = User Info
title-about = About

# =============================================================================
# Placeholders
# =============================================================================

placeholder-username = Username
placeholder-password = Password
placeholder-port = Port
placeholder-server-address = Server Address
placeholder-server-name = Server Name
placeholder-username-optional = Username (optional)
placeholder-password-optional = Password (optional)
placeholder-password-keep-current = Password (leave empty to keep current)
placeholder-message = Type a message...
placeholder-no-permission = No permission
placeholder-broadcast-message = Enter broadcast message...
placeholder-server-description = Server description

# =============================================================================
# Labels
# =============================================================================

label-auto-connect = Auto-Connect
label-add-bookmark = Add Bookmark
label-admin = Admin
label-enabled = Enabled
label-permissions = Permissions:
label-expected-fingerprint = Expected fingerprint:
label-received-fingerprint = Received fingerprint:
label-theme = Theme:
label-chat-font-size = Font Size:
label-show-connection-notifications = Show connect/disconnect notifications
label-show-timestamps = Show timestamps
label-use-24-hour-time = Use 24-hour time
label-show-seconds = Show seconds
label-server-name = Name:
label-server-description = Description:
label-server-version = Version:
label-chat-topic = Chat Topic:
label-chat-topic-set-by = Chat Topic Set By:
label-max-connections-per-ip = Max Connections Per IP:
label-avatar = Avatar:
label-details = Technical Details
label-chat-options = Chat Options
label-appearance = Appearance

# =============================================================================
# Permission Display Names
# =============================================================================

permission-user_list = User List
permission-user_info = User Info
permission-chat_send = Chat Send
permission-chat_receive = Chat Receive
permission-chat_topic = Chat Topic
permission-chat_topic_edit = Chat Topic Edit
permission-user_broadcast = User Broadcast
permission-user_create = User Create
permission-user_delete = User Delete
permission-user_edit = User Edit
permission-user_kick = User Kick
permission-user_message = User Message

# =============================================================================
# Tooltips
# =============================================================================

tooltip-chat = Chat
tooltip-broadcast = Broadcast
tooltip-user-create = User Create
tooltip-user-edit = User Edit
tooltip-server-info = Server Info
tooltip-about = About
tooltip-settings = Settings
tooltip-hide-bookmarks = Hide Bookmarks
tooltip-show-bookmarks = Show Bookmarks
tooltip-hide-user-list = Hide User List
tooltip-show-user-list = Show User List
tooltip-disconnect = Disconnect
tooltip-edit = Edit
tooltip-info = Info
tooltip-message = Message
tooltip-kick = Kick
tooltip-close = Close
tooltip-add-bookmark = Add Bookmark

# =============================================================================
# Empty States
# =============================================================================

empty-select-server = Select a server from the list
empty-no-connections = No connections
empty-no-bookmarks = No bookmarks
empty-no-users = No users online

# =============================================================================
# Chat Tab Labels
# =============================================================================

chat-tab-server = #server

# =============================================================================
# System Message Usernames
# =============================================================================


# =============================================================================
# Chat Message Prefixes
# =============================================================================

chat-prefix-system = [SYS]
chat-prefix-error = [ERR]
chat-prefix-info = [INFO]
chat-prefix-broadcast = [BROADCAST]

# =============================================================================
# Success Messages
# =============================================================================

msg-user-kicked-success = User kicked successfully
msg-broadcast-sent = Broadcast sent successfully
msg-user-created = User created successfully
msg-user-deleted = User deleted successfully
msg-user-updated = User updated successfully
msg-permissions-updated = Your permissions have been updated
msg-topic-updated = Topic updated successfully



# =============================================================================
# Dynamic Messages (with parameters)
# =============================================================================

msg-topic-cleared = Topic cleared by { $username }
msg-topic-set = Topic set by { $username }: { $topic }
msg-server-info-updated = Server configuration updated
msg-topic-display = Topic: { $topic }
msg-user-connected = { $username } connected
msg-user-disconnected = { $username } disconnected
msg-disconnected = Disconnected: { $error }
msg-connection-cancelled = Connection cancelled due to certificate mismatch

# =============================================================================
# Error Messages
# =============================================================================

err-connection-broken = Connection error
err-failed-update-server-info = Failed to update server info: { $error }
err-user-kick-failed = Failed to kick user
err-no-shutdown-handle = Connection error: No shutdown handle
err-userlist-failed = Failed to refresh user list
err-port-invalid = Port must be a valid number (1-65535)

# Network connection errors
err-no-peer-certificates = No peer certificates found
err-no-certificates-in-chain = No certificates in chain
err-unexpected-handshake-response = Unexpected handshake response
err-no-session-id = No session ID received
err-login-failed = Login failed
err-unexpected-login-response = Unexpected login response
err-connection-closed = Connection closed
err-could-not-determine-config-dir = Could not determine config directory
err-message-too-long = Message is too long ({ $length } characters, max { $max })
err-send-failed = Failed to send message
err-no-chat-permission = You don't have permission to send messages
err-broadcast-too-long = Broadcast is too long ({ $length } characters, max { $max })
err-broadcast-send-failed = Failed to send broadcast
err-name-required = Bookmark name is required
err-address-required = Server address is required
err-port-required = Port is required
err-username-required = Username is required
err-password-required = Password is required
err-message-required = Message is required

# Validation errors
err-message-empty = Message cannot be empty
err-message-contains-newlines = Message cannot contain newlines
err-message-invalid-characters = Message contains invalid characters
err-username-empty = Username cannot be empty
err-username-too-long = Username is too long (max { $max } characters)
err-username-invalid = Username contains invalid characters
err-password-too-long = Password is too long (max { $max } characters)
err-topic-too-long = Topic is too long ({ $length } characters, max { $max })
err-avatar-unsupported-type = Unsupported file type. Use PNG, WebP, or SVG.
err-avatar-too-large = Avatar too large. Maximum size is { $max_kb }KB.
err-server-name-empty = Server name cannot be empty
err-server-name-too-long = Server name is too long (max { $max } characters)
err-server-name-contains-newlines = Server name cannot contain newlines
err-server-name-invalid-characters = Server name contains invalid characters
err-server-description-too-long = Description is too long (max { $max } characters)
err-server-description-contains-newlines = Description cannot contain newlines
err-server-description-invalid-characters = Description contains invalid characters
err-failed-send-update = Failed to send update: { $error }

# =============================================================================
# Dynamic Error Messages (with parameters)
# =============================================================================

err-failed-save-config = Failed to save config: { $error }
err-failed-save-settings = Failed to save settings: { $error }
err-invalid-port-bookmark = Invalid port in bookmark: { $name }
err-failed-send-broadcast = Failed to send broadcast: { $error }
err-failed-send-message = Failed to send message: { $error }
err-failed-create-user = Failed to create user: { $error }
err-failed-delete-user = Failed to delete user: { $error }
err-failed-update-user = Failed to update user: { $error }
err-failed-update-topic = Failed to update topic: { $error }
err-message-too-long-details = { $error } ({ $length } characters, max { $max })

# Network connection errors (with parameters)
err-invalid-address = Invalid address '{ $address }': { $error }
err-could-not-resolve = Could not resolve address '{ $address }'
err-connection-timeout = Connection timed out after { $seconds } seconds
err-connection-failed = Connection failed: { $error }
err-tls-handshake-failed = TLS handshake failed: { $error }
err-failed-send-handshake = Failed to send handshake: { $error }
err-failed-read-handshake = Failed to read handshake response: { $error }
err-handshake-failed = Handshake failed: { $error }
err-failed-parse-handshake = Failed to parse handshake response: { $error }
err-failed-send-login = Failed to send login: { $error }
err-failed-read-login = Failed to read login response: { $error }
err-failed-parse-login = Failed to parse login response: { $error }
err-failed-create-server-name = Failed to create server name: { $error }
err-failed-create-config-dir = Failed to create config directory: { $error }
err-failed-serialize-config = Failed to serialize config: { $error }
err-failed-write-config = Failed to write config file: { $error }
err-failed-read-config-metadata = Failed to read config file metadata: { $error }
err-failed-set-config-permissions = Failed to set config file permissions: { $error }

# =============================================================================
# Fingerprint Warning
# =============================================================================

fingerprint-warning = This could indicate a security issue (MITM attack) or the server's certificate was regenerated. Only accept if you trust the server administrator.

# =============================================================================
# User Info Display
# =============================================================================

user-info-username = Username:
user-info-role = Role:
user-info-role-admin = admin
user-info-role-user = user
user-info-connected = Connected:
user-info-connected-value = { $duration } ago
user-info-connected-value-sessions = { $duration } ago ({ $count } sessions)
user-info-features = Features:
user-info-features-value = { $features }
user-info-features-none = None
user-info-locale = Locale:
user-info-address = Address:
user-info-addresses = Addresses:
user-info-created = Created:
user-info-end = End of user info
user-info-unknown = Unknown
user-info-loading = Loading user info...

# =============================================================================
# Time Duration
# =============================================================================

time-days = { $count } { $count ->
    [one] day
   *[other] days
}
time-hours = { $count } { $count ->
    [one] hour
   *[other] hours
}
time-minutes = { $count } { $count ->
    [one] minute
   *[other] minutes
}
time-seconds = { $count } { $count ->
    [one] second
   *[other] seconds
}

# =============================================================================
# Command System
# =============================================================================

cmd-unknown = Unknown command: /{ $command }
cmd-help-header = Available commands:
cmd-help-desc = Show available commands
cmd-help-usage = Usage: /{ $command } [command]
cmd-help-escape-hint = Tip: Use // to send a message starting with /
cmd-message-desc = Send a message to a user
cmd-message-usage = Usage: /{ $command } <username> <message>
cmd-userinfo-desc = Show information about a user
cmd-userinfo-usage = Usage: /{ $command } <username>
cmd-kick-desc = Kick a user from the server
cmd-kick-usage = Usage: /{ $command } <username>
cmd-topic-desc = View or manage the chat topic
cmd-topic-usage = Usage: /{ $command } [set|clear] [topic]
cmd-topic-set-usage = Usage: /{ $command } set <topic>
cmd-topic-none = No topic is set
cmd-topic-permission-denied = You don't have permission to edit the topic
cmd-broadcast-desc = Send a broadcast to all users
cmd-broadcast-usage = Usage: /{ $command } <message>
cmd-clear-desc = Clear chat history for current tab
cmd-clear-usage = Usage: /{ $command }
cmd-focus-desc = Focus server chat or a user's message tab
cmd-focus-usage = Usage: /{ $command } [username]
cmd-focus-not-found = User not found: { $name }
cmd-list-desc = Show connected users
cmd-list-usage = Usage: /{ $command }
cmd-list-empty = No users connected
cmd-list-output = Users online: { $users } ({ $count } { $count ->
    [one] user
   *[other] users
})
cmd-window-desc = Manage chat tabs
cmd-window-usage = Usage: /{ $command } [next|prev|close [username]]
cmd-window-list = Open tabs: { $tabs } ({ $count } { $count ->
    [one] tab
   *[other] tabs
})
cmd-window-close-server = Cannot close the server tab
cmd-window-not-found = Tab not found: { $name }
cmd-serverinfo-desc = Show server information
cmd-serverinfo-usage = Usage: /{ $command }
cmd-serverinfo-header = [server]
cmd-serverinfo-end = End of server info

# =============================================================================
# About Panel
# =============================================================================

about-app-name = Nexus BBS
about-copyright = © 2025 Nexus BBS Project