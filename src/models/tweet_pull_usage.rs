use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct TweetPullUsage {
    pub period: String,
    pub tweet_count: i32,
    pub updated_at: DateTime<Utc>,
}
