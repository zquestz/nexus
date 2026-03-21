-- Drop redundant index on group_permissions.group_id
-- The composite PRIMARY KEY (group_id, permission) already covers group_id lookups.
DROP INDEX IF EXISTS idx_group_permissions_group_id;