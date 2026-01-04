ALTER TABLE tweet_authors 
ADD COLUMN is_ignored BOOLEAN NOT NULL DEFAULT true;

CREATE INDEX IF NOT EXISTS idx_tweet_authors_is_ignored ON tweet_authors(is_ignored) 
WHERE is_ignored = false;