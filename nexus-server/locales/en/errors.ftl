# Authentication & Session Errors
err-not-logged-in = Not logged in

# Nickname Validation Errors
err-nickname-empty = Nickname cannot be empty
err-nickname-in-use = Nickname is already in use
err-nickname-invalid = Nickname contains invalid characters (letters, numbers, and symbols allowed - no whitespace or control characters)
err-nickname-is-username = Nickname cannot be an existing username
err-nickname-not-found = User '{ $nickname }' not found
err-nickname-not-online = User '{ $nickname }' is not online
err-nickname-required = Nickname required for shared accounts
err-nickname-too-long = Nickname is too long (max { $max_length } characters)

# Away Message Errors
err-status-too-long = Status message is too long (max { $max_length } characters)
err-status-contains-newlines = Status message cannot contain newlines
err-status-invalid-characters = Status message contains invalid characters

# Shared Account Errors
err-shared-cannot-be-admin = Shared accounts cannot be admins
err-shared-cannot-self-edit = Shared accounts cannot edit themselves
err-shared-invalid-permissions = Shared accounts cannot have these permissions: { $permissions }
err-shared-message-requires-nickname = Shared accounts can only be messaged by nickname
err-shared-kick-requires-nickname = Shared accounts can only be kicked by nickname

# Guest Account Errors
err-guest-disabled = Guest access is not enabled on this server
err-cannot-rename-guest = The guest account cannot be renamed
err-cannot-change-guest-password = The guest account password cannot be changed
err-cannot-delete-guest = The guest account cannot be deleted

# Avatar Validation Errors
err-avatar-invalid-format = Invalid avatar format (must be a data URI with base64 encoding)
err-avatar-too-large = Avatar is too large (max { $max_length } bytes)
err-avatar-unsupported-type = Unsupported avatar type (PNG, WebP, or SVG only)
err-authentication = Authentication error
err-invalid-credentials = Invalid username or password
err-handshake-required = Handshake required
err-already-logged-in = Already logged in
err-handshake-already-completed = Handshake already completed
err-account-deleted = Your account has been deleted
err-account-disabled-by-admin = Account disabled by admin
err-account-type-changed = This account's type changed. Please reconnect.

# Permission & Access Errors
err-permission-denied = Permission denied
err-permission-denied-chat-create = Permission denied: you can join existing channels but cannot create new ones

# Feature Errors
err-chat-feature-not-enabled = Chat feature not enabled

# Database Errors
err-database = Database error
err-login-permissions-failed = Failed to load account permissions
err-login-group-failed = Failed to load account group
err-login-bandwidth-failed = Failed to load account bandwidth settings

# Message Format Errors
err-invalid-message-format = Invalid message format
err-message-not-supported = Message type not supported

# User Management Errors
err-cannot-delete-last-admin = Cannot delete the last admin
err-cannot-delete-self = You cannot delete yourself
err-cannot-demote-last-admin = Cannot demote the last admin
err-cannot-edit-self = You cannot edit yourself
err-current-password-required = Current password is required to change your password
err-current-password-incorrect = Current password is incorrect
err-cannot-create-admin = Only admins can create admin users
err-admin-cannot-have-group = Cannot assign admin users to a group
err-cannot-kick-self = You cannot kick yourself
err-cannot-kick-admin = Cannot kick admin users
err-cannot-delete-admin = Only admins can delete admin users
err-cannot-edit-admin = Only admins can edit admin users
err-cannot-message-self = You cannot message yourself
err-cannot-disable-last-admin = Cannot disable the last admin

# Chat Topic Errors
err-topic-contains-newlines = Topic cannot contain newlines
err-topic-invalid-characters = Topic contains invalid characters

# Channel Errors
err-channel-name-empty = Channel name cannot be empty
err-channel-name-too-short = Channel name must have at least one character after #
err-channel-name-too-long = Channel name is too long (max { $max_length } characters)
err-channel-name-invalid = Channel name contains invalid characters
err-channel-name-missing-prefix = Channel name must start with #
err-channel-not-found = Channel '{ $channel }' not found
err-channel-already-member = You are already a member of channel '{ $channel }'
err-channel-limit-exceeded = You cannot join more than { $max } channels
err-channel-list-invalid = Invalid channel '{ $channel }': { $reason }

