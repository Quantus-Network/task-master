use axum::{
    extract::{self, State},
    http::StatusCode,
    response::{IntoResponse, NoContent, Response},
    Json,
};

use crate::{
    models::{
        address::{Address, AddressInput},
        referrals::{Referral, ReferralInput},
    },
    AppError, http_server::AppState,
};

use super::SuccessResponse;

pub async fn handle_add_referral(
    State(state): State<AppState>,
    extract::Json(referral_input): Json<ReferralInput>,
) -> Result<Json<SuccessResponse<i32>>, AppError> {
    let referral = Referral::new(referral_input)?;

    let referrer = Address::new(AddressInput {
        quan_address: referral.referrer_address.0.clone(),
        eth_address: None,
    })?;

    let referee = Address::new(AddressInput {
        quan_address: referral.referee_address.0.clone(),
        eth_address: None,
    })?;

    state.db.addresses.create(&referrer).await?;
    state.db.addresses.create(&referee).await?;

    let created_task_id = state.db.referrals.create(&referral).await?;

    Ok(SuccessResponse::new(created_task_id))
}
