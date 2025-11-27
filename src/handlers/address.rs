use axum::{
    extract::{self, Query, State},
    response::NoContent,
    Extension, Json,
};

use crate::{
    db_persistence::DbError,
    handlers::{HandlerError, PaginationMetadata, QueryParams},
    http_server::AppState,
    models::{
        address::{
            Address, AddressStatsResponse, AggregateStatsQueryParams, OptedInPositionResponse,
            PaginatedAddressesResponse, RewardProgramStatusPayload, SyncTransfersResponse,
        },
        eth_association::{
            AssociateEthAddressRequest, AssociateEthAddressResponse, EthAssociation, EthAssociationInput,
        },
        x_association::{AssociateXAccountRequest, AssociateXAccountResponse, XAssociation, XAssociationInput},
    },
    AppError,
};

use super::SuccessResponse;

#[derive(Debug, thiserror::Error)]
pub enum AddressHandlerError {
    #[error("{0}")]
    Unauthorized(String),
    #[error("{0}")]
    InvalidQueryParams(String),
}

pub async fn handle_update_reward_program_status(
    State(state): State<AppState>,
    Extension(user): Extension<Address>,
    extract::Path(id): extract::Path<String>,
    extract::Json(payload): Json<RewardProgramStatusPayload>,
) -> Result<NoContent, AppError> {
    if user.quan_address.0 != id {
        return Err(AppError::Handler(HandlerError::Address(
            AddressHandlerError::Unauthorized("You can only update your own reward program status".to_string()),
        )));
    }
    tracing::debug!("Updating address reward status to {}", payload.new_status);

    if payload.new_status {
        state.db.opt_ins.create(&id).await?;
    } else {
        state.db.opt_ins.delete(&id).await?;
    }

    Ok(NoContent)
}

pub async fn handle_get_address_stats(
    State(state): State<AppState>,
    extract::Path(id): extract::Path<String>,
) -> Result<Json<SuccessResponse<AddressStatsResponse>>, AppError> {
    tracing::info!("Getting address stats...");

    let referrals = state.db.referrals.count_by_referrer(id.clone()).await? as u64;
    let stats = state.graphql_client.get_address_stats(id.clone()).await?;

    let data = AddressStatsResponse {
        referrals,
        referral_events: 0,
        immediate_txs: stats.total_transactions,
        reversible_txs: stats.total_reversible_transactions,
        mining_events: stats.total_mined_blocks,
        mining_rewards: stats.total_mining_rewards,
    };

    Ok(SuccessResponse::new(data))
}

pub async fn handle_aggregate_address_stats(
    State(state): State<AppState>,
    Extension(user): Extension<Address>,
    Query(params): Query<AggregateStatsQueryParams>,
) -> Result<Json<SuccessResponse<AddressStatsResponse>>, AppError> {
    tracing::info!("Aggregate addresses stats...");

    if params.addresses.is_empty() {
        return Err(AppError::Handler(HandlerError::Address(
            AddressHandlerError::InvalidQueryParams(
                "Addresses query parameter should be defined and not empty!".to_string(),
            ),
        )));
    }

    let referrals = state.db.referrals.find_all_by_referrer(user.quan_address.0).await?;
    let referred_addresses: Vec<String> = referrals.iter().map(|acc| acc.referee_address.0.clone()).collect();
    let referral_events = state
        .graphql_client
        .get_addresses_events_count(referred_addresses)
        .await?;
    let stats = state
        .graphql_client
        .get_addresses_stats(params.addresses.clone())
        .await?;

    let data = AddressStatsResponse {
        referrals: referrals.len() as u64,
        referral_events,
        immediate_txs: stats.total_transactions,
        reversible_txs: stats.total_reversible_transactions,
        mining_events: stats.total_mined_blocks,
        mining_rewards: stats.total_mining_rewards,
    };

    Ok(SuccessResponse::new(data))
}

