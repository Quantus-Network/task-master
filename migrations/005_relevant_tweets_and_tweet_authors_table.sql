CREATE TABLE IF NOT EXISTS tweet_authors (
    -- The Author ID from X
    id VARCHAR(255) PRIMARY KEY,
    name VARCHAR(255) NOT NULL,
    username VARCHAR(255) NOT NULL,
    followers_count INTEGER DEFAULT 0,
    following_count INTEGER DEFAULT 0,
    tweet_count INTEGER DEFAULT 0,
    listed_count INTEGER DEFAULT 0,
    like_count INTEGER DEFAULT 0,
    media_count INTEGER DEFAULT 0,
    fetched_at TIMESTAMPTZ DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_tweet_authors_followers ON tweet_authors(followers_count DESC);

CREATE INDEX IF NOT EXISTS idx_tweet_authors_username ON tweet_authors(username);

CREATE TABLE IF NOT EXISTS relevant_tweets (
    -- The Tweet ID from X
    id VARCHAR(255) PRIMARY KEY,
    author_id VARCHAR(255) NOT NULL REFERENCES tweet_authors(id) ON DELETE NO ACTION,
    text TEXT NOT NULL,
    text_fts tsvector GENERATED ALWAYS AS (to_tsvector('english', text)) STORED,
    impression_count INTEGER DEFAULT 0,
    reply_count INTEGER DEFAULT 0,
    retweet_count INTEGER DEFAULT 0,
    like_count INTEGER DEFAULT 0,
    created_at TIMESTAMPTZ NOT NULL,
    fetched_at TIMESTAMPTZ DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_relevant_tweets_created_at ON relevant_tweets(created_at DESC);

CREATE INDEX IF NOT EXISTS idx_relevant_tweets_impressions ON relevant_tweets(impression_count DESC);

CREATE INDEX IF NOT EXISTS idx_relevant_tweets_fts ON relevant_tweets USING GIN(text_fts);