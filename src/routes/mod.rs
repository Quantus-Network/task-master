use axum::Router;
use referral::referral_routes;
use auth::auth_routes;

use crate::{http_server::AppState, routes::{address::address_routes, task::task_routes}};

pub mod address;
pub mod referral;
pub mod auth;
pub mod task;

pub fn api_routes() -> Router<AppState> {
    Router::new()
        .merge(referral_routes())
        .merge(address_routes())
        .merge(auth_routes())
        .merge(task_routes())
}
