use axum::Router;
use referral::referral_routes;

use crate::http_server::AppState;

pub mod referral;

pub fn api_routes() -> Router<AppState> {
    Router::new().merge(referral_routes())
}
