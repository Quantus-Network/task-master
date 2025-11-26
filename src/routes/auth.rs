use crate::{
    handlers::auth::{auth_me, request_challenge, verify_login},
    http_server::AppState,
    middlewares::jwt_auth,
};
use axum::{
    handler::Handler,
    middleware,
    routing::{get, post},
    Router,
};

pub fn auth_routes(state: AppState) -> Router<AppState> {
    Router::new()
        .route("/auth/request-challenge", post(request_challenge))
        .route("/auth/verify", post(verify_login))
        .route(
            "/auth/me",
            get(auth_me.layer(middleware::from_fn_with_state(state, jwt_auth::jwt_auth))),
        )
}
