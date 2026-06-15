# Tracker error messages
#
# All keys use the `err-tracker-*` prefix to keep them isolated from the
# nexus-server locale namespace. Keys correspond 1:1 with `err_tracker_*`
# helpers in `nexus-tracker/src/errors.rs`.

# Authentication
err-tracker-unauthorized = Wrong or missing password

# Field validation (error_kind: invalid)
err-tracker-fingerprint-invalid = Invalid certificate fingerprint format
err-tracker-name-too-long = Server name is too long (max { $max_length } characters)
err-tracker-name-empty = Server name cannot be empty
err-tracker-name-contains-newlines = Server name cannot contain newlines
err-tracker-name-invalid-characters = Server name contains invalid characters
err-tracker-description-too-long = Server description is too long (max { $max_length } characters)
err-tracker-description-contains-newlines = Server description cannot contain newlines
err-tracker-description-invalid-characters = Server description contains invalid characters
err-tracker-password-too-long = Password is too long (max { $max_length } bytes)
err-tracker-address-too-long = Address is too long (max { $max_length } bytes)
err-tracker-address-invalid = Invalid address
err-tracker-version-too-long = Server version string is too long (max { $max_length } bytes)
err-tracker-version-invalid = Invalid version (must be valid semver)
err-tracker-locale-too-long = Locale code is too long (max { $max_length } bytes)
err-tracker-locale-invalid = Locale contains invalid characters
err-tracker-port-zero = Port cannot be zero
err-tracker-websocket-port-zero = WebSocket port cannot be zero

# Rate / capacity
err-tracker-rate-limited = Rate limit exceeded; try again later
err-tracker-refresh-unknown = Registration is no longer active; reconnect to register again
err-tracker-capacity = Tracker is at capacity; try again later
err-tracker-per-ip-capacity = Too many entries from your IP on this tracker

# Protocol-level
err-tracker-malformed-message = Malformed message
err-tracker-handshake-required = Handshake required before any other message
err-tracker-role-violation = Message not allowed for this connection's role
err-tracker-protocol-version-mismatch = Incompatible tracker protocol version (server: { $server }, client: { $client })
err-tracker-handshake-version-invalid = Invalid handshake version (must be valid semver)
err-tracker-unknown-message-type = Unknown message type
err-tracker-unexpected-message-type = Unexpected message type

# Frame / transport
err-tracker-frame-error = Frame format violation
err-tracker-payload-too-large = Payload exceeds the per-message-type limit
