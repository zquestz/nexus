-- Folded nickname lookup key for ip_bans. Nullable (IP-only bans have no
-- nickname) and non-unique (many bans can share a nickname); the display-case
-- `nickname` column is preserved as-typed.
ALTER TABLE ip_bans ADD COLUMN nickname_lower TEXT;
UPDATE ip_bans SET nickname_lower = lower(nickname);
CREATE INDEX idx_ip_bans_nickname_lower ON ip_bans(nickname_lower);
