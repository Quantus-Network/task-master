use axum::{
    extract::{self, Path, Query, State},
    http::StatusCode,
    response::NoContent,
    Extension, Json,
};

use crate::{
    db_persistence::DbError,
    handlers::{
        auth::AuthHandlerError, calculate_total_pages, validate_pagination_query, HandlerError, LeaderboardQueryParams,
        ListQueryParams, PaginatedResponse, PaginationMetadata,
    },
    http_server::AppState,
    models::{
        address::Address,
        admin::Admin,
        raid_leaderboard::RaidLeaderboard,
        raid_quest::{CreateRaidQuest, RaidQuest, RaidQuestFilter, RaidQuestSortColumn},
        raid_submission::{CreateRaidSubmission, RaidSubmissionInput, RaiderSubmissions},
    },
    utils::x_url::{build_x_status_url, parse_x_status_url},
    AppError,
};

use super::SuccessResponse;

// -----------------------------------------------------------------------------
// Admin Handlers
// -----------------------------------------------------------------------------

/// Create a new Raid Quest (Admin Only)
pub async fn handle_create_raid(
    State(state): State<AppState>,
    Extension(_admin): Extension<Admin>,
    Json(payload): Json<CreateRaidQuest>,
) -> Result<Json<SuccessResponse<i32>>, AppError> {
    tracing::info!("Admin creating new raid: {}", payload.name);

    let raid_id = state.db.raid_quests.create(&payload).await?;

    Ok(SuccessResponse::new(raid_id))
}

/// Finish/End a Raid Quest (Admin Only)
pub async fn handle_finish_raid(
    State(state): State<AppState>,
    Extension(_admin): Extension<Admin>,
    Path(id): Path<i32>,
) -> Result<NoContent, AppError> {
    tracing::info!("Admin finishing raid id: {}", id);

    state.db.raid_quests.finish(id).await?;

    Ok(NoContent)
}

pub async fn handle_revert_to_active_raid(
    State(state): State<AppState>,
    Extension(_admin): Extension<Admin>,
    Path(id): Path<i32>,
) -> Result<NoContent, AppError> {
    tracing::info!("Admin reverting to active raid id: {}", id);

    state.db.raid_quests.make_active(id).await?;

    Ok(NoContent)
}

pub async fn handle_delete_raid(
    State(state): State<AppState>,
    Extension(_admin): Extension<Admin>,
    Path(id): Path<i32>,
) -> Result<NoContent, AppError> {
    tracing::info!("Admin deleting raid id: {}", id);

    state.db.raid_quests.delete_by_id(id).await?;

    Ok(NoContent)
}

// -----------------------------------------------------------------------------
// Public/User Handlers
// -----------------------------------------------------------------------------

/// Lists raid quests with pagination, and filtering.
pub async fn handle_get_raid_quests(
    State(state): State<AppState>,
    Query(params): Query<ListQueryParams<RaidQuestSortColumn>>,
    Query(filters): Query<RaidQuestFilter>,
) -> Result<Json<PaginatedResponse<RaidQuest>>, AppError> {
    validate_pagination_query(params.page, params.page_size)?;

    let total_items = state.db.raid_quests.count_filtered(&params, &filters).await? as u32;
    let total_pages = calculate_total_pages(params.page_size, total_items);

    let raid_quests = state.db.raid_quests.find_all(&params, &filters).await?;

    let response = PaginatedResponse {
        data: raid_quests,
        meta: PaginationMetadata {
            page: params.page,
            page_size: params.page_size,
            total_items,
            total_pages,
        },
    };

    Ok(Json(response))
}

/// Get Leaderboard for a specific Raid
pub async fn handle_get_raid_leaderboard(
    State(state): State<AppState>,
    Path(raid_id): Path<i32>,
    Query(params): Query<LeaderboardQueryParams>,
) -> Result<Json<PaginatedResponse<RaidLeaderboard>>, AppError> {
    validate_pagination_query(params.page, params.page_size)?;

    let total_items = state.db.raid_leaderboards.get_total_items(raid_id).await? as u32;
    let total_pages = calculate_total_pages(params.page_size, total_items);

    let offset = (params.page - 1) * params.page_size;
    let entries = state
        .db
        .raid_leaderboards
        .get_entries(raid_id, params.page_size as i64, offset as i64)
        .await?;

    let response = PaginatedResponse {
        data: entries,
        meta: PaginationMetadata {
            page: params.page,
            page_size: params.page_size,
            total_items: total_items as u32,
            total_pages,
        },
    };

    Ok(Json(response))
}

