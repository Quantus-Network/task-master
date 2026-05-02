use axum::{routing::get, Router};

use crate::{handlers::exchange_rate::handle_get_exchange_rate, http_server::AppState};

pub fn exchange_rate_routes() -> Router<AppState> {
    Router::new().route("/exchange-rates", get(handle_get_exchange_rate))
}
