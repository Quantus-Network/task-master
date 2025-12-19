CREATE EXTENSION IF NOT EXISTS btree_gist;

CREATE TABLE IF NOT EXISTS raid_quests (
    id INT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    name VARCHAR(255) NOT NULL,
    start_date TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    end_date TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    
    CONSTRAINT enforce_one_active_raid EXCLUDE USING GIST (
        tstzrange(start_date, end_date, '[)') WITH &&
    )
);

DROP TRIGGER IF EXISTS set_timestamp_raid_quests ON raid_quests;

CREATE TRIGGER set_timestamp_raid_quests BEFORE
UPDATE
    ON raid_quests FOR EACH ROW EXECUTE PROCEDURE trigger_set_timestamp ();

CREATE INDEX IF NOT EXISTS idx_raid_quests_start_date ON raid_quests (start_date);

CREATE INDEX IF NOT EXISTS idx_raid_quests_end_date ON raid_quests (end_date);

CREATE TABLE IF NOT EXISTS raid_submissions (
    id VARCHAR(255) PRIMARY KEY,
    raid_id INTEGER NOT NULL REFERENCES raid_quests (id) ON DELETE CASCADE,
    target_id VARCHAR(255) NOT NULL REFERENCES relevant_tweets (id) ON DELETE NO ACTION,
    raider_id VARCHAR(64) NOT NULL REFERENCES addresses (quan_address) ON DELETE NO ACTION,
    impression_count INTEGER DEFAULT 0,
    reply_count INTEGER DEFAULT 0,
    retweet_count INTEGER DEFAULT 0,
    like_count INTEGER DEFAULT 0,
    is_invalid BOOLEAN NOT NULL DEFAULT false,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW (),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW ()
);

DROP TRIGGER IF EXISTS set_timestamp_raid_submissions ON raid_submissions;

CREATE TRIGGER set_timestamp_raid_submissions BEFORE
UPDATE
    ON raid_submissions FOR EACH ROW EXECUTE PROCEDURE trigger_set_timestamp ();

CREATE INDEX IF NOT EXISTS idx_raid_submissions_raider_id ON raid_submissions (raider_id);

CREATE INDEX IF NOT EXISTS idx_raid_submissions_is_invalid ON raid_submissions (is_invalid);

CREATE INDEX IF NOT EXISTS idx_raid_submissions_target_id ON raid_submissions (target_id);

CREATE INDEX IF NOT EXISTS idx_raid_submissions_single_top_rank ON raid_submissions (raid_id, impression_count DESC);

CREATE INDEX IF NOT EXISTS idx_raid_submissions_aggregation ON raid_submissions (raid_id, raider_id) INCLUDE (impression_count);

CREATE INDEX IF NOT EXISTS idx_raid_submissions_created_at ON raid_submissions (created_at DESC);

CREATE MATERIALIZED VIEW raid_leaderboards AS
SELECT
    raid_id,
    raider_id,
    COUNT(id) as total_submissions,
    SUM(impression_count) as total_impressions,
    SUM(reply_count) as total_replies,
    SUM(retweet_count) as total_retweets,
    SUM(like_count) as total_likes,
    MAX(updated_at) as last_activity
FROM
    raid_submissions
WHERE
    is_invalid = false
GROUP BY
    raid_id,
    raider_id;

CREATE UNIQUE INDEX idx_mv_raid_raider_unique ON raid_leaderboards (raid_id, raider_id);

CREATE INDEX idx_mv_rank_impressions ON raid_leaderboards (raid_id, total_impressions DESC);