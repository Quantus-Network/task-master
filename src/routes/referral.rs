use axum::{
    routing::{get, post},
    Router,
};

use crate::{
    handlers::referral::{handle_add_referral, handle_get_referral_by_referee},
    http_server::AppState,
};

pub fn referral_routes() -> Router<AppState> {
    Router::new()
        .route("/referrals", post(handle_add_referral))
        .route(
            "/referrals/:referee_address",
            get(handle_get_referral_by_referee),
        )
}
