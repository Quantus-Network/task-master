use axum::{extract::State, Json};
use serde_json::Value;

use crate::{handlers::SuccessResponse, http_server::AppState, AppError};

pub async fn handle_get_wallet_feature_flags(
    State(state): State<AppState>,
) -> Result<Json<SuccessResponse<Value>>, AppError> {
    let flags = state.wallet_feature_flags_service.get_wallet_feature_flags()?;

    Ok(SuccessResponse::new(flags))
}
