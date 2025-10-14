use axum::{routing::{post, put}, Router};

use crate::{handlers::address::{associate_eth_address, handle_add_address, sync_transfers}, http_server::AppState};

pub fn address_routes() -> Router<AppState> {
    Router::new()
        .route("/addresses", post(handle_add_address))
        .route("/addresses/associate-eth", put(associate_eth_address))
        .route("/addresses/sync-transfers", post(sync_transfers))
}
