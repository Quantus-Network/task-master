use axum::{
    handler::Handler,
    middleware,
    routing::{delete, get, post, put},
    Router,
};

use crate::{
    handlers::raid_quest::{
        handle_create_raid, handle_create_raid_submission, handle_delete_raid, handle_delete_raid_submission,
        handle_finish_raid, handle_get_active_raid_raider_submissions, handle_get_raid_leaderboard,
        handle_get_raid_quests, handle_get_specific_raider_raid_leaderboard, handle_revert_to_active_raid,
    },
    http_server::AppState,
    middlewares::jwt_auth,
};

pub fn raid_quest_routes(state: AppState) -> Router<AppState> {
    Router::new()
        .route(
            "/raid-quests",
            get(handle_get_raid_quests).post(
                handle_create_raid.layer(middleware::from_fn_with_state(state.clone(), jwt_auth::jwt_admin_auth)),
            ),
        )
        .route(
            "/raid-quests/submissions",
            post(
                handle_create_raid_submission.layer(middleware::from_fn_with_state(state.clone(), jwt_auth::jwt_auth)),
            ),
        )
        .route(
            "/raid-quests/submissions/me",
            get(handle_get_active_raid_raider_submissions
                .layer(middleware::from_fn_with_state(state.clone(), jwt_auth::jwt_auth))),
        )
        .route(
            "/raid-quests/submissions/:id",
            delete(
                handle_delete_raid_submission.layer(middleware::from_fn_with_state(state.clone(), jwt_auth::jwt_auth)),
            ),
        )
        .route(
            "/raid-quests/raiders/:raider_id/leaderboards/:raid_id",
            get(handle_get_specific_raider_raid_leaderboard),
        )
        .route("/raid-quests/leaderboards/:raid_id", get(handle_get_raid_leaderboard))
        .route(
            "/raid-quests/:raid_id",
            delete(handle_delete_raid.layer(middleware::from_fn_with_state(state.clone(), jwt_auth::jwt_admin_auth))),
        )
        .route(
            "/raid-quests/:raid_id/finish",
            put(handle_finish_raid.layer(middleware::from_fn_with_state(state.clone(), jwt_auth::jwt_admin_auth))),
        )
        .route(
            "/raid-quests/:raid_id/active",
            put(handle_revert_to_active_raid
                .layer(middleware::from_fn_with_state(state.clone(), jwt_auth::jwt_admin_auth))),
        )
}
