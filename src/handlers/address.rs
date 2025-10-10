use axum::{
    extract::{self, State},
    Json,
};

use crate::{
    http_server::AppState,
    models::{
        address::{Address, AddressInput, NewAddressPayload},
    },
    utils::generate_referral_code::generate_referral_code,
    AppError,
};

use super::SuccessResponse;

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{config::Config, db_persistence::DbPersistence, models::ModelError, utils::test_db::reset_database};
    use std::sync::Arc;

    // Helper to set up a test AppState with a connection to a clean test DB.
    async fn setup_test_app_state() -> AppState {
        let config = Config::load().expect("Failed to load test configuration");
        let db = DbPersistence::new(config.get_database_url()).await.unwrap();

        reset_database(&db.pool).await;

        AppState { db: Arc::new(db) }
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

        assert!(created_address.is_some(), "Address was not found in the database");
        let address_data = created_address.unwrap();
        assert!(!address_data.referral_code.is_empty(), "A referral code should have been generated");
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
        assert!(addresses.is_empty(), "No address should be created on validation failure");
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
        }).unwrap();
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