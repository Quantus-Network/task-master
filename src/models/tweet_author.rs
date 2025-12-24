use chrono::{DateTime, Utc};
use rusx::resources::user::User as TwitterUser;
use serde::{Deserialize, Serialize};
use sqlx::{postgres::PgRow, FromRow, Row};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct TweetAuthor {
    pub id: String,
    pub name: String,
    pub username: String,
    pub is_ignored: bool,
    pub followers_count: i32,
    pub following_count: i32,
    pub tweet_count: i32,
    pub listed_count: i32,
    pub like_count: i32,
    pub media_count: i32,
    pub fetched_at: Option<DateTime<Utc>>,
}
impl<'r> FromRow<'r, PgRow> for TweetAuthor {
    fn from_row(row: &'r PgRow) -> Result<Self, sqlx::Error> {
        Ok(Self {
            id: row.try_get("id")?,
            name: row.try_get("name")?,
            username: row.try_get("username")?,
            is_ignored: row.try_get("is_ignored")?,
            followers_count: row.try_get("followers_count")?,
            following_count: row.try_get("following_count")?,
            tweet_count: row.try_get("tweet_count")?,
            listed_count: row.try_get("listed_count")?,
            like_count: row.try_get("like_count")?,
            media_count: row.try_get("media_count")?,
            fetched_at: row.try_get("fetched_at")?,
        })
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuthorSortColumn {
    FollowersCount,
    FollowingCount,
    LikeCount,
    TweetCount,
    ListedCount,
    Username,
    FetchedAt,
}

impl AuthorSortColumn {
    pub fn to_sql_column(&self) -> &'static str {
        match self {
            AuthorSortColumn::FollowersCount => "ta.followers_count",
            AuthorSortColumn::FollowingCount => "ta.following_count",
            AuthorSortColumn::LikeCount => "ta.like_count",
            AuthorSortColumn::TweetCount => "ta.tweet_count",
            AuthorSortColumn::ListedCount => "ta.listed_count",
            AuthorSortColumn::Username => "ta.username",
            AuthorSortColumn::FetchedAt => "ta.fetched_at",
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct AuthorFilter {
    pub min_followers: Option<i32>,
    pub min_likes: Option<i32>,
}

// Payload for creating/updating authors from the X API
#[derive(Debug, Deserialize, Clone)]
pub struct NewAuthorPayload {
    pub id: String,
    pub name: String,
    pub username: String,
    pub followers_count: i32,
    pub following_count: i32,
    pub tweet_count: i32,
    pub listed_count: i32,
    pub like_count: i32,
    pub media_count: i32,
}

impl NewAuthorPayload {
    pub fn new(author: TwitterUser) -> Self {
        let public_metrics = author.public_metrics.unwrap_or_default();

        let new_author = NewAuthorPayload {
            id: author.id,
            name: author.name,
            username: author.username,
            followers_count: public_metrics.followers_count as i32,
            following_count: public_metrics.following_count as i32,
            tweet_count: public_metrics.tweet_count as i32,
            listed_count: public_metrics.listed_count as i32,
            media_count: public_metrics.media_count.unwrap_or(0) as i32,
            like_count: public_metrics.like_count.unwrap_or(0) as i32,
        };

        new_author
    }
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct CreateTweetAuthorInput {
    pub username: String,
}
