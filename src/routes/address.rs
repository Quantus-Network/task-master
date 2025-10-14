use axum::{
    routing::{get, post, put},
    Router,
};

use crate::{
    handlers::address::{
        associate_eth_address, handle_add_address, handle_get_address_reward_status_by_id,
        handle_update_reward_program_status, sync_transfers,
    },
    http_server::AppState,
};

pub fn address_routes() -> Router<AppState> {
    Router::new()
        .route("/addresses", post(handle_add_address))
        .route(
            "/addresses/:id/reward-program",
            get(handle_get_address_reward_status_by_id).put(handle_update_reward_program_status),
        )
        .route("/addresses/associate-eth", put(associate_eth_address))
        .route("/addresses/sync-transfers", post(sync_transfers))
}
