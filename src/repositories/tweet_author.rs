use sqlx::{PgPool, Postgres, QueryBuilder};

use crate::{
    db_persistence::DbError,
    handlers::ListQueryParams,
    // Make sure these imports match where you put the Author models
    models::tweet_author::{AuthorFilter, AuthorSortColumn, NewAuthorPayload, TweetAuthor},
    repositories::{calculate_page_offset, DbResult, QueryBuilderExt},
};

#[derive(Clone, Debug)]
pub struct TweetAuthorRepository {
    pool: PgPool,
}

impl TweetAuthorRepository {
    pub fn new(pool: &PgPool) -> Self {
        Self { pool: pool.clone() }
    }

    /// Helper to build the dynamic WHERE clause for Authors
    fn build_base_query<'a>(
        &self,
        query_builder: &mut QueryBuilder<'a, Postgres>,
        search: &Option<String>,
        filters: &AuthorFilter,
    ) {
        query_builder.push(" FROM tweet_authors ta ");

        let mut where_started = false;

        // ---  Global Text Search ---
        if let Some(s) = search {
            if !s.is_empty() {
                // Use the helper trait
                query_builder.push_condition(" (ta.username ILIKE ", &mut where_started);
                query_builder.push_bind(format!("%{}%", s));

                query_builder.push(" OR ta.name ILIKE ");
                query_builder.push_bind(format!("%{}%", s));
                query_builder.push(") ");
            }
        }

        if let Some(min_likes) = filters.min_likes {
            query_builder.push_condition(" ta.like_count >= ", &mut where_started);
            query_builder.push_bind(min_likes);
        }

        if let Some(min_followers) = filters.min_followers {
            query_builder.push_condition(" ta.followers_count >= ", &mut where_started);
            query_builder.push_bind(min_followers);
        }
    }

    pub async fn count_filtered(
        &self,
        params: &ListQueryParams<AuthorSortColumn>,
        filters: &AuthorFilter,
    ) -> Result<i64, DbError> {
        let mut query_builder = QueryBuilder::new("SELECT COUNT(ta.id)");

        self.build_base_query(&mut query_builder, &params.search, filters);

        let count = query_builder
            .build_query_scalar()
            .fetch_one(&self.pool)
            .await
            .map_err(|e| DbError::Database(e))?;

        Ok(count)
    }

    pub async fn find_all(
        &self,
        params: &ListQueryParams<AuthorSortColumn>,
        filters: &AuthorFilter,
    ) -> Result<Vec<TweetAuthor>, DbError> {
        let mut query_builder = QueryBuilder::new("SELECT *");

        self.build_base_query(&mut query_builder, &params.search, filters);

        // Sorting
        query_builder.push(" ORDER BY ");
        let sort_col = params.sort_by.as_ref().unwrap_or(&AuthorSortColumn::FollowersCount);

        query_builder.push(sort_col.to_sql_column());
        query_builder.push(" ");
        query_builder.push(params.order.to_string());

        // Secondary sort for stability
        query_builder.push(", ta.id ASC");

        // Pagination
        let offset = calculate_page_offset(params.page, params.page_size);
        query_builder.push(" LIMIT ");
        query_builder.push_bind(params.page_size as i64);
        query_builder.push(" OFFSET ");
        query_builder.push_bind(offset as i64);

        let authors = query_builder
            .build_query_as::<TweetAuthor>()
            .fetch_all(&self.pool)
            .await
            .map_err(|e| DbError::Database(e))?;

        Ok(authors)
    }

    pub async fn get_whitelist(&self) -> Result<Vec<String>, DbError> {
        let ids = sqlx::query_scalar::<_, String>("SELECT id FROM tweet_authors WHERE is_ignored = false")
            .fetch_all(&self.pool)
            .await?;

        Ok(ids)
    }

    pub async fn set_ignore_status(&self, id: &str, status: bool) -> Result<(), DbError> {
        sqlx::query("UPDATE tweet_authors SET is_ignored = $1 WHERE id = $2")
            .bind(status)
            .bind(id)
            .execute(&self.pool)
            .await?;

        Ok(())
    }

    pub async fn upsert(&self, payload: &NewAuthorPayload) -> DbResult<String> {
        let id = sqlx::query_scalar::<_, String>(
            r#"
            INSERT INTO tweet_authors (
                id, name, username, is_ignored, followers_count, following_count, 
                tweet_count, listed_count, like_count, media_count, fetched_at
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, NOW())
            ON CONFLICT (id) DO UPDATE SET
                name = EXCLUDED.name,
                username = EXCLUDED.username,
                is_ignored = EXCLUDED.is_ignored,
                followers_count = EXCLUDED.followers_count,
                following_count = EXCLUDED.following_count,
                tweet_count = EXCLUDED.tweet_count,
                listed_count = EXCLUDED.listed_count,
                like_count = EXCLUDED.like_count,
                media_count = EXCLUDED.media_count,
                fetched_at = NOW()
            RETURNING id
            "#,
        )
        .bind(&payload.id)
        .bind(&payload.name)
        .bind(&payload.username)
        .bind(&payload.is_ignored)
        .bind(payload.followers_count)
        .bind(payload.following_count)
        .bind(payload.tweet_count)
        .bind(payload.listed_count)
        .bind(payload.like_count)
        .bind(payload.media_count)
        .fetch_one(&self.pool)
        .await?;

        Ok(id)
    }

    /// Batch Upsert for Authors
    pub async fn upsert_many(&self, authors: &Vec<NewAuthorPayload>) -> DbResult<u64> {
        if authors.is_empty() {
            return Ok(0);
        }

        let mut ids = Vec::with_capacity(authors.len());
        let mut names = Vec::with_capacity(authors.len());
        let mut usernames = Vec::with_capacity(authors.len());
        let mut followers_counts = Vec::with_capacity(authors.len());
        let mut following_counts = Vec::with_capacity(authors.len());
        let mut tweet_counts = Vec::with_capacity(authors.len());
        let mut listed_counts = Vec::with_capacity(authors.len());
        let mut like_counts = Vec::with_capacity(authors.len());
        let mut media_counts = Vec::with_capacity(authors.len());

        for a in authors {
            ids.push(a.id.clone());
            names.push(a.name.clone());
            usernames.push(a.username.clone());
            followers_counts.push(a.followers_count);
            following_counts.push(a.following_count);
            tweet_counts.push(a.tweet_count);
            listed_counts.push(a.listed_count);
            like_counts.push(a.like_count);
            media_counts.push(a.media_count);
        }

        let result = sqlx::query(
            r#"
            INSERT INTO tweet_authors (
                id, name, username, followers_count, following_count, 
                tweet_count, listed_count, like_count, media_count, fetched_at
            )
            SELECT *, NOW() FROM UNNEST(
                $1::varchar[], 
                $2::varchar[], 
                $3::varchar[], 
                $4::int[], 
                $5::int[], 
                $6::int[], 
                $7::int[], 
                $8::int[],
                $9::int[]
            )
            ON CONFLICT (id) 
            DO UPDATE SET 
                name = EXCLUDED.name,
                username = EXCLUDED.username,
                followers_count = EXCLUDED.followers_count,
                following_count = EXCLUDED.following_count,
                tweet_count = EXCLUDED.tweet_count,
                listed_count = EXCLUDED.listed_count,
                like_count = EXCLUDED.like_count,
                media_count = EXCLUDED.media_count,
                fetched_at = NOW()
            "#,
        )
        .bind(&ids)
        .bind(&names)
        .bind(&usernames)
        .bind(&followers_counts)
        .bind(&following_counts)
        .bind(&tweet_counts)
        .bind(&listed_counts)
        .bind(&like_counts)
        .bind(&media_counts)
        .execute(&self.pool)
        .await?;

        Ok(result.rows_affected())
    }

    pub async fn find_by_id(&self, id: &str) -> DbResult<Option<TweetAuthor>> {
        let author = sqlx::query_as::<_, TweetAuthor>("SELECT * FROM tweet_authors WHERE id = $1")
            .bind(id)
            .fetch_optional(&self.pool)
            .await?;

        Ok(author)
    }
}
