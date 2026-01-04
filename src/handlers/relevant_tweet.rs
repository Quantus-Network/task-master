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
    models::relevant_tweet::{RelevantTweet, TweetFilter, TweetSortColumn, TweetWithAuthor},
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

#[cfg(test)]
mod tests {
    use axum::{body::Body, extract::Request, http::StatusCode, routing::get, Router};
    use chrono::{Duration, Utc};
    use serde_json::Value;
    use tower::ServiceExt;

    use crate::{
        handlers::relevant_tweet::{handle_get_relevant_tweet_by_id, handle_get_relevant_tweets},
        models::{relevant_tweet::NewTweetPayload, tweet_author::NewAuthorPayload},
        utils::{test_app_state::create_test_app_state, test_db::reset_database},
    };

    // --- Helper to seed data ---
    // We must seed Authors first because Relevant Tweets have a foreign key to them.
    async fn seed_tweets(state: &crate::http_server::AppState) {
        // 1. Create Authors
        let authors = vec![
            NewAuthorPayload {
                id: "auth_A".to_string(),
                name: "Author A".to_string(),
                username: "author_a".to_string(),
                followers_count: 1000,
                following_count: 100,
                tweet_count: 50,
                listed_count: 0,
                like_count: 0,
                media_count: 0,
                is_ignored: Some(true),
            },
            NewAuthorPayload {
                id: "auth_B".to_string(),
                name: "Author B".to_string(),
                username: "author_b".to_string(),
                followers_count: 500,
                following_count: 50,
                tweet_count: 20,
                listed_count: 0,
                like_count: 0,
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

        // 2. Create Tweets
        let tweets = vec![
            // Tweet 1: High impressions, recent
            NewTweetPayload {
                id: "tweet_1".to_string(),
                author_id: "auth_A".to_string(),
                text: "Hello World! This is a popular tweet.".to_string(),
                impression_count: 1000,
                reply_count: 50,
                retweet_count: 20,
                like_count: 200,
                created_at: Utc::now(),
            },
            // Tweet 2: Low impressions, older
            NewTweetPayload {
                id: "tweet_2".to_string(),
                author_id: "auth_A".to_string(),
                text: "Just a quiet update.".to_string(),
                impression_count: 100,
                reply_count: 5,
                retweet_count: 1,
                like_count: 10,
                created_at: Utc::now() - Duration::days(1),
            },
            // Tweet 3: Medium impressions, different author
            NewTweetPayload {
                id: "tweet_3".to_string(),
                author_id: "auth_B".to_string(),
                text: "Author B saying hi.".to_string(),
                impression_count: 500,
                reply_count: 10,
                retweet_count: 5,
                like_count: 50,
                created_at: Utc::now() - Duration::hours(12),
            },
        ];
        state
            .db
            .relevant_tweets
            .upsert_many(&tweets)
            .await
            .expect("Failed to seed tweets");
    }

    #[tokio::test]
    async fn test_get_relevant_tweets_pagination() {
        let state = create_test_app_state().await;
        reset_database(&state.db.pool).await;
        seed_tweets(&state).await;

        let router = Router::new()
            .route("/relevant-tweets", get(handle_get_relevant_tweets))
            .with_state(state);

        // Request: Page 1, Size 2
        let response = router
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/relevant-tweets?page=1&page_size=2")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let body_bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let body_json: Value = serde_json::from_slice(&body_bytes).unwrap();

        let meta = &body_json["meta"];
        assert_eq!(meta["total_items"], 3);
        assert_eq!(meta["page"], 1);
        assert_eq!(meta["page_size"], 2);
        assert_eq!(meta["total_pages"], 2);

        let data = body_json["data"].as_array().unwrap();
        assert_eq!(data.len(), 2);

        assert_eq!(data[0]["tweet"]["id"], "tweet_1");
        assert_eq!(data[1]["tweet"]["id"], "tweet_3");
    }

    #[tokio::test]
    async fn test_get_relevant_tweets_filtering_by_author() {
        let state = create_test_app_state().await;
        reset_database(&state.db.pool).await;
        seed_tweets(&state).await;

        let router = Router::new()
            .route("/relevant-tweets", get(handle_get_relevant_tweets))
            .with_state(state);

        // Request: Filter by author_username "author_a" (should match 2 tweets)
        let response = router
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/relevant-tweets?author_username=author_a")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let body_bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let body_json: Value = serde_json::from_slice(&body_bytes).unwrap();

        let data = body_json["data"].as_array().unwrap();
        assert_eq!(data.len(), 2);

        // Verify both belong to author_a
        for tweet in data {
            assert_eq!(tweet["author_username"], "author_a");
        }
    }

    #[tokio::test]
    async fn test_get_relevant_tweets_filtering_by_min_likes() {
        let state = create_test_app_state().await;
        reset_database(&state.db.pool).await;
        seed_tweets(&state).await;

        let router = Router::new()
            .route("/relevant-tweets", get(handle_get_relevant_tweets))
            .with_state(state);

        // Request: min_likes = 100 (Only tweet_1 has 200, others have < 100)
        let response = router
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/relevant-tweets?min_likes=100")
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
        assert_eq!(data[0]["tweet"]["id"], "tweet_1");
    }

    #[tokio::test]
    async fn test_get_relevant_tweets_search_fts() {
        let state = create_test_app_state().await;
        reset_database(&state.db.pool).await;
        seed_tweets(&state).await;

        let router = Router::new()
            .route("/relevant-tweets", get(handle_get_relevant_tweets))
            .with_state(state);

        // Request: Search text "popular" (Matches tweet_1: "This is a popular tweet")
        let response = router
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/relevant-tweets?search=popular")
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
        assert_eq!(data[0]["tweet"]["id"], "tweet_1");
    }

    #[tokio::test]
    async fn test_get_relevant_tweets_sorting() {
        let state = create_test_app_state().await;
        reset_database(&state.db.pool).await;
        seed_tweets(&state).await;

        let router = Router::new()
            .route("/relevant-tweets", get(handle_get_relevant_tweets))
            .with_state(state);

        // Request: Sort by impression_count ASC (tweet_2=100 -> tweet_3=500 -> tweet_1=1000)
        let response = router
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/relevant-tweets?sort_by=impression_count&order=asc")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let body_bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let body_json: Value = serde_json::from_slice(&body_bytes).unwrap();

        let data = body_json["data"].as_array().unwrap();
        assert_eq!(data.len(), 3);
        assert_eq!(data[0]["tweet"]["id"], "tweet_2"); // 100 impressions
        assert_eq!(data[1]["tweet"]["id"], "tweet_3"); // 500 impressions
        assert_eq!(data[2]["tweet"]["id"], "tweet_1"); // 1000 impressions
    }

    #[tokio::test]
    async fn test_get_relevant_tweet_by_id_success() {
        let state = create_test_app_state().await;
        reset_database(&state.db.pool).await;
        seed_tweets(&state).await;

        let router = Router::new()
            .route("/relevant-tweets/:id", get(handle_get_relevant_tweet_by_id))
            .with_state(state);

        let response = router
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/relevant-tweets/tweet_1")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let body_bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let body_json: Value = serde_json::from_slice(&body_bytes).unwrap();

        assert_eq!(body_json["data"]["id"], "tweet_1");
        assert_eq!(body_json["data"]["author_id"], "auth_A");
    }

    #[tokio::test]
    async fn test_get_relevant_tweet_by_id_not_found() {
        let state = create_test_app_state().await;
        reset_database(&state.db.pool).await;

        let router = Router::new()
            .route("/relevant-tweets/:id", get(handle_get_relevant_tweet_by_id))
            .with_state(state);

        let response = router
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/relevant-tweets/non_existent_tweet")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), 404);
    }
}
