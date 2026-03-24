use axum::{
    routing::get,
    Router,
};

use crate::{handlers::wallet_feature_flags::handle_get_wallet_feature_flags, http_server::AppState};

pub fn wallet_feature_flags_routes(_state: AppState) -> Router<AppState> {
    Router::new().route("/feature-flags/wallet", get(handle_get_wallet_feature_flags))
}
