-- Create groups table for permission group templates
CREATE TABLE IF NOT EXISTS groups (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    name TEXT NOT NULL UNIQUE,
    is_shared BOOLEAN NOT NULL DEFAULT 0
);

-- Case-insensitive unique index on group name (same pattern as users)
CREATE UNIQUE INDEX idx_groups_name_lower ON groups(LOWER(name));

-- Base permissions for each group
CREATE TABLE IF NOT EXISTS group_permissions (
    group_id INTEGER NOT NULL,
    permission TEXT NOT NULL,
    PRIMARY KEY (group_id, permission),
    FOREIGN KEY (group_id) REFERENCES groups(id) ON DELETE CASCADE
);

-- Index for faster permission lookups by group
CREATE INDEX IF NOT EXISTS idx_group_permissions_group_id ON group_permissions(group_id);

-- Add optional group FK to users (ON DELETE SET NULL as safety net)
ALTER TABLE users ADD COLUMN group_id INTEGER REFERENCES groups(id) ON DELETE SET NULL;

-- Add override type to user_permissions for group override support
-- Existing rows get 'grant' (backward compatible legacy behavior)
-- New revoke overrides stored as 'revoke'
ALTER TABLE user_permissions ADD COLUMN override_type TEXT NOT NULL DEFAULT 'grant';