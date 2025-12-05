use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{postgres::PgRow, FromRow, Row};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct RelevantTweet {
    pub id: String,
    pub author_id: String,
    pub text: String,
    pub impression_count: i32,
    pub reply_count: i32,
    pub retweet_count: i32,
    pub like_count: i32,
    pub created_at: DateTime<Utc>,
    pub fetched_at: Option<DateTime<Utc>>,
}

// Manual implementation of FromRow to handle the mapping precisely
impl<'r> FromRow<'r, PgRow> for RelevantTweet {
    fn from_row(row: &'r PgRow) -> Result<Self, sqlx::Error> {
        Ok(Self {
            id: row.try_get("id")?,
            author_id: row.try_get("author_id")?,
            text: row.try_get("text")?,
            impression_count: row.try_get("impression_count")?,
            reply_count: row.try_get("reply_count")?,
            retweet_count: row.try_get("retweet_count")?,
            like_count: row.try_get("like_count")?,
            created_at: row.try_get("created_at")?,
            fetched_at: row.try_get("fetched_at")?,
        })
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TweetSortColumn {
    CreatedAt,
    ImpressionCount,
    ReplyCount,
    RetweetCount,
    LikeCount,
    AuthorId,
    AuthorUsername,
    AuthorName,
}

impl TweetSortColumn {
    pub fn to_sql_column(&self) -> &'static str {
        match self {
            TweetSortColumn::CreatedAt => "rt.created_at",
            TweetSortColumn::ImpressionCount => "rt.impression_count",
            TweetSortColumn::ReplyCount => "rt.reply_count",
            TweetSortColumn::RetweetCount => "rt.retweet_count",
            TweetSortColumn::LikeCount => "rt.like_count",
            TweetSortColumn::AuthorId => "rt.author_id",
            TweetSortColumn::AuthorUsername => "ta.author_username",
            TweetSortColumn::AuthorName => "ta.author_name",
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct TweetFilter {
    pub author_id: Option<String>,
    pub min_likes: Option<i32>,
    pub min_impressions: Option<i32>,
    pub created_after: Option<DateTime<Utc>>,
}

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct TweetWithAuthor {
    #[sqlx(flatten)]
    pub tweet: RelevantTweet,
    pub author_name: String,
    pub author_username: String,
}

#[derive(Debug, Deserialize)]
pub struct NewTweetPayload {
    pub id: String,
    pub author_id: String,
    pub text: String,
    pub impression_count: i32,
    pub reply_count: i32,
    pub retweet_count: i32,
    pub like_count: i32,
    pub created_at: DateTime<Utc>,
}
