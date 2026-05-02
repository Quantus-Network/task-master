use crate::{
    handlers::SuccessResponse, http_server::AppState, services::exchange_rate_service::ExchangeRateSnapshot, AppError,
};
use axum::{extract::State, Json};

pub async fn handle_get_exchange_rate(
    State(state): State<AppState>,
) -> Result<Json<SuccessResponse<ExchangeRateSnapshot>>, AppError> {
    let exchange_rate = state.exchange_rate_service.get_snapshot().await?;

    Ok(SuccessResponse::new(exchange_rate))
}
