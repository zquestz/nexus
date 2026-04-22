-- Add public_address config key for advertising a server's shareable nexus:// URI host.
-- Empty default means "unset" — admins set it via the Server Info edit panel.
INSERT INTO config (key, value) VALUES ('public_address', '');