pub async fn handle_get_leaderboard(
    State(state): State<AppState>,
    Query(params): Query<QueryParams>,
) -> Result<Json<PaginatedAddressesResponse>, AppError> {
    tracing::info!("Getting leadeboard data...");

    if params.page < 1 {
        return Err(AppError::Handler(HandlerError::QueryParams(
            "Page query params must not be less than 1".to_string(),
        )));
    }

    if params.page_size < 1 {
        return Err(AppError::Handler(HandlerError::QueryParams(
            "Page size query params must not be less than 1".to_string(),
        )));
    }

    let total_items = state.db.addresses.get_total_items(params.referral_code.clone()).await? as u32;
    let total_pages = ((total_items as f64) / (params.page_size as f64)).ceil() as u32;
    let offset = (params.page - 1) * params.page_size;

    let addresses = state
        .db
        .addresses
        .get_leaderboard_entries(params.page_size, offset, params.referral_code)
        .await?;

    let response = PaginatedAddressesResponse {
        data: addresses,
        meta: PaginationMetadata {
            page: params.page,
            page_size: params.page_size,
            total_items,
            total_pages,
        },
    };

    Ok(Json(response))
}

pub async fn handle_get_address_reward_status_by_id(
    State(state): State<AppState>,
    extract::Path(id): extract::Path<String>,
) -> Result<Json<SuccessResponse<bool>>, AppError> {
    tracing::info!("Getting address by id {}", id);

    if state.db.addresses.find_by_id(&id).await?.is_none() {
        return Err(AppError::Database(DbError::AddressNotFound(
            "Address not found".to_string(),
        )));
    }

    let is_opted_in = state.db.opt_ins.find_by_address(&id).await?.is_some();
    Ok(SuccessResponse::new(is_opted_in))
}

pub async fn associate_eth_address(
    State(state): State<AppState>,
    Extension(user): Extension<Address>,
    Json(payload): Json<AssociateEthAddressRequest>,
) -> Result<Json<AssociateEthAddressResponse>, AppError> {
    tracing::info!(
        "Received ETH association request for quan_address: {} -> eth_addres: {}",
        user.quan_address.0,
        payload.eth_address,
    );

    let new_association = EthAssociation::new(EthAssociationInput {
        quan_address: user.quan_address.0,
        eth_address: payload.eth_address,
    })?;

    match state.db.eth_associations.create(&new_association).await {
        Ok(_) => {
            tracing::info!(
                "Created association for quan_address {} with eth_address {}",
                new_association.quan_address.0,
                new_association.eth_address.0
            );
        }
        Err(db_err) => return Err(AppError::Database(db_err)),
    }

    let response = AssociateEthAddressResponse {
        success: true,
        message: "ETH account associated successfully".to_string(),
    };

    Ok(Json(response))
}

pub async fn associate_x_account(
    State(state): State<AppState>,
    Extension(user): Extension<Address>,
    Json(payload): Json<AssociateXAccountRequest>,
) -> Result<Json<AssociateXAccountResponse>, AppError> {
    tracing::info!(
        "Received X association request for quan_address: {} -> X username: {}",
        user.quan_address.0,
        payload.x_username,
    );

    let new_association = XAssociation::new(XAssociationInput {
        quan_address: user.quan_address.0,
        username: payload.x_username,
    })?;

    match state.db.x_associations.create(&new_association).await {
        Ok(_) => {
            tracing::info!(
                "Created association for quan_address {} with X username {}",
                new_association.quan_address.0,
                new_association.username
            );
        }
        Err(db_err) => return Err(AppError::Database(db_err)),
    }

    let response = AssociateXAccountResponse {
        success: true,
        message: "X account associated successfully".to_string(),
    };

    Ok(Json(response))
}

