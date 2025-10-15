use auth::auth_routes;
use axum::Router;
use referral::referral_routes;

use crate::{
    http_server::AppState,
    routes::{address::address_routes, task::task_routes},
};

pub mod address;
pub mod auth;
pub mod referral;
pub mod task;

pub fn api_routes(state: AppState) -> Router<AppState> {
    Router::new()
        .merge(referral_routes(state.clone()))
        .merge(address_routes(state.clone()))
        .merge(auth_routes(state.clone()))
        .merge(task_routes(state))
}
