-- Create news table for news posts
CREATE TABLE IF NOT EXISTS news (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    body TEXT,
    image TEXT,
    author_id INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    created_at TEXT NOT NULL,
    updated_at TEXT,
    CHECK (body IS NOT NULL OR image IS NOT NULL)
);

-- Index for faster author lookups and cascade deletes
CREATE INDEX IF NOT EXISTS idx_news_author_id ON news(author_id);

-- Index for ordering by creation time
CREATE INDEX IF NOT EXISTS idx_news_created_at ON news(created_at);