# Version Validation Errors
err-version-empty = Version cannot be empty
err-version-too-long = Version is too long (max { $max_length } bytes)
err-version-invalid-semver = Version must be in semver format (MAJOR.MINOR.PATCH)

# Password Validation Errors
err-password-empty = Password cannot be empty
err-password-too-long = Password is too long (max { $max_length } bytes)
err-password-too-weak = Password is too weak, minimum strength is { $required ->
    [0] Weak
    [1] Fair
    [2] Good
    [3] Strong
    [4] Excellent
   *[other] Unknown
}

# Locale Validation Errors
err-locale-too-long = Locale is too long (max { $max_length } bytes)
err-locale-invalid-characters = Locale contains invalid characters

# Features Validation Errors
err-features-too-many = Too many features (max { $max_count })
err-features-empty-feature = Feature name cannot be empty
err-features-feature-too-long = Feature name is too long (max { $max_length } bytes)
err-features-invalid-characters = Feature name contains invalid characters

# Permissions Validation Errors
err-permissions-too-many = Too many permissions (max { $max_count })
err-permission-grant-revoke-conflict = Permission { $permission } cannot be both granted and revoked
err-permissions-empty-permission = Permission name cannot be empty
err-permissions-permission-too-long = Permission name is too long (max { $max_length } bytes)
err-permissions-contains-newlines = Permission name cannot contain newlines
err-permissions-invalid-characters = Permission name contains invalid characters

# Message Validation Errors
err-message-empty = Message cannot be empty
err-message-contains-newlines = Message cannot contain newlines
err-message-invalid-characters = Message contains invalid characters

# Username Validation Errors
err-username-empty = Username cannot be empty
err-username-invalid = Username contains invalid characters (letters, numbers, and symbols allowed - no whitespace or control characters)

# Unknown Permission Error
err-unknown-permission = Unknown permission: '{ $permission }'

# Dynamic Error Messages (with parameters)
err-broadcast-too-long = Message too long (max { $max_length } characters)
err-chat-too-long = Message too long (max { $max_length } characters)
err-topic-too-long = Topic cannot exceed { $max_length } characters
err-version-major-mismatch = Incompatible protocol version: server is version { $server_major }.x, client is version { $client_major }.x
err-version-client-too-new = Client version { $client_version } is newer than server version { $server_version }. Please update the server or use an older client.
err-version-minor-mismatch = Incompatible protocol version. Server: { $server_version }, Client: { $client_version }. Both must use the same minor version.
err-kicked-by = You have been kicked by { $username }
err-kicked-by-reason = You have been kicked by { $username }: { $reason }
err-kick-reason-too-long = Kick reason is too long (max { $max_length } characters)
err-kick-reason-invalid-characters = Kick reason contains invalid characters
err-username-exists = Username '{ $username }' already exists
err-user-not-found = User '{ $username }' not found
err-user-not-online = User '{ $username }' is not online
err-failed-to-create-user = Failed to create user '{ $username }'
err-account-disabled = Account '{ $username }' is disabled
err-update-failed = Failed to update user '{ $username }'
err-username-too-long = Username is too long (max { $max_length } characters)

# Server Update Errors
err-admin-required = Admin privileges required
err-server-name-empty = Server name cannot be empty
err-server-name-too-long = Server name is too long (max { $max_length } characters)
err-server-name-contains-newlines = Server name cannot contain newlines
err-server-name-invalid-characters = Server name contains invalid characters
err-server-description-too-long = Server description is too long (max { $max_length } characters)
err-server-description-contains-newlines = Server description cannot contain newlines
err-server-description-invalid-characters = Server description contains invalid characters
err-server-image-too-large = Server image is too large (max 512KB)
err-server-image-invalid-format = Invalid server image format (must be a data URI with base64 encoding)
err-server-image-unsupported-type = Unsupported server image type (PNG, WebP, JPEG, or SVG only)
err-public-address-too-long = Public address is too long (max { $max_length } bytes)
err-public-address-contains-scheme = Public address must not include a URL scheme
err-public-address-contains-brackets = Public address must not include brackets
err-public-address-contains-path = Public address must not include a path
err-public-address-contains-userinfo = Public address must not include a username
err-public-address-contains-whitespace = Public address must not contain whitespace
err-public-address-contains-port = Public address must not include a port
err-public-address-contains-zone-id = Public address must not include an IPv6 zone identifier
err-public-address-invalid-format = Public address is not a valid hostname or IP address
err-no-fields-to-update = No fields to update
err-invalid-password-strength = Invalid password strength value

