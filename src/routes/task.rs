use axum::{
    routing::{get, put},
    Router,
};

use crate::{
    handlers::task::{complete_task, get_task, list_all_tasks},
    http_server::AppState,
};

pub fn task_routes() -> Router<AppState> {
    Router::new()
        .route("/tasks", get(list_all_tasks))
        .route("/tasks/complete", put(complete_task))
        .route("/tasks/:task_id", get(get_task))
}
