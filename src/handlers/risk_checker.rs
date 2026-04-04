use axum::{
    extract::{Path, State},
    Json,
};
use serde_json::json;

use crate::{
    handlers::SuccessResponse,
    http_server::AppState,
    services::risk_checker_service::{RiskCheckerError, RiskCheckerService},
    AppError,
};

pub async fn handle_get_risk_report(
    State(state): State<AppState>,
    Path(address_or_ens): Path<String>,
) -> Result<Json<SuccessResponse<serde_json::Value>>, AppError> {
    if !RiskCheckerService::is_valid_eth_address(&address_or_ens) && !RiskCheckerService::is_ens_name(&address_or_ens) {
        return Err(RiskCheckerError::InvalidInput.into());
    }

    let report = state.risk_checker_service.generate_report(&address_or_ens).await?;

    Ok(SuccessResponse::new(json!(report)))
}
