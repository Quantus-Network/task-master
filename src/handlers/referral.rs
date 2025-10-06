use axum::{
    extract::{self, State},
    http::StatusCode,
    response::{IntoResponse, NoContent, Response},
    Json,
};

use crate::{
    http_server::AppState,
    models::{
        address::{Address, AddressInput},
        referrals::{Referral, ReferralInput},
    },
    AppError,
};

use super::SuccessResponse;

pub async fn handle_add_referral(
    State(state): State<AppState>,
    extract::Json(referral_input): Json<ReferralInput>,
) -> Result<Json<SuccessResponse<i32>>, AppError> {
    tracing::info!("Creating referral struct...");
    let referral = Referral::new(referral_input)?;

    let address_input = AddressInput {
        quan_address: referral.referrer_address.0.clone(),
        eth_address: None,
    };
    
    tracing::info!("Creating referrer address struct...");
    let referrer = Address::new(address_input)?;

    tracing::info!("Creating referee address struct...");
    let referee = Address::new(AddressInput {
        quan_address: referral.referee_address.0.clone(),
        eth_address: None,
    })?;

    tracing::info!("Saving referrer address to DB...");
    state.db.addresses.create(&referrer).await?;

    tracing::info!("Saving referee address to DB...");
    state.db.addresses.create(&referee).await?;

    let created_task_id = state.db.referrals.create(&referral).await?;

    Ok(SuccessResponse::new(created_task_id))
}
