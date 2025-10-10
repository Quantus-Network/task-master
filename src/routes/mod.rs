use axum::Router;
use referral::referral_routes;

use crate::{http_server::AppState, routes::address::address_routes};

pub mod address;
pub mod referral;

pub fn api_routes() -> Router<AppState> {
    Router::new()
        .merge(referral_routes())
        .merge(address_routes())
}
