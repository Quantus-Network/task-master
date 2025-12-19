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

#[cfg(test)]
mod tests {
    use axum::{body::Body, extract::Request, http::StatusCode, routing::get, Extension, Router};
    use serde_json::Value;
    use tower::ServiceExt;

    use crate::{
        handlers::tweet_author::{handle_get_tweet_author_by_id, handle_get_tweet_authors},
        models::tweet_author::NewAuthorPayload,
        utils::{
            test_app_state::create_test_app_state,
            test_db::{create_mock_admin, reset_database},
        },
    };

    // --- Helper to seed authors easily ---
    async fn seed_authors(state: &crate::http_server::AppState) {
        let authors = vec![
            NewAuthorPayload {
                id: "auth_1".to_string(),
                name: "Alpha User".to_string(),
                username: "alpha_user".to_string(),
                followers_count: 100,
                following_count: 10,
                tweet_count: 50,
                listed_count: 1,
                like_count: 200,
                media_count: 5,
            },
            NewAuthorPayload {
                id: "auth_2".to_string(),
                name: "Beta User".to_string(),
                username: "beta_tester".to_string(),
                followers_count: 500,
                following_count: 50,
                tweet_count: 100,
                listed_count: 5,
                like_count: 1000,
                media_count: 10,
            },
            NewAuthorPayload {
                id: "auth_3".to_string(),
                name: "Charlie".to_string(),
                username: "charlie_x".to_string(),
                followers_count: 10,
                following_count: 5,
                tweet_count: 10,
                listed_count: 0,
                like_count: 20,
                media_count: 0,
            },
        ];

        state
            .db
            .tweet_authors
            .upsert_many(&authors)
            .await
            .expect("Failed to seed authors");
    }

    #[tokio::test]
    async fn test_get_tweet_authors_success_pagination() {
        let state = create_test_app_state().await;
        reset_database(&state.db.pool).await;
        seed_authors(&state).await;

        let router = Router::new()
            .route("/tweet-authors", get(handle_get_tweet_authors))
            .layer(Extension(create_mock_admin())) // Bypass auth middleware
            .with_state(state);

        // Request: Get Page 1 with size 2
        let response = router
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/tweet-authors?page=1&page_size=2")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let body_bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let body_json: Value = serde_json::from_slice(&body_bytes).unwrap();

        // Check Metadata
        let meta = &body_json["meta"];
        assert_eq!(meta["total_items"], 3);
        assert_eq!(meta["page"], 1);
        assert_eq!(meta["page_size"], 2);
        assert_eq!(meta["total_pages"], 2); // 3 items / 2 per page = 2 pages

        // Check Data
        let data = body_json["data"].as_array().unwrap();
        assert_eq!(data.len(), 2);
        assert_eq!(data[0]["username"], "beta_tester");
        assert_eq!(data[1]["username"], "alpha_user");
    }

    #[tokio::test]
    async fn test_get_tweet_authors_filtering() {
        let state = create_test_app_state().await;
        reset_database(&state.db.pool).await;
        seed_authors(&state).await;

        let router = Router::new()
            .route("/tweet-authors", get(handle_get_tweet_authors))
            .layer(Extension(create_mock_admin()))
            .with_state(state);

        // Request: Search for "Beta"
        let response = router
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/tweet-authors?search=Beta")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let body_bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let body_json: Value = serde_json::from_slice(&body_bytes).unwrap();

        let data = body_json["data"].as_array().unwrap();
        assert_eq!(data.len(), 1);
        assert_eq!(data[0]["username"], "beta_tester");
    }

    #[tokio::test]
    async fn test_get_tweet_authors_sorting() {
        let state = create_test_app_state().await;
        reset_database(&state.db.pool).await;
        seed_authors(&state).await;

        let router = Router::new()
            .route("/tweet-authors", get(handle_get_tweet_authors))
            .layer(Extension(create_mock_admin()))
            .with_state(state);

        // Request: Sort by Followers Descending
        let response = router
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/tweet-authors?sort_by=followers_count&order=desc")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let body_bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let body_json: Value = serde_json::from_slice(&body_bytes).unwrap();

        let data = body_json["data"].as_array().unwrap();
        // Expected order: Beta (500), Alpha (100), Charlie (10)
        assert_eq!(data[0]["username"], "beta_tester");
        assert_eq!(data[1]["username"], "alpha_user");
        assert_eq!(data[2]["username"], "charlie_x");
    }

    #[tokio::test]
    async fn test_get_tweet_author_by_id_success() {
        let state = create_test_app_state().await;
        reset_database(&state.db.pool).await;
        seed_authors(&state).await;

        let router = Router::new()
            .route("/tweet-authors/:id", get(handle_get_tweet_author_by_id))
            .with_state(state);

        let response = router
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/tweet-authors/auth_2")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let body_bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let body_json: Value = serde_json::from_slice(&body_bytes).unwrap();

        assert_eq!(body_json["data"]["id"], "auth_2");
        assert_eq!(body_json["data"]["username"], "beta_tester");
    }

    #[tokio::test]
    async fn test_get_tweet_author_by_id_not_found() {
        let state = create_test_app_state().await;
        reset_database(&state.db.pool).await;
        // No authors seeded

        let router = Router::new()
            .route("/tweet-authors/:id", get(handle_get_tweet_author_by_id))
            .with_state(state);

        let response = router
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/tweet-authors/non_existent_id")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), 404);
    }
}
