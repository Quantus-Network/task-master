use axum::{extract::State, Json};
use serde_json::json;

use crate::{handlers::SuccessResponse, http_server::AppState, AppError};

pub async fn handle_get_exchange_rate(
    State(state): State<AppState>,
) -> Result<Json<SuccessResponse<serde_json::Value>>, AppError> {
    let exchange_rate = state.exchange_rate_service.get_snapshot().await?;

    Ok(SuccessResponse::new(json!(exchange_rate.conversion_rates)))
}
