use axum::{
    extract::{Path, Query, State},
    response::NoContent,
    Extension, Json,
};

use crate::{
    handlers::{
        calculate_total_pages, validate_pagination_query, ListQueryParams, PaginatedResponse, PaginationMetadata,
    },
    http_server::AppState,
    models::{
        admin::Admin,
        raid_quest::{CreateRaidQuest, RaidQuest, RaidQuestFilter, RaidQuestSortColumn},
    },
    AppError,
};

use super::SuccessResponse;

pub async fn handle_create_raid(
    State(state): State<AppState>,
    Extension(_admin): Extension<Admin>,
    Json(payload): Json<CreateRaidQuest>,
) -> Result<Json<SuccessResponse<i32>>, AppError> {
    tracing::info!("Admin creating new raid: {}", payload.name);

    let raid_id = state.db.raid_quests.create(&payload).await?;

    Ok(SuccessResponse::new(raid_id))
}

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

#[cfg(test)]
mod tests {
    use axum::{
        body::Body,
        extract::Request,
        http::{self, StatusCode},
        routing::{get, post, put},
        Extension, Router,
    };
    use serde_json::Value;
    use tower::ServiceExt;

    use crate::{
        handlers::raid_quest::{
            handle_create_raid, handle_finish_raid, handle_get_raid_quests, handle_revert_to_active_raid,
        },
        models::raid_quest::CreateRaidQuest,
        utils::{
            test_app_state::create_test_app_state,
            test_db::{create_mock_admin, reset_database},
        },
    };

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

        let create_payload = CreateRaidQuest {
            name: "Active Raid".to_string(),
        };
        let raid_id = state.db.raid_quests.create(&create_payload).await.unwrap();

        let router = Router::new()
            .route("/raids/:id/finish", put(handle_finish_raid))
            .layer(Extension(create_mock_admin()))
            .with_state(state.clone());

        let response = router
            .oneshot(
                Request::builder()
                    .method("PUT")
                    .uri(format!("/raids/{}/finish", raid_id))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::NO_CONTENT);

        let raid = state.db.raid_quests.find_active().await.unwrap();
        assert!(raid.is_none());
    }

    #[tokio::test]
    async fn test_admin_revert_to_active() {
        let state = create_test_app_state().await;
        reset_database(&state.db.pool).await;

        let create_payload = CreateRaidQuest {
            name: "Finished Raid".to_string(),
        };
        let raid_id = state.db.raid_quests.create(&create_payload).await.unwrap();
        state.db.raid_quests.finish(raid_id).await.unwrap();

        let router = Router::new()
            .route("/raids/:id/activate", put(handle_revert_to_active_raid))
            .layer(Extension(create_mock_admin()))
            .with_state(state.clone());

        let response = router
            .oneshot(
                Request::builder()
                    .method("PUT")
                    .uri(format!("/raids/{}/activate", raid_id))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::NO_CONTENT);

        let raid = state.db.raid_quests.find_active().await.unwrap();
        assert!(raid.is_some());
        assert!(raid.unwrap().end_date.is_none())
    }

    #[tokio::test]
    async fn test_get_raid_quests_pagination() {
        let state = create_test_app_state().await;
        reset_database(&state.db.pool).await;

        let raid_id = state
            .db
            .raid_quests
            .create(&CreateRaidQuest {
                name: "Raid 1".to_string(),
            })
            .await
            .unwrap();
        state.db.raid_quests.finish(raid_id).await.unwrap();
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
}
