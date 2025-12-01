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
title-fingerprint-mismatch = Certificate Fingerprint Mismatch!

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
label-theme = Theme

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
msg-topic-display = Topic: { $topic }
msg-user-connected = { $username } connected
msg-user-disconnected = { $username } disconnected
msg-disconnected = Disconnected: { $error }
msg-connection-cancelled = Connection cancelled due to certificate mismatch

# =============================================================================
# Error Messages
# =============================================================================

err-connection-broken = Connection error
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
err-message-too-long = Chat message too long
err-send-failed = Failed to send message
err-broadcast-too-long = Broadcast message too long
err-broadcast-send-failed = Failed to send broadcast
err-name-required = Bookmark name is required
err-address-required = Server address is required
err-port-required = Port is required
err-username-required = Username is required
err-password-required = Password is required
err-message-required = Message is required

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

user-info-header = [{ $username }]
user-info-is-admin = is an Administrator
user-info-connected-ago = connected: { $duration } ago
user-info-connected-sessions = connected: { $duration } ago ({ $count } sessions)
user-info-features = features: { $features }
user-info-locale = locale: { $locale }
user-info-address = address: { $address }
user-info-addresses = addresses:
user-info-address-item = - { $address }
user-info-created = created: { $created }
user-info-end = End of user info
user-info-unknown = Unknown
user-info-error = Error: { $error }

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