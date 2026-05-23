-- Unicode-folded username key for case-insensitive uniqueness and lookup.
ALTER TABLE users ADD COLUMN username_lower TEXT NOT NULL DEFAULT '';
UPDATE users SET username_lower = lower(username);
CREATE UNIQUE INDEX idx_users_username_lower ON users(username_lower);
