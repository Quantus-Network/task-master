use axum::{
    extract::{self, Query, State},
    Json,
};

use crate::{
    db_persistence::DbError,
    handlers::{
        calculate_total_pages, validate_pagination_query, ListQueryParams, PaginatedResponse, PaginationMetadata,
        SuccessResponse,
    },
    http_server::AppState,
    models::revelant_tweet::{NewTweetPayload, RelevantTweet, TweetFilter, TweetSortColumn, TweetWithAuthor},
    AppError,
};

/// GET /relevant-tweets
/// Lists tweets with pagination, filtering, and joined author details.
pub async fn handle_get_relevant_tweets(
    State(state): State<AppState>,
    Query(params): Query<ListQueryParams<TweetSortColumn>>,
    Query(filters): Query<TweetFilter>,
) -> Result<Json<PaginatedResponse<TweetWithAuthor>>, AppError> {
    validate_pagination_query(params.page, params.page_size)?;

    let total_items = state.db.relevant_tweets.count_filtered(&params, &filters).await? as u32;
    let total_pages = calculate_total_pages(params.page_size, total_items);

    let tweets = state
        .db
        .relevant_tweets
        .find_all_with_authors(&params, &filters)
        .await?;

    let response = PaginatedResponse::<TweetWithAuthor> {
        data: tweets,
        meta: PaginationMetadata {
            page: params.page,
            page_size: params.page_size,
            total_items,
            total_pages,
        },
    };

    Ok(Json(response))
}

/// GET /relevant-tweets/:id
/// Gets a single relevant tweet by its ID
pub async fn handle_get_relevant_tweet_by_id(
    State(state): State<AppState>,
    extract::Path(id): extract::Path<String>,
) -> Result<Json<SuccessResponse<RelevantTweet>>, AppError> {
    tracing::info!("Getting relevant tweet by id {}", id);

    let tweet = state.db.relevant_tweets.find_by_id(&id).await?.ok_or_else(|| {
        // Assuming you have a generic not found error, or define a specific one
        AppError::Database(DbError::AddressNotFound(format!("Tweet {} not found", id)))
    })?;

    Ok(SuccessResponse::new(tweet))
}

/// POST /relevant-tweets/sync
/// Batch upsert tweets (useful for background workers syncing from X API)
pub async fn handle_sync_relevant_tweets(
    State(state): State<AppState>,
    Json(payload): Json<Vec<NewTweetPayload>>,
) -> Result<Json<SuccessResponse<String>>, AppError> {
    tracing::info!("Syncing {} relevant tweets", payload.len());

    let count = state.db.relevant_tweets.upsert_many(payload).await?;

    Ok(SuccessResponse::new(format!("Successfully synced {} tweets", count)))
}
