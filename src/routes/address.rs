use axum::{handler::Handler, middleware, routing::get, Router};

use crate::{handlers::address::handle_get_addresses, http_server::AppState, middlewares::jwt_auth};

pub fn address_routes(state: AppState) -> Router<AppState> {
    Router::new().route(
        "/addresses",
        get(handle_get_addresses.layer(middleware::from_fn_with_state(state.clone(), jwt_auth::jwt_admin_auth))),
    )
}
