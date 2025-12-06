use axum::{handler::Handler, middleware, routing::get, Router};

use crate::{
    handlers::relevant_tweet::{handle_get_relevant_tweet_by_id, handle_get_relevant_tweets},
    http_server::AppState,
    middlewares::jwt_auth,
};

pub fn relevant_tweet_routes(state: AppState) -> Router<AppState> {
    Router::new()
        .route(
            "/relevant-tweets",
            get(handle_get_relevant_tweets
                .layer(middleware::from_fn_with_state(state.clone(), jwt_auth::jwt_admin_auth))),
        )
        .route(
            "/relevant-tweets/:id",
            get(handle_get_relevant_tweet_by_id
                .layer(middleware::from_fn_with_state(state.clone(), jwt_auth::jwt_admin_auth))),
        )
}
