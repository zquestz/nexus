-- Folded nickname lookup key for ip_trusted. Nullable (IP-only trusts have no
-- nickname) and non-unique (many trusts can share a nickname); the display-case
-- `nickname` column is preserved as-typed.
ALTER TABLE ip_trusted ADD COLUMN nickname_lower TEXT;
UPDATE ip_trusted SET nickname_lower = lower(nickname);
CREATE INDEX idx_ip_trusted_nickname_lower ON ip_trusted(nickname_lower);
