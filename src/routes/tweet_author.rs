use axum::{handler::Handler, middleware, routing::get, Router};

use crate::{
    handlers::tweet_author::{handle_get_tweet_author_by_id, handle_get_tweet_authors},
    http_server::AppState,
    middlewares::jwt_auth,
};

pub fn tweet_author_routes(state: AppState) -> Router<AppState> {
    Router::new()
        .route(
            "/tweet-authors",
            get(handle_get_tweet_authors
                .layer(middleware::from_fn_with_state(state.clone(), jwt_auth::jwt_admin_auth))),
        )
        .route(
            "/tweet-authors/:id",
            get(handle_get_tweet_author_by_id
                .layer(middleware::from_fn_with_state(state.clone(), jwt_auth::jwt_admin_auth))),
        )
}
