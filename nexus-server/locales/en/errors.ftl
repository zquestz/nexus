# Authentication & Session Errors
err-not-logged-in = Not logged in

# Avatar Validation Errors
err-avatar-invalid-format = Invalid avatar format (must be a data URI with base64 encoding)
err-avatar-too-large = Avatar is too large (max { $max_length } characters)
err-avatar-unsupported-type = Unsupported avatar type (PNG, WebP, or SVG only)
err-authentication = Authentication error
err-invalid-credentials = Invalid username or password
err-handshake-required = Handshake required
err-already-logged-in = Already logged in
err-handshake-already-completed = Handshake already completed
err-account-deleted = Your account has been deleted
err-account-disabled-by-admin = Account disabled by admin

# Permission & Access Errors
err-permission-denied = Permission denied

# Feature Errors
err-chat-feature-not-enabled = Chat feature not enabled

# Database Errors
err-database = Database error

# Message Format Errors
err-invalid-message-format = Invalid message format

# User Management Errors
err-cannot-delete-last-admin = Cannot delete the last admin
err-cannot-delete-self = You cannot delete yourself
err-cannot-demote-last-admin = Cannot demote the last admin
err-cannot-edit-self = You cannot edit yourself
err-current-password-required = Current password is required to change your password
err-current-password-incorrect = Current password is incorrect
err-cannot-create-admin = Only admins can create admin users
err-cannot-kick-self = You cannot kick yourself
err-cannot-kick-admin = Cannot kick admin users
err-cannot-delete-admin = Only admins can delete admin users
err-cannot-edit-admin = Only admins can edit admin users
err-cannot-message-self = You cannot message yourself
err-cannot-disable-last-admin = Cannot disable the last admin

# Chat Topic Errors
err-topic-contains-newlines = Topic cannot contain newlines
err-topic-invalid-characters = Topic contains invalid characters

# Version Validation Errors
err-version-empty = Version cannot be empty
err-version-too-long = Version is too long (max { $max_length } characters)
err-version-invalid-semver = Version must be in semver format (MAJOR.MINOR.PATCH)

# Password Validation Errors
err-password-empty = Password cannot be empty
err-password-too-long = Password is too long (max { $max_length } characters)

# Locale Validation Errors
err-locale-too-long = Locale is too long (max { $max_length } characters)
err-locale-invalid-characters = Locale contains invalid characters

# Features Validation Errors
err-features-too-many = Too many features (max { $max_count })
err-features-empty-feature = Feature name cannot be empty
err-features-feature-too-long = Feature name is too long (max { $max_length } characters)
err-features-invalid-characters = Feature name contains invalid characters

# Permissions Validation Errors
err-permissions-too-many = Too many permissions (max { $max_count })
err-permissions-empty-permission = Permission name cannot be empty
err-permissions-permission-too-long = Permission name is too long (max { $max_length } characters)
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
err-kicked-by = You have been kicked by { $username }
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
err-max-connections-per-ip-invalid = Max connections per IP must be greater than 0
err-no-fields-to-update = No fields to update

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