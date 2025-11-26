-- Add enabled column to users table
ALTER TABLE users ADD COLUMN enabled BOOLEAN NOT NULL DEFAULT 1;

-- Set all existing users to enabled
UPDATE users SET enabled = 1;