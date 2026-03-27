use axum::{routing::get, Router};

use crate::{handlers::config::handle_get_wallet_configs, http_server::AppState};

pub fn config_routes() -> Router<AppState> {
    Router::new().route("/configs/wallet", get(handle_get_wallet_configs))
}
