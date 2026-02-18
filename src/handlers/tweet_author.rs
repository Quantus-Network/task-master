use axum::{
    extract::{self, Path, Query, State},
    http::StatusCode,
    response::NoContent,
    Extension, Json,
};
use rusx::resources::{user::UserParams, UserField};

use crate::{
    db_persistence::DbError,
    handlers::{
        calculate_total_pages, HandlerError, ListQueryParams, PaginatedResponse, PaginationMetadata, SuccessResponse,
    },
    http_server::AppState,
    models::{
        admin::Admin,
        tweet_author::{AuthorFilter, AuthorSortColumn, CreateTweetAuthorInput, NewAuthorPayload, TweetAuthor},
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

/// POST /tweet-authors
pub async fn handle_create_tweet_author(
    State(state): State<AppState>,
    Extension(_): Extension<Admin>,
    Json(payload): Json<CreateTweetAuthorInput>,
) -> Result<(StatusCode, Json<SuccessResponse<String>>), AppError> {
    let mut params = UserParams::new();
    params.user_fields = Some(vec![
        UserField::PublicMetrics,
        UserField::Id,
        UserField::Name,
        UserField::Username,
    ]);

    let author_response = state
        .twitter_gateway
        .users()
        .get_by_username(&payload.username, Some(params.clone()))
        .await?;
    let Some(author) = author_response.data else {
        return Err(AppError::Handler(HandlerError::InvalidBody(format!(
            "Tweet Author {} not found",
            payload.username
        ))));
    };

    let new_author = NewAuthorPayload::new(author);
    let create_response = state.db.tweet_authors.upsert(&new_author).await?;

    Ok((StatusCode::CREATED, SuccessResponse::new(create_response)))
}

/// PUT /tweet-authors/:id/ignore
pub async fn handle_ignore_tweet_author(
    State(state): State<AppState>,
    Extension(_): Extension<Admin>,
    Path(id): Path<String>,
) -> Result<NoContent, AppError> {
    state.db.tweet_authors.set_ignore_status(&id, true).await?;

    Ok(NoContent)
}

/// PUT /tweet-authors/:id/watch
pub async fn handle_watch_tweet_author(
    State(state): State<AppState>,
    Extension(_): Extension<Admin>,
    Path(id): Path<String>,
) -> Result<NoContent, AppError> {
    state.db.tweet_authors.set_ignore_status(&id, false).await?;

    Ok(NoContent)
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
    use std::sync::Arc;

    use axum::{
        body::Body,
        extract::Request,
        http::StatusCode,
        routing::{get, post, put},
        Extension, Router,
    };
    use rusx::{
        resources::{
            user::{User, UserApi, UserPublicMetrics},
            TwitterApiResponse,
        },
        MockTwitterGateway, MockUserApi,
    };
    use serde_json::Value;
    use tower::ServiceExt;

    use crate::{
        handlers::tweet_author::{
            handle_create_tweet_author, handle_get_tweet_author_by_id, handle_get_tweet_authors,
            handle_ignore_tweet_author, handle_watch_tweet_author,
        },
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
                is_ignored: Some(true),
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
                is_ignored: Some(true),
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
                is_ignored: Some(true),
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

        assert_eq!(response.status(), 200);
        let body_bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let body: Value = serde_json::from_slice(&body_bytes).unwrap();
        assert_eq!(
            body["error"].as_str().unwrap(),
            "Tweet Author non_existent_id not found"
        );
    }

    #[tokio::test]
    async fn test_create_tweet_author_success() {
        let mut state = create_test_app_state().await;
        reset_database(&state.db.pool).await;

        // --- Setup Twitter Mock ---
        let mut mock_gateway = MockTwitterGateway::new();
        let mut mock_user = MockUserApi::new();

        mock_user.expect_get_by_username().returning(|_, _| {
            Ok(TwitterApiResponse {
                data: Some(User {
                    id: "hello".to_string(),
                    name: "hello".to_string(),
                    username: "test_user".to_string(),
                    description: Some("Quantus Network".to_string()),
                    public_metrics: Some(UserPublicMetrics {
                        followers_count: 100,
                        following_count: 50,
                        tweet_count: 10,
                        listed_count: 5,
                        like_count: Some(0),
                        media_count: Some(0),
                    }),
                }),
                includes: None,
                meta: None,
            })
        });

        let user_api_arc: Arc<dyn UserApi> = Arc::new(mock_user);

        mock_gateway.expect_users().times(1).return_const(user_api_arc);

        state.twitter_gateway = Arc::new(mock_gateway);

        let router = Router::new()
            .route("/tweet-authors", post(handle_create_tweet_author))
            .layer(Extension(create_mock_admin()))
            .with_state(state.clone());

        let payload = serde_json::json!({
            "username": "test_user"
        });

        let response = router
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/tweet-authors")
                    .header("Content-Type", "application/json")
                    .body(Body::from(serde_json::to_vec(&payload).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert!(response.status() == StatusCode::CREATED);

        let author = state.db.tweet_authors.find_by_id("hello").await.unwrap().unwrap();

        assert!(author.is_ignored);
    }

    #[tokio::test]
    async fn test_ignore_and_watch_tweet_author() {
        let state = create_test_app_state().await;
        reset_database(&state.db.pool).await;
        seed_authors(&state).await;

        let router = Router::new()
            .route("/tweet-authors/:id/ignore", put(handle_ignore_tweet_author))
            .route("/tweet-authors/:id/watch", put(handle_watch_tweet_author))
            .layer(Extension(create_mock_admin()))
            .with_state(state.clone());

        // 1. Test Ignore
        let ignore_res = router
            .clone()
            .oneshot(
                Request::builder()
                    .method("PUT")
                    .uri("/tweet-authors/auth_1/ignore")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(ignore_res.status(), StatusCode::NO_CONTENT);

        // Verify in DB
        let author = state.db.tweet_authors.find_by_id("auth_1").await.unwrap().unwrap();
        assert!(author.is_ignored);

        // 2. Test Watch (Un-ignore)
        let watch_res = router
            .oneshot(
                Request::builder()
                    .method("PUT")
                    .uri("/tweet-authors/auth_1/watch")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(watch_res.status(), StatusCode::NO_CONTENT);

        // Verify in DB
        let author_updated = state.db.tweet_authors.find_by_id("auth_1").await.unwrap().unwrap();
        assert!(!author_updated.is_ignored);
    }
}
