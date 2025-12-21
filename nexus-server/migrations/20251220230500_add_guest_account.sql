-- Create guest account (disabled by default)
-- The guest account is a shared account that allows passwordless login
-- Admins can enable it through the User Management panel
INSERT INTO users (username, password_hash, is_admin, is_shared, enabled, created_at)
VALUES ('guest', '', 0, 1, 0, strftime('%s', 'now'));