use axum::{
    extract::{self, State},
    Json,
};

use crate::{
    http_server::AppState,
    models::{
        address::{Address, AddressInput},
        referrals::{Referral, ReferralInput},
    },
    utils::generate_referral_code::generate_referral_code,
    AppError,
};

use super::SuccessResponse;

pub async fn handle_add_referral(
    State(state): State<AppState>,
    extract::Json(referral_input): Json<ReferralInput>,
) -> Result<Json<SuccessResponse<i32>>, AppError> {
    tracing::info!("Creating referral struct...");
    let referral = Referral::new(referral_input)?;

    if let Ok(referral_code) = generate_referral_code(referral.referrer_address.0.clone()).await {
        let address_input = AddressInput {
            quan_address: referral.referrer_address.0.clone(),
            eth_address: None,
            referral_code,
        };

        tracing::info!("Creating referrer address struct...");
        let referrer = Address::new(address_input)?;

        tracing::info!("Saving referrer address to DB...");
        state.db.addresses.create(&referrer).await?;
    }

    if let Ok(referral_code) = generate_referral_code(referral.referee_address.0.clone()).await {
        tracing::info!("Creating referee address struct...");
        let referee = Address::new(AddressInput {
            quan_address: referral.referee_address.0.clone(),
            eth_address: None,
            referral_code,
        })?;

        tracing::info!("Saving referee address to DB...");
        state.db.addresses.create(&referee).await?;
    }

    let created_task_id = state.db.referrals.create(&referral).await?;

    Ok(SuccessResponse::new(created_task_id))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{config::Config, db_persistence::DbPersistence};
    use std::sync::Arc;
    use crate::utils::test_db::reset_database;

    // Helper to set up a test AppState with a connection to a real test DB.
    async fn setup_test_app_state() -> AppState {
        let config = Config::load().expect("Failed to load test configuration");
        let db = DbPersistence::new(config.get_database_url()).await.unwrap();

        reset_database(&db.pool).await;

        AppState { db: Arc::new(db) }
    }

    #[tokio::test]
    async fn test_add_referral_success() {
        // Arrange
        let state = setup_test_app_state().await;
        let input = ReferralInput {
            referrer_address: "qz_a_valid_referrer_address".to_string(),
            referee_address: "qz_a_valid_referee_address".to_string(),
        };

        // Act: Call the handler function directly.
        let result = handle_add_referral(State(state.clone()), Json(input.clone())).await;

        // Assert: Check the handler's response.
        assert!(result.is_ok());
        let response = result.unwrap();
        assert!(response.data > 0, "Expected a positive ID for the new referral");

        // Assert: Verify the database state was changed correctly.
        let referrals = state
            .db
            .referrals
            .find_all_by_referrer(input.referrer_address.clone())
            .await
            .unwrap();
        assert_eq!(referrals.len(), 1);
        assert_eq!(referrals[0].referee_address.0, input.referee_address);

        let referrer = state
            .db
            .addresses
            .find_by_id(&input.referrer_address)
            .await
            .unwrap();
        let referee = state
            .db
            .addresses
            .find_by_id(&input.referee_address)
            .await
            .unwrap();
        assert!(referrer.is_some(), "Referrer address should have been created");
        assert!(referee.is_some(), "Referee address should have been created");
    }

    #[tokio::test]
    async fn test_add_referral_invalid_input() {
        // Arrange
        let state = setup_test_app_state().await;
        // This address is too short and will fail validation in `Referral::new`.
        let input = ReferralInput {
            referrer_address: "qzshort".to_string(),
            referee_address: "qz_a_valid_referee_address".to_string(),
        };

        // Act
        let result = handle_add_referral(State(state.clone()), Json(input)).await;

        // Assert
        assert!(result.is_err());
        let error = result.unwrap_err();
        // Check that it's the expected validation error.
        assert!(matches!(error, AppError::Model(crate::models::ModelError::InvalidInput)));

        // Verify that no records were created in the database.
        let addresses = state.db.addresses.find_all().await.unwrap();
        assert!(addresses.is_empty(), "No addresses should be created on validation failure");
    }
}