use axum::{routing::post, Router};

use crate::{handlers::referral::handle_add_referral, http_server::AppState};

pub fn referral_routes() -> Router<AppState> {
    Router::new().route("/referrals", post(handle_add_referral))
}
