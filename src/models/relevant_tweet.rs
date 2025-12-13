use chrono::{DateTime, Utc};
use rusx::resources::tweet::{Tweet as TwitterTweet, TweetPublicMetrics};
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
            TweetSortColumn::AuthorUsername => "ta.username",
            TweetSortColumn::AuthorName => "ta.name",
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct TweetFilter {
    pub author_username: Option<String>,
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

#[derive(Debug, Clone, Deserialize)]
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

impl NewTweetPayload {
    pub fn new(tweet: TwitterTweet) -> Self {
        let public_metrics = tweet
            .public_metrics
            .ok_or_else(|| TweetPublicMetrics { ..Default::default() })
            .unwrap();
        let created_at = tweet.created_at.ok_or_else(|| chrono::Utc::now().to_rfc3339()).unwrap();

        let new_tweet = NewTweetPayload {
            id: tweet.id,
            author_id: tweet.author_id.unwrap(),
            text: tweet.text,
            impression_count: public_metrics.impression_count as i32,
            like_count: public_metrics.like_count as i32,
            retweet_count: public_metrics.retweet_count as i32,
            reply_count: public_metrics.reply_count as i32,
            created_at: DateTime::parse_from_rfc3339(&created_at).unwrap().with_timezone(&Utc),
        };

        new_tweet
    }
}
