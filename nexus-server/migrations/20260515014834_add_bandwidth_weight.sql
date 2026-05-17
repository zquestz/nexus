ALTER TABLE users ADD COLUMN bandwidth_weight INTEGER;
ALTER TABLE groups ADD COLUMN bandwidth_weight INTEGER NOT NULL DEFAULT 1;
INSERT INTO config (key, value) VALUES ('max_outbound_rate', '0');
INSERT INTO config (key, value) VALUES ('scheduler_chunk_size', '8192');
