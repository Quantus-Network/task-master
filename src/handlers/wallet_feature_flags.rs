use axum::{extract::State, Json};

use crate::{
    handlers::SuccessResponse, http_server::AppState, services::wallet_feature_flags_service::WalletFeatureFlags,
    AppError,
};

pub async fn handle_get_wallet_feature_flags(
    State(state): State<AppState>,
) -> Result<Json<SuccessResponse<WalletFeatureFlags>>, AppError> {
    let flags = state.wallet_feature_flags_service.get_wallet_feature_flags()?;

    Ok(SuccessResponse::new(flags))
}
