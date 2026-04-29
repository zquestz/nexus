# Tracker error messages (v0.1.0)
#
# All keys use the `err-tracker-*` prefix to keep them isolated from the
# nexus-server locale namespace. Keys correspond 1:1 with `err_tracker_*`
# helpers in `nexus-tracker/src/errors.rs`.

# Authentication
err-tracker-unauthorized = Wrong or missing password

# Field validation (error_kind: invalid)
err-tracker-fingerprint-invalid = Invalid certificate fingerprint format
err-tracker-name-too-long = Server name is too long (max { $max_length } bytes)
err-tracker-description-too-long = Server description is too long (max { $max_length } bytes)
err-tracker-password-too-long = Password is too long (max { $max_length } bytes)
err-tracker-address-too-long = Address is too long (max { $max_length } bytes)
err-tracker-address-invalid = Invalid address
err-tracker-version-too-long = Server version string is too long (max { $max_length } bytes)
err-tracker-locale-too-long = Locale code is too long (max { $max_length } bytes)

# Rate / capacity
err-tracker-rate-limited = Rate limit exceeded; try again later
err-tracker-capacity = Tracker is at capacity; try again later
err-tracker-per-ip-capacity = Too many entries from your IP on this tracker

# Protocol-level
err-tracker-malformed-message = Malformed message
err-tracker-handshake-required = Handshake required before any other message
err-tracker-role-violation = Message not allowed for this connection's role
err-tracker-protocol-version-mismatch = Incompatible tracker protocol version (server: { $server }, client: { $client })
err-tracker-handshake-version-invalid = Invalid handshake version (must be valid semver)
err-tracker-unknown-message-type = Unknown message type

# Frame / transport
err-tracker-frame-error = Frame format violation
err-tracker-payload-too-large = Payload exceeds the per-message-type limit
