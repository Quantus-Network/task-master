use axum::{routing::{post, get}, Router};
use crate::{http_server::AppState, handlers::auth::{request_challenge, verify_login, auth_me}};

pub fn auth_routes() -> Router<AppState> {
    Router::new()
        .route("/auth/request-challenge", post(request_challenge))
        .route("/auth/verify", post(verify_login))
        .route("/auth/me", get(auth_me))
}

