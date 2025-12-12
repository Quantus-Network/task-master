use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{postgres::PgRow, FromRow, Row};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct RaiderInfo {
    pub address: String,
    pub referral_code: Option<String>,
    pub referrals_count: i32,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct RaidLeaderboard {
    pub raid_id: i32,
    pub rank: Option<i64>,
    pub raider: RaiderInfo,
    pub total_submissions: Option<i64>,
    pub total_impressions: Option<i64>,
    pub total_replies: Option<i64>,
    pub total_retweets: Option<i64>,
    pub total_likes: Option<i64>,
    pub last_activity: Option<DateTime<Utc>>,
}

// Manual implementation to map flat SQL joins to nested struct
impl<'r> FromRow<'r, PgRow> for RaidLeaderboard {
    fn from_row(row: &'r PgRow) -> Result<Self, sqlx::Error> {
        Ok(RaidLeaderboard {
            raid_id: row.try_get("raid_id")?,
            rank: row.try_get("rank")?,
            raider: RaiderInfo {
                address: row.try_get("quan_address")?,
                referral_code: row.try_get("referral_code")?,
                referrals_count: row.try_get::<Option<i32>, _>("referrals_count")?.unwrap_or(0),
            },
            total_submissions: row.try_get("total_submissions")?,
            total_impressions: row.try_get("total_impressions")?,
            total_replies: row.try_get("total_replies")?,
            total_retweets: row.try_get("total_retweets")?,
            total_likes: row.try_get("total_likes")?,
            last_activity: row.try_get("last_activity")?,
        })
    }
}
