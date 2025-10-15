use axum::{
    handler::Handler,
    middleware,
    routing::{get, post},
    Router,
};

use crate::{
    handlers::referral::{handle_add_referral, handle_get_referral_by_referee},
    http_server::AppState,
    middlewares::jwt_auth,
};

pub fn referral_routes(state: AppState) -> Router<AppState> {
    Router::new()
        .route(
            "/referrals",
            post(
                handle_add_referral
                    .layer(middleware::from_fn_with_state(state, jwt_auth::jwt_auth)),
            ),
        )
        .route(
            "/referrals/:referee_address",
            get(handle_get_referral_by_referee),
        )
}
