use axum::{
    extract::{self, State},
    response::NoContent,
    Extension, Json,
};

use crate::{
    db_persistence::DbError,
    handlers::HandlerError,
    http_server::AppState,
    models::{
        address::{
            Address, AddressInput, AddressStatsResponse, AssociateEthAddressRequest,
            AssociateEthAddressResponse, NewAddressPayload, RewardProgramStatusPayload,
            SyncTransfersResponse,
        },
        ModelError,
    },
    utils::generate_referral_code::generate_referral_code,
    AppError,
};

use super::SuccessResponse;

#[derive(Debug, thiserror::Error)]
pub enum AddressHandlerError {
    #[error("{0}")]
    Unauthorized(String),
}

pub async fn handle_update_reward_program_status(
    State(state): State<AppState>,
    Extension(user): Extension<Address>,
    extract::Path(id): extract::Path<String>,
    extract::Json(payload): Json<RewardProgramStatusPayload>,
) -> Result<NoContent, AppError> {
    // Ensure the authenticated user can only update their own reward program status
    if user.quan_address.0 != id {
        return Err(AppError::Handler(HandlerError::Address(
            AddressHandlerError::Unauthorized("You can only update your own reward program status".to_string()),
        )));
    }

    tracing::info!("Making sure address exist by trying to save address...");

    let _ = handle_add_address(
        State(state.clone()),
        Json(NewAddressPayload {
            quan_address: id.clone(),
        }),
    )
    .await?;

    tracing::info!("Updating address reward status to {}", payload.new_status);

    state
        .db
        .addresses
        .update_address_reward_status(&id, payload.new_status)
        .await?;

    Ok(NoContent)
}

pub async fn handle_add_address(
    State(state): State<AppState>,
    extract::Json(payload): Json<NewAddressPayload>,
) -> Result<Json<SuccessResponse<String>>, AppError> {
    tracing::info!("Creating address struct...");

    let referral_code = generate_referral_code(payload.quan_address.clone()).await?;
    let input = AddressInput {
        quan_address: payload.quan_address,
        eth_address: None,
        referral_code,
    };

    let address_data = Address::new(input)?;

    let created_id = state.db.addresses.create(&address_data).await?;

    Ok(SuccessResponse::new(created_id))
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
        immediate_txs: stats.total_transactions,
        reversible_txs: stats.total_reversible_transactions,
        mining_events: stats.total_mined_blocks,
        mining_rewards: stats.total_mining_rewards,
    };

    Ok(SuccessResponse::new(data))
}

pub async fn handle_get_address_reward_status_by_id(
    State(state): State<AppState>,
    extract::Path(id): extract::Path<String>,
) -> Result<Json<SuccessResponse<bool>>, AppError> {
    tracing::info!("Getting address by id {}", id);

    if let Some(address) = state.db.addresses.find_by_id(&id).await? {
        Ok(SuccessResponse::new(address.is_reward_program_participant))
    } else {
        Err(AppError::Database(DbError::AddressNotFound(
            "Address not found!".to_string(),
        )))
    }
}

pub async fn associate_eth_address(
    State(state): State<AppState>,
    Extension(user): Extension<Address>,
    Json(payload): Json<AssociateEthAddressRequest>,
) -> Result<Json<AssociateEthAddressResponse>, AppError> {

    tracing::info!(
        "Received ETH address association request for quan_address: {} -> eth_address: {}",
        user.quan_address.0,
        payload.eth_address,
    );

    // Update the user's ETH address
    match state
        .db
        .addresses
        .update_address_eth(&user.quan_address.0, &payload.eth_address)
        .await
    {
        Ok(_) => {
            tracing::info!(
                "Updated quan_address {} with eth_address {}",
                user.quan_address.0,
                payload.eth_address
            );
        }
        Err(db_err) => return Err(AppError::Database(db_err)),
    }

    let response = AssociateEthAddressResponse {
        success: true,
        message: "Ethereum address associated successfully".to_string(),
    };

    Ok(Json(response))
}

