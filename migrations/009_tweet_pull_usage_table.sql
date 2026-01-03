-- Table to track Twitter API usage for the monthly cap
CREATE TABLE IF NOT EXISTS tweet_pull_usage (
    period VARCHAR(7) PRIMARY KEY, -- Format: YYYY-MM
    tweet_count INTEGER DEFAULT 0,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Trigger for updated_at
DROP TRIGGER IF EXISTS set_timestamp ON tweet_pull_usage;
CREATE TRIGGER set_timestamp
BEFORE UPDATE ON tweet_pull_usage
FOR EACH ROW
EXECUTE PROCEDURE trigger_set_timestamp();

