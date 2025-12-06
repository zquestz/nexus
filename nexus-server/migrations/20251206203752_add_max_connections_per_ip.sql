-- Add default max connections per IP configuration
-- This limits concurrent connections from a single IP address for DoS protection

INSERT INTO server_config (key, value) 
VALUES ('max_connections_per_ip', '5');