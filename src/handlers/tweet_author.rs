use axum::{
    extract::{self, Query, State},
    Extension, Json,
};

use crate::{
    db_persistence::DbError,
    handlers::{calculate_total_pages, ListQueryParams, PaginatedResponse, PaginationMetadata, SuccessResponse},
    http_server::AppState,
    models::{
        admin::Admin,
        tweet_author::{AuthorFilter, AuthorSortColumn, TweetAuthor},
    },
    AppError,
};

/// GET /tweet-authors
/// Lists authors with pagination, sorting, and filtering
pub async fn handle_get_tweet_authors(
    State(state): State<AppState>,
    Extension(_): Extension<Admin>,
    Query(params): Query<ListQueryParams<AuthorSortColumn>>,
    Query(filters): Query<AuthorFilter>,
) -> Result<Json<PaginatedResponse<TweetAuthor>>, AppError> {
    let total_items = state.db.tweet_authors.count_filtered(&params, &filters).await? as u32;
    let total_pages = calculate_total_pages(params.page_size, total_items);

    let authors = state.db.tweet_authors.find_all(&params, &filters).await?;

    let response = PaginatedResponse::<TweetAuthor> {
        data: authors,
        meta: PaginationMetadata {
            page: params.page,
            page_size: params.page_size,
            total_items,
            total_pages,
        },
    };

    Ok(Json(response))
}

/// GET /tweet-authors/:id
/// Gets a single author by their X ID
pub async fn handle_get_tweet_author_by_id(
    State(state): State<AppState>,
    extract::Path(id): extract::Path<String>,
) -> Result<Json<SuccessResponse<TweetAuthor>>, AppError> {
    tracing::info!("Getting tweet author by id {}", id);

    let author = state
        .db
        .tweet_authors
        .find_by_id(&id)
        .await?
        .ok_or_else(|| AppError::Database(DbError::RecordNotFound(format!("Tweet Author {} not found", id))))?;

    Ok(SuccessResponse::new(author))
}
