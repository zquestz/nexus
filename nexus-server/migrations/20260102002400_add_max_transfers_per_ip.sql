-- Add default max transfers per IP configuration
-- This limits concurrent file transfer connections from a single IP address

INSERT INTO config (key, value) 
VALUES ('max_transfers_per_ip', '3');