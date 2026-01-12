use auth::auth_routes;
use axum::Router;
use referral::referral_routes;

use crate::{
    http_server::AppState,
    routes::{
        address::address_routes, raid_quest::raid_quest_routes, relevant_tweet::relevant_tweet_routes,
        tweet_author::tweet_author_routes,
    },
};

pub mod address;
pub mod auth;
pub mod raid_quest;
pub mod referral;
pub mod relevant_tweet;
pub mod tweet_author;

pub fn api_routes(state: AppState) -> Router<AppState> {
    Router::new()
        .merge(referral_routes(state.clone()))
        .merge(address_routes(state.clone()))
        .merge(auth_routes(state.clone()))
        .merge(relevant_tweet_routes(state.clone()))
        .merge(tweet_author_routes(state.clone()))
        .merge(raid_quest_routes(state))
}
