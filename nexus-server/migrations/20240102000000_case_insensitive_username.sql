-- Add case-insensitive unique constraint on usernames
-- This allows usernames to preserve their case (Alice vs alice) but prevents
-- duplicate usernames with different casing from being created

-- Drop the old unique constraint on username
DROP INDEX IF EXISTS idx_users_username;

-- Create a unique index on LOWER(username) to enforce case-insensitive uniqueness
CREATE UNIQUE INDEX idx_users_username_lower ON users(LOWER(username));

-- Create a regular index on username for fast case-sensitive lookups
CREATE INDEX idx_users_username ON users(username);