# News Errors
err-news-not-found = News item #{ $id } not found
err-news-body-too-long = News body is too long (max { $max_length } characters)
err-news-body-invalid-characters = News body contains invalid characters
err-news-image-too-large = News image is too large (max 512KB)
err-news-image-invalid-format = Invalid news image format (must be a data URI with base64 encoding)
err-news-image-unsupported-type = Unsupported news image type (PNG, WebP, JPEG, or SVG only)
err-news-empty-content = News must have either text content or an image
err-cannot-edit-admin-news = Only admins can edit news posted by admins
err-cannot-delete-admin-news = Only admins can delete news posted by admins

# File Area Errors
err-file-path-too-long = File path is too long (max { $max_length } bytes)
err-file-path-invalid = File path contains invalid characters
err-file-not-found = File or directory not found
err-file-not-directory = Path is not a directory
err-dir-name-empty = Directory name cannot be empty
err-dir-name-too-long = Directory name is too long (max { $max_length } bytes)
err-dir-name-invalid = Directory name contains invalid characters
err-dir-already-exists = A file or directory with that name already exists
err-dir-create-failed = Failed to create directory
err-dir-not-empty = Directory is not empty
err-delete-failed = Failed to delete file or directory
err-rename-failed = Failed to rename file or directory
err-rename-target-exists = A file or directory with that name already exists
err-move-failed = Failed to move file or directory
err-copy-failed = Failed to copy file or directory
err-destination-exists = A file or directory with that name already exists at the destination
err-cannot-move-into-itself = Cannot move a directory into itself
err-cannot-copy-into-itself = Cannot copy a directory into itself
err-destination-not-directory = Destination path is not a directory

# Transfer Errors
err-file-area-not-configured = File area not configured
err-file-area-not-accessible = File area not accessible
err-transfer-path-too-long = Path is too long
err-transfer-path-invalid = Path contains invalid characters
err-transfer-access-denied = Access denied
err-transfer-read-failed = Failed to read files
err-transfer-path-not-found = File or directory not found
err-transfer-file-failed = Failed to transfer { $path }: { $error }

# Upload Errors
err-upload-destination-not-allowed = Destination folder does not allow uploads
err-upload-write-failed = Failed to write file
err-upload-hash-mismatch = File verification failed - hash mismatch
err-upload-path-invalid = Invalid file path in upload
err-upload-conflict = Another upload to this filename is in progress or was interrupted. Please try a different filename.
err-upload-file-exists = A file with this name already exists. Please choose a different filename or ask an admin to delete the existing file.
err-upload-empty = Upload must contain at least one file
err-upload-protocol-error = Upload protocol error
err-upload-connection-lost = Connection lost during upload

# Ban System Errors
err-ban-self = Cannot ban yourself
err-ban-admin-by-nickname = Cannot ban administrators
err-ban-admin-by-ip = Cannot ban this IP
err-ban-invalid-target = Invalid target (use nickname, IP address, or CIDR range)
err-target-too-long = Target is too long (max { $max_length } characters)
err-ban-invalid-duration = Invalid duration format (use 10m, 4h, 7d, or 0 for permanent)
err-ban-not-found = No ban found for '{ $target }'
err-reason-too-long = Reason is too long (max { $max_length } characters)
err-reason-invalid = Reason contains invalid characters
err-banned-permanent = You have been banned from this server
err-banned-with-expiry = You have been banned from this server (expires in { $remaining })

# Trust System Errors
err-trust-invalid-target = Invalid target (use nickname, IP address, or CIDR range)
err-trust-invalid-duration = Invalid duration format (use 10m, 4h, 7d, or 0 for permanent)
err-trust-not-found = No trusted entry found for '{ $target }'

# File Search Errors
err-search-query-empty = Search query cannot be empty
err-search-query-too-short = Search query is too short (min { $min_length } bytes)
err-search-query-too-long = Search query is too long (max { $max_length } bytes)
err-search-query-invalid = Search query contains invalid characters
err-search-failed = Search failed