pub async fn sync_transfers(
    State(state): State<AppState>,
) -> Result<Json<SyncTransfersResponse>, AppError> {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        config::Config, db_persistence::DbPersistence, models::ModelError,
        utils::test_db::reset_database, GraphqlClient,
    };
    use std::sync::Arc;

    // Helper to set up a test AppState with a connection to a real test DB.
    async fn setup_test_app_state() -> AppState {
        let config = Config::load_test_env().expect("Failed to load test configuration");
        let db = DbPersistence::new(config.get_database_url()).await.unwrap();
        let graphql_client = GraphqlClient::new(db.clone(), config.candidates.graphql_url.clone());

        reset_database(&db.pool).await;

        AppState {
            db: Arc::new(db),
            graphql_client: Arc::new(graphql_client),
            config: Arc::new(config),
            challenges: Arc::new(tokio::sync::RwLock::new(std::collections::HashMap::new())),
        }
    }

    #[tokio::test]
    async fn test_add_address_success() {
        // Arrange
        let state = setup_test_app_state().await;
        let payload = NewAddressPayload {
            quan_address: "qz_a_valid_and_long_address_string".to_string(),
        };

        // Act: Call the handler function directly.
        let result = handle_add_address(State(state.clone()), Json(payload.clone())).await;

        // Assert: Check the handler's response.
        assert!(result.is_ok());
        let response = result.unwrap();
        assert_eq!(
            response.data, payload.quan_address,
            "Expected the created address ID to be returned"
        );

        // Assert: Verify the address was correctly saved to the database.
        let created_address = state
            .db
            .addresses
            .find_by_id(&payload.quan_address)
            .await
            .unwrap();

        assert!(
            created_address.is_some(),
            "Address was not found in the database"
        );
        let address_data = created_address.unwrap();
        assert!(
            !address_data.referral_code.is_empty(),
            "A referral code should have been generated"
        );
    }

    #[tokio::test]
    async fn test_add_address_invalid_input() {
        // Arrange
        let state = setup_test_app_state().await;
        // This address is too short and will fail validation inside `Address::new`.
        let payload = NewAddressPayload {
            quan_address: "qzshort".to_string(),
        };

        // Act
        let result = handle_add_address(State(state.clone()), Json(payload)).await;

        // Assert
        assert!(result.is_err());
        let error = result.unwrap_err();
        // Check that it's the expected validation error.
        assert!(matches!(error, AppError::Model(ModelError::InvalidInput)));

        // Verify that no records were created in the database.
        let addresses = state.db.addresses.find_all().await.unwrap();
        assert!(
            addresses.is_empty(),
            "No address should be created on validation failure"
        );
    }

    #[tokio::test]
    async fn test_add_address_handles_conflict() {
        // Arrange
        let state = setup_test_app_state().await;
        let address_string = "qz_an_existing_address_for_conflict".to_string();

        // Manually create an address first.
        let initial_address = Address::new(AddressInput {
            quan_address: address_string.clone(),
            eth_address: None,
            referral_code: "INITIAL_CODE".to_string(),
        })
        .unwrap();
        state.db.addresses.create(&initial_address).await.unwrap();

        // Create a payload with the same address.
        let payload = NewAddressPayload {
            quan_address: address_string.clone(),
        };

        // Act: Call the handler with the duplicate address.
        let result = handle_add_address(State(state.clone()), Json(payload)).await;

        // Assert: The operation should still be successful due to ON CONFLICT.
        assert!(result.is_ok());
        let response = result.unwrap();
        assert_eq!(response.data, address_string);

        // Verify there is still only one address in the database.
        let addresses = state.db.addresses.find_all().await.unwrap();
        assert_eq!(addresses.len(), 1, "No duplicate address should be created");
    }
}
