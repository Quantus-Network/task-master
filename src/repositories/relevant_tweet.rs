use std::collections::HashSet;

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
    pub async fn upsert_many(&self, tweets: &Vec<NewTweetPayload>) -> DbResult<u64> {
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
            ids.push(t.id.clone());
            author_ids.push(t.author_id.clone());
            texts.push(t.text.clone());
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

    pub async fn get_existing_ids(&self, ids: &[String]) -> DbResult<HashSet<String>> {
        if ids.is_empty() {
            return Ok(HashSet::new());
        }

        let rows: Vec<String> = sqlx::query_scalar("SELECT id FROM relevant_tweets WHERE id = ANY($1)")
            .bind(ids)
            .fetch_all(&self.pool)
            .await?;

        Ok(rows.into_iter().collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        models::{relevant_tweet::NewTweetPayload, tweet_author::NewAuthorPayload},
        repositories::tweet_author::TweetAuthorRepository,
        utils::test_db::reset_database,
        Config,
    };
    use chrono::Utc;
    use sqlx::PgPool;

    // --- Helpers to create dummy data ---
    async fn setup_test_repository() -> (RelevantTweetRepository, TweetAuthorRepository) {
        let config = Config::load_test_env().expect("Failed to load configuration for tests");
        let pool = PgPool::connect(config.get_database_url())
            .await
            .expect("Failed to create pool.");

        // Clean database before each test
        reset_database(&pool).await;

        (RelevantTweetRepository::new(&pool), TweetAuthorRepository::new(&pool))
    }

    async fn seed_author(repo: &TweetAuthorRepository, id: &str, username: &str) {
        let authors = vec![NewAuthorPayload {
            id: id.to_string(),
            name: username.to_string(),
            username: username.to_string(),
            followers_count: 100,
            following_count: 10,
            tweet_count: 50,
            listed_count: 1,
            like_count: 200,
            media_count: 5,
            is_ignored: Some(true),
        }];

        repo.upsert_many(&authors).await.expect("Failed to seed authors");
    }

    fn create_payload(id: &str, author_id: &str, text: &str) -> NewTweetPayload {
        NewTweetPayload {
            id: id.to_string(),
            author_id: author_id.to_string(),
            text: text.to_string(),
            impression_count: 100,
            reply_count: 5,
            retweet_count: 10,
            like_count: 50,
            created_at: Utc::now(),
        }
    }

    // --- Tests ---

    #[tokio::test]
    async fn test_upsert_and_find_by_id() {
        let (repo, author_repo) = setup_test_repository().await;

        // 1. Setup Author (Foreign Key constraint)
        let author_id = "author_1";
        seed_author(&author_repo, author_id, "rust_dev").await;

        // 2. Prepare Payload
        let tweet_id = "tweet_123";
        let payload = create_payload(tweet_id, author_id, "Hello Rust!");
        let tweets = vec![payload.clone()];

        // 3. Test Upsert (Insert)
        let count = repo.upsert_many(&tweets).await.unwrap();
        assert_eq!(count, 1, "Should insert 1 row");

        // 4. Test Find
        let found = repo.find_by_id(tweet_id).await.unwrap();
        assert!(found.is_some());
        let t = found.unwrap();
        assert_eq!(t.text, "Hello Rust!");
        assert_eq!(t.impression_count, 100);

        // 5. Test Upsert (Update)
        // Change metrics and upsert again
        let mut update_payload = payload.clone();
        update_payload.impression_count = 999;
        repo.upsert_many(&vec![update_payload]).await.unwrap();

        let updated = repo.find_by_id(tweet_id).await.unwrap().unwrap();
        assert_eq!(
            updated.impression_count, 999,
            "Should update existing record on conflict"
        );
    }

    #[tokio::test]
    async fn test_get_existing_ids() {
        let (repo, author_repo) = setup_test_repository().await;
        seed_author(&author_repo, "a1", "user1").await;

        let t1 = create_payload("t1", "a1", "one");
        let t2 = create_payload("t2", "a1", "two");
        repo.upsert_many(&vec![t1, t2]).await.unwrap();

        // Check for t1, t2 and a non-existent t3
        let check_ids = vec!["t1".to_string(), "t2".to_string(), "t3".to_string()];
        let existing = repo.get_existing_ids(&check_ids).await.unwrap();

        assert_eq!(existing.len(), 2);
        assert!(existing.contains("t1"));
        assert!(existing.contains("t2"));
        assert!(!existing.contains("t3"));
    }
}