# Voice Errors
err-voice-listen-required = You need voice_listen permission to join voice
err-voice-already-joined = You are already in a voice session
err-voice-not-joined = You are not in a voice session
err-voice-not-channel-member = You must be a member of { $channel } to join voice
err-voice-target-not-online = { $nickname } is not online
err-voice-invalid-target = Invalid voice target

# Group Errors
err-group-name-empty = Group name cannot be empty
err-group-name-too-long = Group name is too long (max { $max_length } characters)
err-group-name-invalid = Group name contains invalid characters
err-group-not-found = Group not found
err-group-already-exists = A group with this name already exists
err-group-shared-permission = Shared groups cannot have this permission
err-group-not-empty-delete = Cannot delete group while users are assigned to it
err-group-not-empty-modify = Cannot modify shared status while users are assigned to it
err-group-no-fields = No fields to update
err-group-shared-mismatch = Account type does not match group type (shared accounts require shared groups)

# Tracker Errors
err-tracker-not-found = Tracker not found
err-tracker-no-pending-fingerprint = Tracker has no pending fingerprint to accept
err-tracker-name-invalid = Tracker name contains invalid characters
err-tracker-name-empty = Tracker name cannot be empty
err-tracker-name-contains-newlines = Tracker name cannot contain newlines
err-tracker-name-too-long = Tracker name is too long (max { $max_length } characters)
err-tracker-address-invalid = Invalid tracker address
err-tracker-address-empty = Tracker address cannot be empty
err-tracker-address-too-long = Tracker address is too long (max { $max_length } bytes)
err-tracker-address-contains-scheme = Tracker address must not include a URL scheme
err-tracker-address-contains-brackets = Tracker address must not include brackets
err-tracker-address-contains-path = Tracker address must not include a path
err-tracker-address-contains-userinfo = Tracker address must not include a username
err-tracker-address-contains-whitespace = Tracker address must not contain whitespace
err-tracker-address-contains-port = Tracker address must not include a port
err-tracker-address-contains-zone-id = Tracker address must not include an IPv6 zone identifier
err-tracker-address-invalid-format = Tracker address is not a valid hostname or IP address
err-tracker-port-invalid = Invalid tracker port
err-tracker-fingerprint-invalid = Invalid tracker fingerprint format
err-tracker-password-too-long = Tracker password is too long (max { $max_length } bytes)
err-tracker-endpoint-duplicate = Another tracker is already configured at this address and port
err-tracker-name-duplicate = Another tracker is already configured with this name
err-tracker-too-many = Tracker limit reached (max { $max })

# Tracker registration status messages — shown in the admin UI as the
# `last_error` field of TrackerInfo. Translated server-side using the
# requesting admin's locale; keyed off `last_error_kind`.
err-tracker-connection-failed = Could not connect to tracker
err-tracker-tls-failed = TLS handshake with tracker failed
err-tracker-handshake-failed = Tracker handshake failed
err-tracker-connection-lost = Connection to tracker lost
err-tracker-db-failed = Database error while updating tracker state
err-tracker-fingerprint-mismatch = Tracker certificate does not match the pinned fingerprint
err-tracker-fingerprint-intercepted = Tracker self-reported fingerprint does not match its TLS certificate
err-tracker-unauthorized = Tracker rejected registration
err-tracker-rate-limited = Rate limited by tracker
err-tracker-capacity = Tracker is at capacity
err-tracker-invalid = Tracker rejected the registration as invalid
err-tracker-protocol-error = Tracker sent a malformed error response
err-tracker-unknown = Tracker reported an unknown error

# Flood Protection Errors
err-flood-warning = Message rate limited (warning { $violation } of { $max_violations }). You can send another message in { $seconds } { $seconds ->
    [one] second
   *[other] seconds
}. Continued flooding will result in disconnection.
err-flood-disconnect = Disconnected: chat rate limit exceeded.

# Bandwidth Errors
err-bandwidth-weight-delegation = Cannot grant a bandwidth weight above your own
err-bandwidth-weight-inherit-would-elevate = Cannot inherit a bandwidth weight above your own
err-bandwidth-weight-zero = Bandwidth weight must be at least { $min }
err-bandwidth-chunk-size-too-small = Scheduler chunk size must be at least { $min } { $min ->
    [one] byte
   *[other] bytes
}
err-bandwidth-chunk-size-too-large = Scheduler chunk size must be at most { $max } { $max ->
    [one] byte
   *[other] bytes
}
