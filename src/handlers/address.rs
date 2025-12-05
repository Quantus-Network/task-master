use axum::{
    extract::{self, Query, State},
    response::NoContent,
    Extension, Json,
};

use crate::{
    db_persistence::DbError,
    handlers::{
        calculate_total_pages, HandlerError, LeaderboardQueryParams, ListQueryParams, PaginatedResponse,
        PaginationMetadata,
    },
    http_server::AppState,
    models::{
        address::{
            Address, AddressFilter, AddressSortColumn, AddressStatsResponse, AddressWithOptInAndAssociations,
            AddressWithRank, AggregateStatsQueryParams, AssociatedAccountsResponse, OptedInPositionResponse,
            RewardProgramStatusPayload, SyncTransfersResponse,
        },
        admin::Admin,
        eth_association::{
            AssociateEthAddressRequest, AssociateEthAddressResponse, EthAssociation, EthAssociationInput,
        },
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

pub fn validate_leaderboard_pagination_query(params: &LeaderboardQueryParams) -> Result<(), AppError> {
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

    Ok(())
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
    Query(params): Query<LeaderboardQueryParams>,
) -> Result<Json<PaginatedResponse<AddressWithRank>>, AppError> {
    tracing::info!("Getting leadeboard data...");

    validate_leaderboard_pagination_query(&params)?;
    let total_items = state
        .db
        .addresses
        .get_leaderboard_total_items(params.referral_code.clone())
        .await? as u32;
    let total_pages = calculate_total_pages(params.page_size, total_items);

    let addresses = state.db.addresses.get_leaderboard_entries(&params).await?;

    let response = PaginatedResponse::<AddressWithRank> {
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

pub async fn handle_get_addresses(
    State(state): State<AppState>,
    Extension(_): Extension<Admin>,
    Query(params): Query<ListQueryParams<AddressSortColumn>>,
    Query(filters): Query<AddressFilter>,
) -> Result<Json<PaginatedResponse<AddressWithOptInAndAssociations>>, AppError> {
    let total_items = state.db.addresses.count_filtered(&params, &filters).await? as u32;
    let total_pages = calculate_total_pages(params.page_size, total_items);

    let addresses = state
        .db
        .addresses
        .find_all_with_optin_and_associations(&params, &filters)
        .await?;

    let response = PaginatedResponse::<AddressWithOptInAndAssociations> {
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

    state.db.eth_associations.create(&new_association).await?;
    tracing::info!(
        "Created association for quan_address {} with eth_address {}",
        new_association.quan_address.0,
        new_association.eth_address.0
    );

    let response = AssociateEthAddressResponse {
        success: true,
        message: "ETH account associated successfully".to_string(),
    };

    Ok(Json(response))
}

pub async fn update_eth_address(
    State(state): State<AppState>,
    Extension(user): Extension<Address>,
    Json(payload): Json<AssociateEthAddressRequest>,
) -> Result<Json<AssociateEthAddressResponse>, AppError> {
    tracing::info!(
        "Received update request for quan_address: {} -> eth_addres: {}",
        user.quan_address.0,
        payload.eth_address,
    );

    let new_association = EthAssociation::new(EthAssociationInput {
        quan_address: user.quan_address.0,
        eth_address: payload.eth_address,
    })?;

    state.db.eth_associations.update_eth_address(&new_association).await?;

    tracing::info!(
        "Association updated for quan_address {} with eth_address {}",
        new_association.quan_address.0,
        new_association.eth_address.0
    );

    let response = AssociateEthAddressResponse {
        success: true,
        message: "ETH account association updated successfully".to_string(),
    };

    Ok(Json(response))
}

pub async fn dissociate_eth_address(
    State(state): State<AppState>,
    Extension(user): Extension<Address>,
) -> Result<NoContent, AppError> {
    tracing::info!("Dissociating ETH address from quan address {}", user.quan_address.0,);

    state.db.eth_associations.delete(&user.quan_address).await?;

    Ok(NoContent)
}

pub async fn dissociate_x_account(
    State(state): State<AppState>,
    Extension(user): Extension<Address>,
) -> Result<NoContent, AppError> {
    tracing::info!("Dissociating X account from quan address {}", user.quan_address.0,);

    state.db.x_associations.delete(&user.quan_address).await?;

    Ok(NoContent)
}

pub async fn retrieve_associated_accounts(
    State(state): State<AppState>,
    Extension(user): Extension<Address>,
) -> Result<Json<SuccessResponse<AssociatedAccountsResponse>>, AppError> {
    tracing::info!("Retrieving associated accounts...");

    let (x_username, eth_address) = tokio::try_join!(
        state.db.x_associations.find_by_address(&user.quan_address),
        state.db.eth_associations.find_by_quan_address(&user.quan_address)
    )?;

    Ok(SuccessResponse::new(AssociatedAccountsResponse {
        eth_address: eth_address.map(|v| v.eth_address.0),
        x_username: x_username.map(|v| v.username),
    }))
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
        models::x_association::{XAssociation, XAssociationInput},
        utils::{
            test_app_state::{create_test_app_state, generate_test_token},
            test_db::{create_persisted_address, create_persisted_eth_association, reset_database},
        },
    };
    use axum::{
        body::Body,
        http::{self, Request, StatusCode},
        middleware,
        routing::{delete, get, post, put},
        Router,
    };
    use serde_json::json;
    use tower::ServiceExt;
    use uuid::Uuid; // Required for .oneshot()

    #[tokio::test]
    async fn test_update_eth_address_success() {
        let state = create_test_app_state().await;
        reset_database(&state.db.pool).await;

        // 1. Setup: Create user, token, and INITIAL association
        let user = create_persisted_address(&state.db.addresses, "103").await;
        let token = generate_test_token(&state.config.jwt.secret, &user.quan_address.0);

        // Create initial ETH link
        let initial_eth = "0xC02aaA39b223FE8D0A0e5C4F27eAD9083C756Cc2";
        let initial_assoc = EthAssociation::new(EthAssociationInput {
            quan_address: user.quan_address.0.clone(),
            eth_address: initial_eth.to_string(),
        })
        .unwrap();
        state.db.eth_associations.create(&initial_assoc).await.unwrap();

        // 2. Setup Router for UPDATE
        let router = Router::new()
            .route("/associate-eth", put(update_eth_address)) // Using PUT handler
            .layer(middleware::from_fn_with_state(state.clone(), jwt_auth))
            .with_state(state.clone());

        // 3. Request: Send NEW eth address
        let new_eth = "0x00000000219ab540356cBB839Cbe05303d7705Fa";
        let payload = json!({ "eth_address": new_eth });

        let response = router
            .oneshot(
                Request::builder()
                    .method("PUT")
                    .uri("/associate-eth")
                    .header(http::header::CONTENT_TYPE, "application/json")
                    .header(http::header::AUTHORIZATION, format!("Bearer {}", token))
                    .body(Body::from(serde_json::to_string(&payload).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();

        // 4. Assertions
        assert_eq!(response.status(), StatusCode::OK);

        // 5. Verify DB has NEW address, not OLD
        let saved_assoc = state
            .db
            .eth_associations
            .find_by_quan_address(&user.quan_address)
            .await
            .unwrap()
            .unwrap();

        assert_eq!(saved_assoc.eth_address.0, new_eth);
        assert_ne!(saved_assoc.eth_address.0, initial_eth);
    }

    #[tokio::test]
    async fn test_dissociate_eth_address_success() {
        let state = create_test_app_state().await;
        reset_database(&state.db.pool).await;

        // 1. Setup: User, Token, and Association
        let user = create_persisted_address(&state.db.addresses, "104").await;
        let token = generate_test_token(&state.config.jwt.secret, &user.quan_address.0);

        let assoc = EthAssociation::new(EthAssociationInput {
            quan_address: user.quan_address.0.clone(),
            eth_address: "0x00000000219ab540356cBB839Cbe05303d7705Fa".to_string(),
        })
        .unwrap();
        state.db.eth_associations.create(&assoc).await.unwrap();

        // 2. Router
        let router = Router::new()
            .route("/associate-eth", delete(dissociate_eth_address))
            .layer(middleware::from_fn_with_state(state.clone(), jwt_auth))
            .with_state(state.clone());

        // 3. Request
        let response = router
            .oneshot(
                Request::builder()
                    .method("DELETE")
                    .uri("/associate-eth")
                    .header(http::header::AUTHORIZATION, format!("Bearer {}", token))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        // 4. Assert
        assert_eq!(response.status(), StatusCode::NO_CONTENT);

        // 5. Verify DB is empty
        let saved_assoc = state
            .db
            .eth_associations
            .find_by_quan_address(&user.quan_address)
            .await
            .unwrap();

        assert!(saved_assoc.is_none());
    }

    #[tokio::test]
    async fn test_dissociate_eth_address_idempotency() {
        // Test that deleting when nothing exists returns 204 (not 404 or 500)
        let state = create_test_app_state().await;
        reset_database(&state.db.pool).await;

        let user = create_persisted_address(&state.db.addresses, "105").await;
        let token = generate_test_token(&state.config.jwt.secret, &user.quan_address.0);

        // NOTE: No association created in DB

        let router = Router::new()
            .route("/associate-eth", delete(dissociate_eth_address))
            .layer(middleware::from_fn_with_state(state.clone(), jwt_auth))
            .with_state(state);

        let response = router
            .oneshot(
                Request::builder()
                    .method("DELETE")
                    .uri("/associate-eth")
                    .header(http::header::AUTHORIZATION, format!("Bearer {}", token))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::NO_CONTENT);
    }

    #[tokio::test]
    async fn test_retrieve_associated_accounts_full() {
        let state = create_test_app_state().await;
        reset_database(&state.db.pool).await;

        let user = create_persisted_address(&state.db.addresses, "106").await;
        let token = generate_test_token(&state.config.jwt.secret, &user.quan_address.0);

        // Setup X Association
        let x_assoc = XAssociation::new(XAssociationInput {
            quan_address: user.quan_address.0.clone(),
            username: "test_twitter_user".to_string(),
        })
        .unwrap();
        state.db.x_associations.create(&x_assoc).await.unwrap();

        // Setup ETH Association
        let eth_addr_str = "0x00000000219ab540356cBB839Cbe05303d7705Fa";
        let eth_assoc = EthAssociation::new(EthAssociationInput {
            quan_address: user.quan_address.0.clone(),
            eth_address: eth_addr_str.to_string(),
        })
        .unwrap();
        state.db.eth_associations.create(&eth_assoc).await.unwrap();

        let router = Router::new()
            .route("/associated-accounts", get(retrieve_associated_accounts))
            .layer(middleware::from_fn_with_state(state.clone(), jwt_auth))
            .with_state(state);

        let response = router
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/associated-accounts")
                    .header(http::header::AUTHORIZATION, format!("Bearer {}", token))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let body_bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let body_json: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();

        // Verify structure matches AssociatedAccountsResponse
        let data = &body_json["data"];
        assert_eq!(data["x_username"], "test_twitter_user");
        assert_eq!(data["eth_address"], eth_addr_str);
    }

    #[tokio::test]
    async fn test_retrieve_associated_accounts_empty() {
        let state = create_test_app_state().await;
        reset_database(&state.db.pool).await;

        let user = create_persisted_address(&state.db.addresses, "107").await;
        let token = generate_test_token(&state.config.jwt.secret, &user.quan_address.0);

        // NOTE: No associations created

        let router = Router::new()
            .route("/associated-accounts", get(retrieve_associated_accounts))
            .layer(middleware::from_fn_with_state(state.clone(), jwt_auth))
            .with_state(state);

        let response = router
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/associated-accounts")
                    .header(http::header::AUTHORIZATION, format!("Bearer {}", token))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let body_bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let body_json: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();

        let data = &body_json["data"];
        assert!(data["x_username"].is_null());
        assert!(data["eth_address"].is_null());
    }

    #[tokio::test]
    async fn test_dissociate_x_account_success() {
        let state = create_test_app_state().await;
        reset_database(&state.db.pool).await;

        // Setup Data: Create user & Token
        let user = create_persisted_address(&state.db.addresses, "101").await;
        let token = generate_test_token(&state.config.jwt.secret, &user.quan_address.0);
        let new_association = XAssociation::new(XAssociationInput {
            quan_address: user.quan_address.0,
            username: "test_twitter".to_string(),
        })
        .unwrap();
        state.db.x_associations.create(&new_association).await.unwrap();

        // Setup Router: MUST include the jwt_auth middleware
        let router = Router::new()
            .route("/associate-x", delete(dissociate_x_account))
            .layer(middleware::from_fn_with_state(state.clone(), jwt_auth))
            .with_state(state.clone());

        let response = router
            .oneshot(
                Request::builder()
                    .method("DELETE")
                    .uri("/associate-x")
                    .header(http::header::CONTENT_TYPE, "application/json")
                    .header(http::header::AUTHORIZATION, format!("Bearer {}", token))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        //  Assertions
        assert_eq!(response.status(), StatusCode::NO_CONTENT);

        //  Verification in DB
        let saved_assoc = state
            .db
            .x_associations
            .find_by_username("twitter_pro_101")
            .await
            .unwrap();

        assert!(saved_assoc.is_none());
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

    #[tokio::test]
    async fn test_handle_get_addresses_success() {
        let state = create_test_app_state().await;
        reset_database(&state.db.pool).await;

        // 1. Setup Data
        // Create 3 Addresses
        let addr1 = create_persisted_address(&state.db.addresses, "A1").await;
        let addr2 = create_persisted_address(&state.db.addresses, "A2").await;
        let addr3 = create_persisted_address(&state.db.addresses, "A3").await;

        // Setup Opt-In for Addr1 (using the repo method seen in your code)
        state
            .db
            .opt_ins
            .create(&addr1.quan_address.0)
            .await
            .expect("Failed to create opt-in");

        // Setup Eth Association for Addr2
        create_persisted_eth_association(
            &state.db.eth_associations,
            &addr2.quan_address.0,
            "0x00000000219ab540356cBB839Cbe05303d7705Fa",
        )
        .await;

        // 2. Mock Admin
        // NOTE: Adjust this instantiation to match your Admin struct definition
        let admin = Admin {
            id: Uuid::new_v4(),
            username: "new-user".to_string(),
            password: "what-ever".to_string(),
            updated_at: chrono::Utc::now(),
            created_at: chrono::Utc::now(),
        };

        // 3. Setup Router
        // We inject the Admin extension directly to bypass auth middleware logic for this unit test
        let router = Router::new()
            .route("/", get(handle_get_addresses))
            .layer(Extension(admin))
            .with_state(state);

        // 4. Request
        let response = router
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/?page=1&page_size=10") // Test default pagination
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        // 5. Assertions
        assert_eq!(response.status(), StatusCode::OK);

        let body_bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let body_json: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();

        // Check Pagination Meta
        let meta = &body_json["meta"];
        assert_eq!(meta["total_items"], 3);
        assert_eq!(meta["page"], 1);

        // Check Data content
        let data = body_json["data"].as_array().unwrap();
        assert_eq!(data.len(), 3);

        // Find Address 1 (Should be opted in)
        let res_addr1 = data
            .iter()
            .find(|x| x["address"]["quan_address"] == addr1.quan_address.0)
            .unwrap();
        assert_eq!(res_addr1["is_opted_in"], true);
        assert!(!res_addr1["opt_in_number"].is_null());

        // Find Address 2 (Should have Eth Address)
        let res_addr2 = data
            .iter()
            .find(|x| x["address"]["quan_address"] == addr2.quan_address.0)
            .unwrap();
        assert_eq!(res_addr2["eth_address"], "0x00000000219ab540356cBB839Cbe05303d7705Fa");

        // Find Address 3 (Clean)
        let res_addr3 = data
            .iter()
            .find(|x| x["address"]["quan_address"] == addr3.quan_address.0)
            .unwrap();
        assert_eq!(res_addr3["is_opted_in"], false);
        assert!(res_addr3["eth_address"].is_null());
    }
}
