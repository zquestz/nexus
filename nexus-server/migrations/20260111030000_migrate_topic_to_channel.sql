-- Migrate legacy topic from chat_state table to channel_settings
-- This completes the transition from single-channel to multi-channel chat

-- Copy topic from chat_state to #nexus channel_settings (if topic exists and is non-empty)
UPDATE channel_settings
SET 
    topic = COALESCE((SELECT value FROM chat_state WHERE key = 'topic'), ''),
    topic_set_by = COALESCE((SELECT value FROM chat_state WHERE key = 'topic_set_by'), '')
WHERE LOWER(name) = '#nexus'
    AND EXISTS (SELECT 1 FROM chat_state WHERE key = 'topic' AND value != '');

-- Remove the legacy topic keys from chat_state
DELETE FROM chat_state WHERE key IN ('topic', 'topic_set_by');