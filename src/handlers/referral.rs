use axum::{
    extract::{self, State},
    Json,
};

use crate::{
    db_persistence::DbError,
    http_server::AppState,
    models::{
        address::{Address, AddressInput},
        referrals::{Referral, ReferralData, ReferralInput},
    },
    utils::generate_referral_code::generate_referral_code,
    AppError,
};

use super::SuccessResponse;

pub async fn handle_add_referral(
    State(state): State<AppState>,
    extract::Json(referral_input): Json<ReferralInput>,
) -> Result<Json<SuccessResponse<String>>, AppError> {
    tracing::info!("Creating referral struct...");

    tracing::info!("Lookup referral code owner...");
    let referrer = state
        .db
        .addresses
        .find_by_referral_code(&referral_input.referral_code)
        .await?;
    if let Some(referrer) = referrer {
        let referral_data = ReferralData {
            referrer_address: referrer.quan_address.0.clone(),
            referee_address: referral_input.referee_address,
        };

        let referral = Referral::new(referral_data)?;

        if let Ok(referral_code) = generate_referral_code(referral.referee_address.0.clone()).await
        {
            tracing::info!("Creating referee address struct...");
            let referee = Address::new(AddressInput {
                quan_address: referral.referee_address.0.clone(),
                eth_address: None,
                referral_code,
            })?;

            tracing::info!("Saving referee address to DB...");
            state.db.addresses.create(&referee).await?;
        }

        state.db.referrals.create(&referral).await?;

        Ok(SuccessResponse::new(referrer.referral_code))
    } else {
        return Err(AppError::Database(DbError::AddressNotFound("".to_string())));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        config::Config, db_persistence::DbPersistence, repositories::address::AddressRepository,
    };
    use std::sync::Arc;

    // Helper to set up a test AppState with a connection to a real test DB.
    async fn setup_test_app_state() -> AppState {
        let config = Config::load().expect("Failed to load test configuration");
        let db = DbPersistence::new(config.get_database_url()).await.unwrap();

        // Clean tables for test isolation
        sqlx::query("TRUNCATE addresses, referrals, tasks RESTART IDENTITY CASCADE")
            .execute(&db.pool)
            .await
            .unwrap();

        AppState { db: Arc::new(db) }
    }

    // Helper to create a persisted address for tests.
    async fn create_persisted_address(repo: &AddressRepository, id: &str) -> Address {
        let input = AddressInput {
            quan_address: format!("qz_test_address_{}", id),
            eth_address: None,
            referral_code: format!("REF{}", id),
        };
        let address = Address::new(input).unwrap();
        repo.create(&address).await.unwrap();
        address
    }

    #[tokio::test]
    async fn test_add_referral_success() {
        // Arrange
        let state = setup_test_app_state().await;
        // Referrals require existing addresses, so we create them first.
        let referrer = create_persisted_address(&state.db.addresses, "referrer_01").await;
        let input = ReferralInput {
            referral_code: referrer.referral_code,
            referee_address: "qz_a_valid_referee_address".to_string(),
        };

        // Act: Call the handler function directly.
        let result = handle_add_referral(State(state.clone()), Json(input.clone())).await;

        // Assert: Check the handler's response.
        assert!(result.is_ok());
        let response = result.unwrap();
        assert!(
            response.data == input.referral_code,
            "Expected the same input referral code as response data"
        );

        // Assert: Verify the database state was changed correctly.
        let referrer = state
            .db
            .addresses
            .find_by_referral_code(&input.referral_code)
            .await
            .unwrap();

        assert!(
            referrer.is_some(),
            "Referrer address should have been created"
        );

        let referrals = state
            .db
            .referrals
            .find_all_by_referrer(referrer.unwrap().quan_address.0.clone())
            .await
            .unwrap();
        assert_eq!(referrals.len(), 1);
        assert_eq!(referrals[0].referee_address.0, input.referee_address);

        let referee = state
            .db
            .addresses
            .find_by_id(&input.referee_address)
            .await
            .unwrap();

        assert!(
            referee.is_some(),
            "Referee address should have been created"
        );
    }

    #[tokio::test]
    async fn test_add_referral_invalid_referee_input() {
        // Arrange
        let state = setup_test_app_state().await;
        // Referrals require existing addresses, so we create them first.
        let referrer = create_persisted_address(&state.db.addresses, "referrer_01").await;

        // This address is too short and will fail validation in `Referral::new`.
        let input = ReferralInput {
            referral_code: referrer.referral_code,
            referee_address: "qzshort".to_string(),
        };

        // Act
        let result = handle_add_referral(State(state.clone()), Json(input)).await;

        // Assert
        assert!(result.is_err());
        let error = result.unwrap_err();
        // Check that it's the expected validation error.
        assert!(matches!(
            error,
            AppError::Model(crate::models::ModelError::InvalidInput)
        ));

        // Verify that no records were created in the database.
        let addresses = state.db.addresses.find_all().await.unwrap();
        assert!(
            addresses.len() == 1,
            "No addresses should be created on validation failure"
        );
    }
}
