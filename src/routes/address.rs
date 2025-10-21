use axum::{
    handler::Handler,
    middleware,
    routing::{get, post, put},
    Router,
};

use crate::{
    handlers::address::{
        associate_eth_address, handle_aggregate_address_stats,
        handle_get_address_reward_status_by_id, handle_get_address_stats,
        handle_update_reward_program_status, sync_transfers,
    },
    http_server::AppState,
    middlewares::jwt_auth,
};

pub fn address_routes(state: AppState) -> Router<AppState> {
    Router::new()
        .route(
            "/addresses/stats",
            get(
                handle_aggregate_address_stats.layer(middleware::from_fn_with_state(
                    state.clone(),
                    jwt_auth::jwt_auth,
                )),
            ),
        )
        .route("/addresses/:id/stats", get(handle_get_address_stats))
        .route(
            "/addresses/:id/reward-program",
            get(handle_get_address_reward_status_by_id).put(
                handle_update_reward_program_status.layer(middleware::from_fn_with_state(
                    state.clone(),
                    jwt_auth::jwt_auth,
                )),
            ),
        )
        .route(
            "/addresses/associate-eth",
            put(associate_eth_address
                .layer(middleware::from_fn_with_state(state, jwt_auth::jwt_auth))),
        )
        .route("/addresses/sync-transfers", post(sync_transfers))
}