/// Get Leaderboard for a specific raider in a specific raid
pub async fn handle_get_specific_raider_raid_leaderboard(
    State(state): State<AppState>,
    Path((raider_id, raid_id)): Path<(String, i32)>,
) -> Result<Json<SuccessResponse<RaidLeaderboard>>, AppError> {
    let Some(raider_leaderboard) = state.db.raid_leaderboards.get_raider_entry(raid_id, &raider_id).await? else {
        return Err(AppError::Database(DbError::RecordNotFound(format!(
            "No raider leaderboard is found"
        ))));
    };

    Ok(SuccessResponse::new(raider_leaderboard))
}

pub async fn handle_get_active_raid_raider_submissions(
    State(state): State<AppState>,
    Extension(user): Extension<Address>,
) -> Result<Json<SuccessResponse<RaiderSubmissions>>, AppError> {
    let Some(current_active_raid) = state.db.raid_quests.find_active().await? else {
        return Err(AppError::Database(DbError::RecordNotFound(format!(
            "No active raid is found"
        ))));
    };
    let Some(user_x) = state.db.x_associations.find_by_address(&user.quan_address).await? else {
        return Err(AppError::Database(DbError::RecordNotFound(format!(
            "User doesn't have X association"
        ))));
    };

    let submissions = state
        .db
        .raid_submissions
        .find_by_raider(current_active_raid.id, &user.quan_address.0)
        .await?;
    let raider_submissions: Vec<String> = submissions
        .iter()
        .map(|submission| build_x_status_url(&user_x.username, &submission.id))
        .collect();

    Ok(SuccessResponse::new(RaiderSubmissions {
        current_raid: current_active_raid,
        submissions: raider_submissions,
    }))
}

pub async fn handle_create_raid_submission(
    State(state): State<AppState>,
    Extension(user): Extension<Address>,
    extract::Json(payload): Json<RaidSubmissionInput>,
) -> Result<(StatusCode, Json<SuccessResponse<String>>), AppError> {
    let Some(current_active_raid) = state.db.raid_quests.find_active().await? else {
        return Err(AppError::Database(DbError::RecordNotFound(format!(
            "No active raid is found"
        ))));
    };
    let Some(user_x) = state.db.x_associations.find_by_address(&user.quan_address).await? else {
        return Err(AppError::Database(DbError::RecordNotFound(format!(
            "User doesn't have X association"
        ))));
    };
    let Some((reply_username, reply_id)) = parse_x_status_url(&payload.tweet_reply_link) else {
        return Err(AppError::Handler(HandlerError::InvalidBody(format!(
            "Couldn't parse tweet reply link"
        ))));
    };
    if user_x.username != reply_username {
        return Err(AppError::Handler(HandlerError::Auth(AuthHandlerError::Unauthorized(
            format!("Only tweet reply author is eligible to submit"),
        ))));
    }

    let new_raid_submission = CreateRaidSubmission {
        id: reply_id,
        raid_id: current_active_raid.id,
        raider_id: user.quan_address.0,
    };

    let created_id = state.db.raid_submissions.create(&new_raid_submission).await?;

    Ok((StatusCode::CREATED, SuccessResponse::new(created_id)))
}

pub async fn handle_delete_raid_submission(
    State(state): State<AppState>,
    Extension(user): Extension<Address>,
    Path(submission_id): Path<String>,
) -> Result<NoContent, AppError> {
    let Some(raid_submission) = state.db.raid_submissions.find_by_id(&submission_id).await? else {
        return Err(AppError::Database(DbError::RecordNotFound(format!(
            "Couldn't find raid submission with id {}",
            submission_id
        ))));
    };

    if raid_submission.raider_id != user.quan_address.0 {
        return Err(AppError::Handler(HandlerError::Auth(AuthHandlerError::Unauthorized(
            format!("Only raid submission owner can delete"),
        ))));
    }

    state.db.raid_submissions.delete(&submission_id).await?;

    Ok(NoContent)
}

#[cfg(test)]
mod tests {
    use axum::{
        body::Body,
        extract::Request,
        http::{self, StatusCode},
        routing::{delete, get, post, put},
        Extension, Router,
    };
    use chrono::Utc;
    use serde_json::Value;
    use tower::ServiceExt;