pub async fn sync_transfers(State(state): State<AppState>) -> Result<Json<SyncTransfersResponse>, AppError> {
    tracing::info!("Received request to sync transfers from GraphQL endpoint");

    match state.graphql_client.sync_transfers_and_addresses().await {
        Ok((transfer_count, address_count)) => {
            tracing::info!(
                "Transfer sync completed successfully: {} transfers, {} addresses",
                transfer_count,
                address_count
            );

            let response = SyncTransfersResponse {
                success: true,
                message: format!(
                    "Successfully processed {} transfers and stored {} addresses",
                    transfer_count, address_count
                ),
                transfers_processed: Some(transfer_count),
                addresses_stored: Some(address_count),
            };

            Ok(Json(response))
        }
        Err(e) => {
            tracing::error!("Failed to sync transfers: {}", e);

            Err(AppError::Graphql(e))
        }
    }
}

pub async fn handle_get_opted_in_users(
    State(state): State<AppState>,
) -> Result<Json<SuccessResponse<Vec<crate::models::opt_in::OptIn>>>, AppError> {
    tracing::info!("Getting first 100 opted-in users");

    let opt_ins = state.db.opt_ins.get_all_ordered(100).await?;

    Ok(SuccessResponse::new(opt_ins))
}

pub async fn handle_get_opted_in_position(
    State(state): State<AppState>,
    Extension(user): Extension<Address>,
) -> Result<Json<SuccessResponse<OptedInPositionResponse>>, AppError> {
    tracing::info!("Getting opted-in position for {}", user.quan_address.0);

    if let Some(opt_in) = state.db.opt_ins.find_by_address(&user.quan_address.0).await? {
        Ok(SuccessResponse::new(OptedInPositionResponse {
            quan_address: user.quan_address.0,
            position: opt_in.opt_in_number as i64,
            is_opted_in: true,
        }))
    } else {
        Ok(SuccessResponse::new(OptedInPositionResponse {
            quan_address: user.quan_address.0,
            position: 0,
            is_opted_in: false,
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        middlewares::jwt_auth::jwt_auth,
        utils::{
            test_app_state::{create_test_app_state, generate_test_token},
            test_db::{create_persisted_address, reset_database},
        },
    };
    use axum::{
        body::Body,
        http::{self, Request, StatusCode},
        middleware,
        routing::post,
        Router,
    };
    use serde_json::json;
    use tower::ServiceExt; // Required for .oneshot()

    #[tokio::test]
    async fn test_associate_x_account_success() {
        let state = create_test_app_state().await;
        reset_database(&state.db.pool).await;

        // A. Setup Data: Create user & Token
        let user = create_persisted_address(&state.db.addresses, "101").await;
        let token = generate_test_token(&state.config.jwt.secret, &user.quan_address.0);

        // B. Setup Router: MUST include the jwt_auth middleware
        let router = Router::new()
            .route("/associate-x", post(associate_x_account))
            .layer(middleware::from_fn_with_state(state.clone(), jwt_auth))
            .with_state(state.clone());

        // C. Perform Request
        let payload = json!({ "x_username": "twitter_pro_101" });

        let response = router
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/associate-x")
                    .header(http::header::CONTENT_TYPE, "application/json")
                    .header(http::header::AUTHORIZATION, format!("Bearer {}", token))
                    .body(Body::from(serde_json::to_string(&payload).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();

        // D. Assertions
        assert_eq!(response.status(), StatusCode::OK);

        let body_bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let body_json: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();

        assert_eq!(body_json["success"], true);
        assert_eq!(body_json["message"], "X account associated successfully");

        // E. Verification in DB
        let saved_assoc = state
            .db
            .x_associations
            .find_by_username("twitter_pro_101")
            .await
            .unwrap();

        assert!(saved_assoc.is_some());
        assert_eq!(saved_assoc.unwrap().quan_address.0, user.quan_address.0);
    }

    #[tokio::test]
    async fn test_associate_x_account_fails_without_token() {
        let state = create_test_app_state().await;
        reset_database(&state.db.pool).await;

        let router = Router::new()
            .route("/associate-x", post(associate_x_account))
            .layer(middleware::from_fn_with_state(state.clone(), jwt_auth))
            .with_state(state);

        let payload = json!({ "x_username": "im_a_hacker" });

        let response = router
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/associate-x")
                    .header(http::header::CONTENT_TYPE, "application/json")
                    // NO Authorization header
                    .body(Body::from(serde_json::to_string(&payload).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn test_associate_x_account_fails_invalid_payload() {
        let state = create_test_app_state().await;
        reset_database(&state.db.pool).await;

        let user = create_persisted_address(&state.db.addresses, "102").await;
        let token = generate_test_token(&state.config.jwt.secret, &user.quan_address.0);

        let router = Router::new()
            .route("/associate-x", post(associate_x_account))
            .layer(middleware::from_fn_with_state(state.clone(), jwt_auth))
            .with_state(state);

        // Invalid Payload (missing 'x_username')
        let payload = json!({ "wrong_field": "oops" });

        let response = router
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/associate-x")
                    .header(http::header::CONTENT_TYPE, "application/json")
                    .header(http::header::AUTHORIZATION, format!("Bearer {}", token))
                    .body(Body::from(serde_json::to_string(&payload).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();

        // 422 Unprocessable Entity is Axum's default for Json<T> deserialization errors
        assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    }

    #[tokio::test]
    async fn test_associate_eth_address_success() {
        let state = create_test_app_state().await;
        reset_database(&state.db.pool).await;

        // A. Setup Data: Create user & Token
        let user = create_persisted_address(&state.db.addresses, "101").await;
        let token = generate_test_token(&state.config.jwt.secret, &user.quan_address.0);

        // B. Setup Router: MUST include the jwt_auth middleware
        let router = Router::new()
            .route("/associate-eth", post(associate_eth_address))
            .layer(middleware::from_fn_with_state(state.clone(), jwt_auth))
            .with_state(state.clone());

        // C. Perform Request
        let payload = json!({ "eth_address": "0x00000000219ab540356cBB839Cbe05303d7705Fa" });

        let response = router
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/associate-eth")
                    .header(http::header::CONTENT_TYPE, "application/json")
                    .header(http::header::AUTHORIZATION, format!("Bearer {}", token))
                    .body(Body::from(serde_json::to_string(&payload).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();

        // D. Assertions
        assert_eq!(response.status(), StatusCode::OK);

        let body_bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let body_json: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();

        assert_eq!(body_json["success"], true);
        assert_eq!(body_json["message"], "ETH account associated successfully");

        // E. Verification in DB
        let saved_assoc = state
            .db
            .eth_associations
            .find_by_quan_address(&user.quan_address)
            .await
            .unwrap();

        assert!(saved_assoc.is_some());
        assert_eq!(saved_assoc.unwrap().quan_address.0, user.quan_address.0);
    }

    #[tokio::test]
    async fn test_associate_eth_address_fails_without_token() {
        let state = create_test_app_state().await;
        reset_database(&state.db.pool).await;

        let router = Router::new()
            .route("/associate-eth", post(associate_eth_address))
            .layer(middleware::from_fn_with_state(state.clone(), jwt_auth))
            .with_state(state);

        let payload = json!({ "eth_address": "0x00000000219ab540356cBB839Cbe05303d7705Fa" });

        let response = router
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/associate-eth")
                    .header(http::header::CONTENT_TYPE, "application/json")
                    // NO Authorization header
                    .body(Body::from(serde_json::to_string(&payload).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn test_associate_eth_address_fails_invalid_payload() {
        let state = create_test_app_state().await;
        reset_database(&state.db.pool).await;

        let user = create_persisted_address(&state.db.addresses, "102").await;
        let token = generate_test_token(&state.config.jwt.secret, &user.quan_address.0);

        let router = Router::new()
            .route("/associate-eth", post(associate_eth_address))
            .layer(middleware::from_fn_with_state(state.clone(), jwt_auth))
            .with_state(state);

        let payload = json!({ "eth_address": "invalid_eth_address" });

        let response = router
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/associate-eth")
                    .header(http::header::CONTENT_TYPE, "application/json")
                    .header(http::header::AUTHORIZATION, format!("Bearer {}", token))
                    .body(Body::from(serde_json::to_string(&payload).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }
}
