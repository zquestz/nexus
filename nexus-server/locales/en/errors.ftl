# Authentication & Session Errors
err-not-logged-in = Not logged in
err-authentication = Authentication error
err-invalid-credentials = Invalid username or password
err-handshake-required = Handshake required
err-already-logged-in = Already logged in
err-handshake-already-completed = Handshake already completed
err-account-deleted = Your account has been deleted
err-account-disabled-by-admin = Account disabled by admin

# Permission & Access Errors
err-permission-denied = Permission denied

# Database Errors
err-database = Database error

# Message Format Errors
err-invalid-message-format = Invalid message format

# User Management Errors
err-cannot-delete-last-admin = Cannot delete the last admin
err-cannot-delete-self = You cannot delete yourself
err-cannot-demote-last-admin = Cannot demote the last admin
err-cannot-edit-self = You cannot edit yourself
err-cannot-create-admin = Only admins can create admin users
err-cannot-kick-self = You cannot kick yourself
err-cannot-kick-admin = Cannot kick admin users
err-cannot-message-self = You cannot message yourself
err-cannot-disable-last-admin = Cannot disable the last admin

# Chat Topic Errors
err-topic-contains-newlines = Topic cannot contain newlines

# Message Validation Errors
err-message-empty = Message cannot be empty

# Username Validation Errors
err-username-empty = Username cannot be empty
err-username-invalid = Username contains invalid characters (letters, numbers, and symbols allowed - no whitespace or control characters)

# Dynamic Error Messages (with parameters)
err-broadcast-too-long = Message too long (max { $max_length } characters)
err-chat-too-long = Message too long (max { $max_length } characters)
err-topic-too-long = Topic cannot exceed { $max_length } characters
err-version-mismatch = Version mismatch: server uses { $server_version }, client uses { $client_version }
err-kicked-by = You have been kicked by { $username }
err-username-exists = Username '{ $username }' already exists
err-user-not-found = User '{ $username }' not found
err-user-not-online = User '{ $username }' is not online
err-failed-to-create-user = Failed to create user '{ $username }'
err-account-disabled = Account '{ $username }' is disabled
err-update-failed = Failed to update user '{ $username }'
err-username-too-long = Username is too long (max { $max_length } characters)