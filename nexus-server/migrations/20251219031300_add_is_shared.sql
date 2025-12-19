-- Add is_shared column to users table
-- Shared accounts allow multiple users to log in with the same credentials,
-- each providing a unique nickname for identification.
ALTER TABLE users ADD COLUMN is_shared BOOLEAN NOT NULL DEFAULT 0;

-- Shared accounts cannot be admins (enforced in application layer)
-- Shared accounts have restricted permissions (enforced in application layer)
-- Once created as shared, an account cannot be converted to regular (enforced in application layer)