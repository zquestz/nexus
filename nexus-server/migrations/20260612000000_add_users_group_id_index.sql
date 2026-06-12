-- Index users.group_id: speeds up group-member queries (weight cascade,
-- member-count, delete-eligibility) and the ON DELETE RESTRICT foreign-key
-- enforcement when a group row is deleted, both of which otherwise scan users.
CREATE INDEX IF NOT EXISTS idx_users_group_id ON users(group_id);
