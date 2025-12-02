-- Add topic_set_by config key to track who set the current topic
-- This allows showing the correct username when a user gains chat_topic permission

INSERT INTO server_config (key, value) 
VALUES ('topic_set_by', '');