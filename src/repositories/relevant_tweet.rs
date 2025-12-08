use sqlx::{PgPool, Postgres, QueryBuilder, Row};

use crate::{
    db_persistence::DbError,
    handlers::ListQueryParams,
    models::relevant_tweet::{NewTweetPayload, RelevantTweet, TweetFilter, TweetSortColumn, TweetWithAuthor},
    repositories::{calculate_page_offset, DbResult, QueryBuilderExt},
};

#[derive(Clone, Debug)]
pub struct RelevantTweetRepository {
    pool: PgPool,
}

impl RelevantTweetRepository {
    pub fn new(pool: &PgPool) -> Self {
        Self { pool: pool.clone() }
    }

    fn build_base_query_with_authors<'a>(
        &self,
        query_builder: &mut QueryBuilder<'a, Postgres>,
        search: &Option<String>,
        filters: &TweetFilter,
    ) {
        query_builder.push(" FROM relevant_tweets rt ");
        query_builder.push(" LEFT JOIN tweet_authors ta ON rt.author_id = ta.id ");

        let mut where_started = false;

        // Global Text Search ---
        if let Some(s) = search {
            if !s.is_empty() {
                query_builder.push(" WHERE (");
                where_started = true;

                query_builder.push("text_fts @@ websearch_to_tsquery('english', ");
                query_builder.push_bind(s.clone());
                query_builder.push(")");

                // Allow searching by author username as well
                query_builder.push(" OR ta.username ILIKE ");
                query_builder.push_bind(format!("%{}%", s));
                query_builder.push(") ");
            }
        }

        // Filter: Specific Author
        if let Some(author_username) = filters.author_username.clone() {
            query_builder.push_condition(" ta.username = ", &mut where_started);
            query_builder.push_bind(author_username);
        }

        // Filter: Minimum Likes
        if let Some(min_likes) = filters.min_likes {
            query_builder.push_condition(" rt.like_count >= ", &mut where_started);
            query_builder.push_bind(min_likes);
        }

        // Filter: Minimum Impressions
        if let Some(min_impressions) = filters.min_impressions {
            query_builder.push_condition(" rt.impression_count >= ", &mut where_started);
            query_builder.push_bind(min_impressions);
        }

        // Filter: Created After
        if let Some(created_after) = filters.created_after {
            query_builder.push_condition(" rt.created_at >= ", &mut where_started);
            query_builder.push_bind(created_after);
        }
    }

    /// Count tweets matching the current filters (for pagination)
    pub async fn count_filtered(
        &self,
        params: &ListQueryParams<TweetSortColumn>,
        filters: &TweetFilter,
    ) -> Result<i64, DbError> {
        // We count on 'rt.id' to be specific
        let mut query_builder = QueryBuilder::new("SELECT COUNT(rt.id)");

        self.build_base_query_with_authors(&mut query_builder, &params.search, filters);

        let count = query_builder
            .build_query_scalar()
            .fetch_one(&self.pool)
            .await
            .map_err(|e| DbError::Database(e))?;

        Ok(count)
    }

    pub async fn get_newest_tweet_id(&self) -> Result<Option<String>, DbError> {
        let row = sqlx::query("SELECT id FROM relevant_tweets ORDER BY created_at DESC LIMIT 1")
            .fetch_optional(&self.pool)
            .await?;

        Ok(row.map(|r| r.get("id")))
    }

    /// Find all tweets with author details joined
    pub async fn find_all_with_authors(
        &self,
        params: &ListQueryParams<TweetSortColumn>,
        filters: &TweetFilter,
    ) -> Result<Vec<TweetWithAuthor>, DbError> {
        // Select all tweet columns + author name/username
        // We use aliases that match the TweetWithAuthor struct expectations
        let mut query_builder = QueryBuilder::new(
            r#"
            SELECT 
                rt.*,
                ta.name as author_name,
                ta.username as author_username
            "#,
        );

        self.build_base_query_with_authors(&mut query_builder, &params.search, filters);

        // Sorting
        query_builder.push(" ORDER BY ");
        let sort_col = params.sort_by.as_ref().unwrap_or(&TweetSortColumn::CreatedAt);
        query_builder.push(sort_col.to_sql_column());

        query_builder.push(" ");
        query_builder.push(params.order.to_string());

        // Secondary sort for stability
        query_builder.push(", rt.id ASC");

        // Pagination
        let offset = calculate_page_offset(params.page, params.page_size);
        query_builder.push(" LIMIT ");
        query_builder.push_bind(params.page_size as i64);
        query_builder.push(" OFFSET ");
        query_builder.push_bind(offset as i64);

        let tweets = query_builder
            .build_query_as::<TweetWithAuthor>()
            .fetch_all(&self.pool)
            .await
            .map_err(|e| DbError::Database(e))?;

        Ok(tweets)
    }

    /// Batch Upsert
    pub async fn upsert_many(&self, tweets: Vec<NewTweetPayload>) -> DbResult<u64> {
        if tweets.is_empty() {
            return Ok(0);
        }

        // Deconstruct vector for UNNEST
        let mut ids = Vec::with_capacity(tweets.len());
        let mut author_ids = Vec::with_capacity(tweets.len());
        let mut texts = Vec::with_capacity(tweets.len());
        let mut impression_counts = Vec::with_capacity(tweets.len());
        let mut reply_counts = Vec::with_capacity(tweets.len());
        let mut retweet_counts = Vec::with_capacity(tweets.len());
        let mut like_counts = Vec::with_capacity(tweets.len());
        let mut created_ats = Vec::with_capacity(tweets.len());

        for t in tweets {
            ids.push(t.id);
            author_ids.push(t.author_id);
            texts.push(t.text);
            impression_counts.push(t.impression_count);
            reply_counts.push(t.reply_count);
            retweet_counts.push(t.retweet_count);
            like_counts.push(t.like_count);
            created_ats.push(t.created_at);
        }

        let result = sqlx::query(
            r#"
            INSERT INTO relevant_tweets (
                id, author_id, text, impression_count, reply_count, 
                retweet_count, like_count, created_at, fetched_at
            )
            SELECT *, NOW() FROM UNNEST(
                $1::varchar[], 
                $2::varchar[], 
                $3::text[], 
                $4::int[], 
                $5::int[], 
                $6::int[], 
                $7::int[], 
                $8::timestamptz[]
            )
            ON CONFLICT (id) 
            DO UPDATE SET 
                impression_count = EXCLUDED.impression_count,
                reply_count = EXCLUDED.reply_count,
                retweet_count = EXCLUDED.retweet_count,
                like_count = EXCLUDED.like_count,
                fetched_at = NOW()
            "#,
        )
        .bind(&ids)
        .bind(&author_ids)
        .bind(&texts)
        .bind(&impression_counts)
        .bind(&reply_counts)
        .bind(&retweet_counts)
        .bind(&like_counts)
        .bind(&created_ats)
        .execute(&self.pool)
        .await?;

        Ok(result.rows_affected())
    }

    pub async fn find_by_id(&self, id: &str) -> DbResult<Option<RelevantTweet>> {
        let tweet = sqlx::query_as::<_, RelevantTweet>("SELECT * FROM relevant_tweets WHERE id = $1")
            .bind(id)
            .fetch_optional(&self.pool)
            .await?;

        Ok(tweet)
    }
}
