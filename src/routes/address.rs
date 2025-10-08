use axum::{routing::post, Router};

use crate::{handlers::address::handle_add_address, http_server::AppState};

pub fn address_routes() -> Router<AppState> {
    Router::new().route("/addresses", post(handle_add_address))
}