    use crate::{
        handlers::raid_quest::{
            handle_create_raid, handle_create_raid_submission, handle_delete_raid_submission, handle_finish_raid,
            handle_get_raid_quests, handle_get_specific_raider_raid_leaderboard, handle_revert_to_active_raid,
        },
        models::{
            raid_quest::CreateRaidQuest, raid_submission::RaidSubmissionInput, relevant_tweet::NewTweetPayload,
            tweet_author::NewAuthorPayload,
        },
        utils::{
            test_app_state::create_test_app_state,
            test_db::{create_mock_admin, create_persisted_address, reset_database},
        },
    };

    // Helper to seed the strict Foreign Key chain required for submissions
    // Author -> Tweet -> (Submission uses Tweet ID)
    async fn seed_target_tweet(state: &crate::http_server::AppState, tweet_id: &str) {
        let author_id = "auth_seed_1";

        // 1. Upsert Author
        let author = NewAuthorPayload {
            id: author_id.to_string(),
            name: "Target Author".to_string(),
            username: "target_auth".to_string(),
            followers_count: 0,
            following_count: 0,
            tweet_count: 0,
            listed_count: 0,
            like_count: 0,
            media_count: 0,
            is_ignored: Some(true),
        };
        state.db.tweet_authors.upsert_many(&vec![author]).await.unwrap();

        // 2. Upsert Tweet
        let tweet = NewTweetPayload {
            id: tweet_id.to_string(),
            author_id: author_id.to_string(),
            text: "Target Tweet Text".to_string(),
            impression_count: 0,
            reply_count: 0,
            retweet_count: 0,
            like_count: 0,
            created_at: Utc::now(),
        };
        state.db.relevant_tweets.upsert_many(&vec![tweet]).await.unwrap();
    }

    // --- Admin Tests ---

