use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{postgres::PgRow, FromRow, Row};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct RaidLeaderboard {
    pub raid_id: i32,
    pub raider_id: String,
    pub total_submissions: i64,
    pub total_impressions: i64,
    pub total_replies: i64,
    pub total_retweets: i64,
    pub total_likes: i64,
    pub last_activity: Option<DateTime<Utc>>,
}

impl<'r> FromRow<'r, PgRow> for RaidLeaderboard {
    fn from_row(row: &'r PgRow) -> Result<Self, sqlx::Error> {
        let raid_id = row.try_get("raid_id")?;
        let raider_id = row.try_get("raider_id")?;
        let total_submissions: Option<i64> = row.try_get("total_submissions")?;
        let total_impressions: Option<i64> = row.try_get("total_impressions")?;
        let total_replies: Option<i64> = row.try_get("total_replies")?;
        let total_retweets: Option<i64> = row.try_get("total_retweets")?;
        let total_likes: Option<i64> = row.try_get("total_likes")?;
        let last_activity = row.try_get("last_activity")?;

        Ok(RaidLeaderboard {
            raid_id,
            raider_id,
            total_submissions: total_submissions.unwrap_or(0),
            total_impressions: total_impressions.unwrap_or(0),
            total_replies: total_replies.unwrap_or(0),
            total_retweets: total_retweets.unwrap_or(0),
            total_likes: total_likes.unwrap_or(0),
            last_activity,
        })
    }
}
