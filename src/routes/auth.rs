use crate::{
    handlers::auth::{
        auth_admin, auth_me, generate_x_oauth_link, handle_admin_login, handle_x_oauth, handle_x_oauth_callback,
        request_challenge, verify_login,
    },
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
            get(auth_me.layer(middleware::from_fn_with_state(state.clone(), jwt_auth::jwt_auth))),
        )
        .route("/auth/x", get(handle_x_oauth))
        .route(
            "/auth/x/link",
            get(generate_x_oauth_link.layer(middleware::from_fn_with_state(state.clone(), jwt_auth::jwt_auth))),
        )
        .route("/auth/x/callback", get(handle_x_oauth_callback))
        .route("/auth/admin/login", post(handle_admin_login))
        .route(
            "/auth/admin/me",
            get(auth_admin.layer(middleware::from_fn_with_state(state.clone(), jwt_auth::jwt_admin_auth))),
        )
}
