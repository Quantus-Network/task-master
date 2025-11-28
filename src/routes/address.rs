use axum::{
    handler::Handler,
    middleware,
    routing::{delete, get, post},
    Router,
};

use crate::{
    handlers::address::{
        associate_eth_address, dissociate_eth_address, dissociate_x_account, handle_aggregate_address_stats,
        handle_get_address_reward_status_by_id, handle_get_address_stats, handle_get_leaderboard,
        handle_get_opted_in_position, handle_get_opted_in_users, handle_update_reward_program_status,
        retrieve_associated_accounts, sync_transfers, update_eth_address,
    },
    http_server::AppState,
    middlewares::jwt_auth,
};

pub fn address_routes(state: AppState) -> Router<AppState> {
    Router::new()
        .route("/addresses/leaderboard", get(handle_get_leaderboard))
        .route("/addresses/opted-in", get(handle_get_opted_in_users))
        .route(
            "/addresses/my-position",
            get(
                handle_get_opted_in_position.layer(middleware::from_fn_with_state(
                    state.clone(),
                    jwt_auth::jwt_auth,
                )),
            ),
        )
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
        .route("/addresses/associations", get(retrieve_associated_accounts).layer(middleware::from_fn_with_state(state.clone(), jwt_auth::jwt_auth)))
           .route(
            "/addresses/associations/eth",
            post(associate_eth_address
                .layer(middleware::from_fn_with_state(state.clone(), jwt_auth::jwt_auth)))
                .put(update_eth_address.layer(middleware::from_fn_with_state(state.clone(), jwt_auth::jwt_auth)))
                .delete(dissociate_eth_address.layer(middleware::from_fn_with_state(state.clone(), jwt_auth::jwt_auth))),
        )
           .route(
            "/addresses/associations/x",
            delete(dissociate_x_account
                .layer(middleware::from_fn_with_state(state, jwt_auth::jwt_auth))),
        )
        .route("/addresses/sync-transfers", post(sync_transfers))
}