    #[tokio::test]
    async fn test_admin_create_raid() {
        let state = create_test_app_state().await;
        reset_database(&state.db.pool).await;

        let router = Router::new()
            .route("/raids", post(handle_create_raid))
            .layer(Extension(create_mock_admin()))
            .with_state(state);

        let payload = CreateRaidQuest {
            name: "Unit Test Raid".to_string(),
        };

        let response = router
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/raids")
                    .header(http::header::CONTENT_TYPE, "application/json")
                    .body(Body::from(serde_json::to_string(&payload).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let body_bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let body: Value = serde_json::from_slice(&body_bytes).unwrap();

        assert!(body["data"].is_number(), "Should return the new Raid ID");
    }

    #[tokio::test]
    async fn test_admin_finish_raid() {
        let state = create_test_app_state().await;
        reset_database(&state.db.pool).await;

        // 1. Create Active Raid
        let create_payload = CreateRaidQuest {
            name: "Active Raid".to_string(),
        };
        let raid_id = state.db.raid_quests.create(&create_payload).await.unwrap();

        let router = Router::new()
            .route("/raids/:id/finish", put(handle_finish_raid))
            .layer(Extension(create_mock_admin()))
            .with_state(state.clone());

        // 2. Finish It
        let response = router
            .oneshot(
                Request::builder()
                    .method("PUT")
                    .uri(&format!("/raids/{}/finish", raid_id))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::NO_CONTENT);

        // 3. Verify in DB
        let raid = state.db.raid_quests.find_active().await.unwrap();
        assert!(raid.is_none());
    }

    #[tokio::test]
    async fn test_admin_revert_to_active() {
        let state = create_test_app_state().await;
        reset_database(&state.db.pool).await;

        // 1. Create Raid and immediately finish it
        let create_payload = CreateRaidQuest {
            name: "Finished Raid".to_string(),
        };
        let raid_id = state.db.raid_quests.create(&create_payload).await.unwrap();
        state.db.raid_quests.finish(raid_id).await.unwrap();

        let router = Router::new()
            .route("/raids/:id/activate", put(handle_revert_to_active_raid))
            .layer(Extension(create_mock_admin()))
            .with_state(state.clone());

        // 2. Revert
        let response = router
            .oneshot(
                Request::builder()
                    .method("PUT")
                    .uri(&format!("/raids/{}/activate", raid_id))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::NO_CONTENT);

        // 3. Verify end_date is NULL
        let raid = state.db.raid_quests.find_active().await.unwrap();
        assert!(raid.is_some());
        assert!(raid.unwrap().end_date.is_none())
    }

    // --- Public Get Handlers ---

    #[tokio::test]
    async fn test_get_raid_quests_pagination() {
        let state = create_test_app_state().await;
        reset_database(&state.db.pool).await;

        // Seed 2 raids
        let raid_id = state
            .db
            .raid_quests
            .create(&CreateRaidQuest {
                name: "Raid 1".to_string(),
            })
            .await
            .unwrap();
        state.db.raid_quests.finish(raid_id).await.unwrap();
        // Sleep briefly to ensure timestamp difference if sorting by date
        tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
        state
            .db
            .raid_quests
            .create(&CreateRaidQuest {
                name: "Raid 2".to_string(),
            })
            .await
            .unwrap();

        let router = Router::new()
            .route("/raids", get(handle_get_raid_quests))
            .with_state(state);

        let response = router
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/raids?page=1&page_size=10")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let body_bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let body: Value = serde_json::from_slice(&body_bytes).unwrap();

        let data = body["data"].as_array().unwrap();
        assert_eq!(data.len(), 2);
        assert_eq!(body["meta"]["total_items"], 2);
    }

    #[tokio::test]
    async fn test_get_specific_raider_leaderboard() {
        let state = create_test_app_state().await;
        reset_database(&state.db.pool).await;

        // 1. Setup: Raid, User, Target Tweet, Submission
        let raid_id = state
            .db
            .raid_quests
            .create(&CreateRaidQuest { name: "Raid".into() })
            .await
            .unwrap();
        let user = create_persisted_address(&state.db.addresses, "user_A").await;

        seed_target_tweet(&state, "target_123").await;

        // Directly insert into DB to bypass handler logic for seeding
        sqlx::query("INSERT INTO raid_submissions (id, raid_id, target_id, raider_id, impression_count) VALUES ($1, $2, $3, $4, 10)")
            .bind("sub_1")
            .bind(raid_id)
            .bind("target_123")
            .bind(&user.quan_address.0)
            .execute(&state.db.pool)
            .await
            .unwrap();

        // Must refresh view!
        state.db.raid_leaderboards.refresh().await.unwrap();

        let router = Router::new()
            .route(
                "/raiders/:raider_id/leaderboard/:raid_id",
                get(handle_get_specific_raider_raid_leaderboard),
            )
            .with_state(state);

        // 2. Request
        let response = router
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri(&format!("/raiders/{}/leaderboard/{}", user.quan_address.0, raid_id,))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        let body_bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let body: Value = serde_json::from_slice(&body_bytes).unwrap();

        assert_eq!(body["data"]["raider"]["address"], user.quan_address.0);
        assert_eq!(body["data"]["total_impressions"], 10);
    }

    #[tokio::test]
    async fn test_create_raid_submission_success() {
        let state = create_test_app_state().await;
        reset_database(&state.db.pool).await;

        // 1. Setup Active Raid
        let raid_id = state
            .db
            .raid_quests
            .create(&CreateRaidQuest { name: "Active".into() })
            .await
            .unwrap();

        // 2. Setup User
        let user = create_persisted_address(&state.db.addresses, "submitter").await;

        // 3. Seed Target Tweet (Required for Foreign Key)
        let target_tweet_id = "1868000000000000000";
        seed_target_tweet(&state, target_tweet_id).await;

        // 4. Setup X Association
        // The handler requires the user to have an X account, and that account
        // must match the username in the 'tweet_reply_link' (which is "me" below).
        sqlx::query("INSERT INTO x_associations (quan_address, username, created_at) VALUES ($1, $2, NOW())")
            .bind(&user.quan_address.0)
            .bind("me") // Must match the username in the payload URL
            .execute(&state.db.pool)
            .await
            .expect("Failed to create X association");

        let router = Router::new()
            .route("/submissions", post(handle_create_raid_submission))
            .layer(Extension(user))
            .with_state(state.clone());

        // 5. Payload
        // Reply Link -> ID 999999999, Username "me"
        let payload = RaidSubmissionInput {
            tweet_reply_link: "https://x.com/me/status/999999999".to_string(),
        };

        let response = router
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/submissions")
                    .header(http::header::CONTENT_TYPE, "application/json")
                    .body(Body::from(serde_json::to_string(&payload).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::CREATED); // Note: Handler returns CREATED (201), your assert was checking 200

        // 6. Verify in DB
        let sub = state.db.raid_submissions.find_by_id("999999999").await.unwrap();
        assert!(sub.is_some());
        let sub = sub.unwrap();
        assert_eq!(sub.raid_id, raid_id);
        assert_eq!(&sub.id, "999999999");
        assert!(sub.target_id.is_none());
    }

    #[tokio::test]
    async fn test_create_raid_submission_fails_no_active_raid() {
        let state = create_test_app_state().await;
        reset_database(&state.db.pool).await;

        // 1. Create a raid and finish it (No Active Raid)
        let raid_id = state
            .db
            .raid_quests
            .create(&CreateRaidQuest { name: "Ended".into() })
            .await
            .unwrap();
        state.db.raid_quests.finish(raid_id).await.unwrap();

        let user = create_persisted_address(&state.db.addresses, "submitter").await;

        let router = Router::new()
            .route("/submissions", post(handle_create_raid_submission))
            .layer(Extension(user))
            .with_state(state);

        let payload = RaidSubmissionInput {
            tweet_reply_link: "https://x.com/b/status/200".into(),
        };

        let response = router
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/submissions")
                    .header(http::header::CONTENT_TYPE, "application/json")
                    .body(Body::from(serde_json::to_string(&payload).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();

        // 404/RecordNotFound for No Active Raid
        assert!(response.status().is_server_error() || response.status() == StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn test_create_raid_submission_fails_invalid_url() {
        let state = create_test_app_state().await;
        reset_database(&state.db.pool).await;

        let _raid_id = state
            .db
            .raid_quests
            .create(&CreateRaidQuest { name: "Active".into() })
            .await
            .unwrap();
        let user = create_persisted_address(&state.db.addresses, "submitter").await;

        let router = Router::new()
            .route("/submissions", post(handle_create_raid_submission))
            .layer(Extension(user))
            .with_state(state);

        let payload = RaidSubmissionInput {
            tweet_reply_link: "https://x.com/b/dwdwdwt/dwdwd".into(),
        };

        let response = router
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/submissions")
                    .header(http::header::CONTENT_TYPE, "application/json")
                    .body(Body::from(serde_json::to_string(&payload).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();

        // 400 Bad Request / Handler Error
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn test_delete_raid_submission_success() {
        let state = create_test_app_state().await;
        reset_database(&state.db.pool).await;

        let raid_id = state
            .db
            .raid_quests
            .create(&CreateRaidQuest { name: "Active".into() })
            .await
            .unwrap();
        let user = create_persisted_address(&state.db.addresses, "owner").await;

        // Seed FKs
        seed_target_tweet(&state, "target_X").await;

        // Manually Insert Submission
        let submission_id = "sub_to_delete";
        sqlx::query("INSERT INTO raid_submissions (id, raid_id, target_id, raider_id) VALUES ($1, $2, 'target_X', $3)")
            .bind(submission_id)
            .bind(raid_id)
            .bind(&user.quan_address.0)
            .execute(&state.db.pool)
            .await
            .unwrap();

        let router = Router::new()
            .route("/submissions/:id", delete(handle_delete_raid_submission))
            .layer(Extension(user))
            .with_state(state.clone());

        let response = router
            .oneshot(
                Request::builder()
                    .method("DELETE")
                    .uri(&format!("/submissions/{}", submission_id))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::NO_CONTENT);

        // Verify Deletion
        let found = state.db.raid_submissions.find_by_id(submission_id).await.unwrap();
        assert!(found.is_none());
    }

    #[tokio::test]
    async fn test_delete_raid_submission_unauthorized() {
        let state = create_test_app_state().await;
        reset_database(&state.db.pool).await;

        let raid_id = state
            .db
            .raid_quests
            .create(&CreateRaidQuest { name: "Active".into() })
            .await
            .unwrap();
        let owner = create_persisted_address(&state.db.addresses, "owner").await;
        let attacker = create_persisted_address(&state.db.addresses, "attacker").await;

        seed_target_tweet(&state, "target_X").await;

        // Submission belongs to OWNER
        let submission_id = "sub_protected";
        sqlx::query("INSERT INTO raid_submissions (id, raid_id, target_id, raider_id) VALUES ($1, $2, 'target_X', $3)")
            .bind(submission_id)
            .bind(raid_id)
            .bind(&owner.quan_address.0)
            .execute(&state.db.pool)
            .await
            .unwrap();

        let router = Router::new()
            .route("/submissions/:id", delete(handle_delete_raid_submission))
            .layer(Extension(attacker)) // Attacker tries to delete
            .with_state(state);

        let response = router
            .oneshot(
                Request::builder()
                    .method("DELETE")
                    .uri(&format!("/submissions/{}", submission_id))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        // Should return 401 Unauthorized (or 403 Forbidden depending on your error mapping)
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }
}
