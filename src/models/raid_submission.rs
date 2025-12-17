use chrono::{DateTime, Utc};
use rusx::resources::tweet::{Tweet as TwitterTweet, TweetPublicMetrics};
use serde::{Deserialize, Serialize};
use sqlx::{postgres::PgRow, FromRow, Row};

use crate::models::raid_quest::RaidQuest;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct RaidSubmission {
    pub id: String,
    pub raid_id: i32,
    pub target_id: String,
    pub raider_id: String,
    pub impression_count: i32,
    pub reply_count: i32,
    pub retweet_count: i32,
    pub like_count: i32,
    pub updated_at: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
}

impl<'r> FromRow<'r, PgRow> for RaidSubmission {
    fn from_row(row: &'r PgRow) -> Result<Self, sqlx::Error> {
        let id = row.try_get("id")?;
        let raid_id = row.try_get("raid_id")?;
        let target_id = row.try_get("target_id")?;
        let raider_id = row.try_get("raider_id")?;
        let impression_count: Option<i32> = row.try_get("impression_count")?;
        let reply_count: Option<i32> = row.try_get("reply_count")?;
        let retweet_count: Option<i32> = row.try_get("retweet_count")?;
        let like_count: Option<i32> = row.try_get("like_count")?;
        let updated_at = row.try_get("updated_at")?;
        let created_at = row.try_get("created_at")?;

        Ok(RaidSubmission {
            id,
            raid_id,
            target_id,
            raider_id,
            impression_count: impression_count.unwrap_or(0),
            reply_count: reply_count.unwrap_or(0),
            retweet_count: retweet_count.unwrap_or(0),
            like_count: like_count.unwrap_or(0),
            updated_at,
            created_at,
        })
    }
}

#[derive(Debug, Deserialize, Serialize)]
pub struct CreateRaidSubmission {
    pub id: String,
    pub raid_id: i32,
    pub target_id: String,
    pub raider_id: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RaidSubmissionInput {
    pub target_tweet_link: String,
    pub tweet_reply_link: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct UpdateRaidSubmissionStats {
    pub id: String,
    pub impression_count: i32,
    pub reply_count: i32,
    pub retweet_count: i32,
    pub like_count: i32,
}

impl From<TwitterTweet> for UpdateRaidSubmissionStats {
    fn from(tweet: TwitterTweet) -> Self {
        let public_metrics = tweet
            .public_metrics
            .unwrap_or_else(|| TweetPublicMetrics { ..Default::default() });

        let update_payload = UpdateRaidSubmissionStats {
            id: tweet.id,
            impression_count: public_metrics.impression_count as i32,
            like_count: public_metrics.like_count as i32,
            retweet_count: public_metrics.retweet_count as i32,
            reply_count: public_metrics.reply_count as i32,
        };

        update_payload
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct RaiderSubmissions {
    pub current_raid: RaidQuest,
    pub submissions: Vec<String>,
}
