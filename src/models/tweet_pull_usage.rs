use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use chrono::{DateTime, Utc};

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct TweetPullUsage {
    pub period: String,
    pub tweet_count: i32,
    pub updated_at: DateTime<Utc>,
}

