use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::Json,
    routing::{get, post},
    Router,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tower::ServiceBuilder;
use tower_http::{cors::CorsLayer, trace::TraceLayer};

use crate::csv_persistence::CsvPersistence;

#[derive(Debug, thiserror::Error)]
pub enum HttpServerError {
    #[error("CSV error: {0}")]
    Csv(#[from] crate::csv_persistence::CsvError),
    #[error("Task not found: {0}")]
    TaskNotFound(String),
    #[error("Invalid task URL format: {0}")]
    InvalidTaskUrl(String),
    #[error("Server error: {0}")]
    Server(String),
}

pub type HttpServerResult<T> = Result<T, HttpServerError>;

#[derive(Debug, Clone)]
pub struct AppState {
    pub csv: Arc<CsvPersistence>,
}

#[derive(Debug, Deserialize)]
pub struct CompleteTaskRequest {
    pub task_url: String,
}

#[derive(Debug, Serialize)]
pub struct CompleteTaskResponse {
    pub success: bool,
    pub message: String,
    pub task_id: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct StatusResponse {
    pub status: String,
    pub total_tasks: usize,
    pub pending_tasks: usize,
    pub completed_tasks: usize,
    pub reversed_tasks: usize,
    pub failed_tasks: usize,
}

#[derive(Debug, Serialize)]
pub struct HealthResponse {
    pub healthy: bool,
    pub service: String,
    pub version: String,
    pub timestamp: String,
}

/// Create the HTTP server router
pub fn create_router(state: AppState) -> Router {
    Router::new()
        .route("/health", get(health_check))
        .route("/status", get(get_status))
        .route("/complete", post(complete_task))
        .route("/tasks", get(list_all_tasks))
        .route("/tasks/:task_id", get(get_task))
        .layer(
            ServiceBuilder::new()
                .layer(TraceLayer::new_for_http())
                .layer(CorsLayer::permissive())
        )
        .with_state(state)
}

/// Health check endpoint
async fn health_check() -> Json<HealthResponse> {
    Json(HealthResponse {
        healthy: true,
        service: "TaskMaster".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        timestamp: chrono::Utc::now().to_rfc3339(),
    })
}

/// Get service status and task counts
async fn get_status(State(state): State<AppState>) -> Result<Json<StatusResponse>, StatusCode> {
    let status_counts = state.csv.status_counts().await;
    let total_tasks = state.csv.task_count().await;

    let response = StatusResponse {
        status: "running".to_string(),
        total_tasks,
        pending_tasks: status_counts
            .get(&crate::csv_persistence::TaskStatus::Pending)
            .copied()
            .unwrap_or(0),
        completed_tasks: status_counts
            .get(&crate::csv_persistence::TaskStatus::Completed)
            .copied()
            .unwrap_or(0),
        reversed_tasks: status_counts
            .get(&crate::csv_persistence::TaskStatus::Reversed)
            .copied()
            .unwrap_or(0),
        failed_tasks: status_counts
            .get(&crate::csv_persistence::TaskStatus::Failed)
            .copied()
            .unwrap_or(0),
    };

    Ok(Json(response))
}

/// Complete a task by marking it as completed
async fn complete_task(
    State(state): State<AppState>,
    Json(payload): Json<CompleteTaskRequest>,
) -> Result<Json<CompleteTaskResponse>, (StatusCode, Json<CompleteTaskResponse>)> {
    tracing::info!("Received task completion request for URL: {}", payload.task_url);

    // Validate task URL format (12 digits)
    if payload.task_url.len() != 12 || !payload.task_url.chars().all(|c| c.is_ascii_digit()) {
        let response = CompleteTaskResponse {
            success: false,
            message: format!("Invalid task URL format: {}", payload.task_url),
            task_id: None,
        };
        return Err((StatusCode::BAD_REQUEST, Json(response)));
    }

    // Find task by URL
    let task = match state.csv.find_task_by_url(&payload.task_url).await {
        Some(task) => task,
        None => {
            let response = CompleteTaskResponse {
                success: false,
                message: format!("Task not found with URL: {}", payload.task_url),
                task_id: None,
            };
            return Err((StatusCode::NOT_FOUND, Json(response)));
        }
    };

    // Check if task is in a valid state for completion
    match task.status {
        crate::csv_persistence::TaskStatus::Pending => {
            // Task can be completed
        }
        crate::csv_persistence::TaskStatus::Completed => {
            let response = CompleteTaskResponse {
                success: false,
                message: "Task is already completed".to_string(),
                task_id: Some(task.task_id.clone()),
            };
            return Err((StatusCode::CONFLICT, Json(response)));
        }
        crate::csv_persistence::TaskStatus::Reversed => {
            let response = CompleteTaskResponse {
                success: false,
                message: "Task has already been reversed".to_string(),
                task_id: Some(task.task_id.clone()),
            };
            return Err((StatusCode::CONFLICT, Json(response)));
        }
        crate::csv_persistence::TaskStatus::Failed => {
            let response = CompleteTaskResponse {
                success: false,
                message: "Task has failed and cannot be completed".to_string(),
                task_id: Some(task.task_id.clone()),
            };
            return Err((StatusCode::CONFLICT, Json(response)));
        }
    }

    // Mark task as completed
    match state.csv.update_task(&task.task_id, |task| {
        task.mark_completed();
    }).await {
        Ok(()) => {
            tracing::info!("Task {} marked as completed", task.task_id);
            let response = CompleteTaskResponse {
                success: true,
                message: "Task completed successfully".to_string(),
                task_id: Some(task.task_id.clone()),
            };
            Ok(Json(response))
        }
        Err(e) => {
            tracing::error!("Failed to update task {}: {}", task.task_id, e);
            let response = CompleteTaskResponse {
                success: false,
                message: format!("Failed to update task: {}", e),
                task_id: Some(task.task_id.clone()),
            };
            Err((StatusCode::INTERNAL_SERVER_ERROR, Json(response)))
        }
    }
}

/// List all tasks (for debugging/monitoring)
async fn list_all_tasks(
    State(state): State<AppState>,
) -> Result<Json<Vec<crate::csv_persistence::TaskRecord>>, StatusCode> {
    let tasks = state.csv.get_all_tasks().await;
    Ok(Json(tasks))
}

/// Get a specific task by ID
async fn get_task(
    State(state): State<AppState>,
    Path(task_id): Path<String>,
) -> Result<Json<crate::csv_persistence::TaskRecord>, StatusCode> {
    match state.csv.get_task(&task_id).await {
        Some(task) => Ok(Json(task)),
        None => Err(StatusCode::NOT_FOUND),
    }
}

/// Start the HTTP server
pub async fn start_server(csv: Arc<CsvPersistence>, bind_address: &str) -> Result<(), Box<dyn std::error::Error>> {
    let state = AppState { csv };
    let app = create_router(state);

    tracing::info!("Starting HTTP server on {}", bind_address);

    let listener = tokio::net::TcpListener::bind(bind_address).await?;
    axum::serve(listener, app).await?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::csv_persistence::{CsvPersistence, TaskRecord, TaskStatus};
    use axum::http::StatusCode;
    use std::sync::Arc;
    use tempfile::NamedTempFile;
    use tower::ServiceExt; // for `oneshot`

    #[tokio::test]
    async fn test_health_check() {
        let temp_file = NamedTempFile::new().unwrap();
        let csv = Arc::new(CsvPersistence::new(temp_file.path()));
        let state = AppState { csv };
        let app = create_router(state);

        let response = app
            .oneshot(
                axum::http::Request::builder()
                    .uri("/health")
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_complete_task_success() {
        let temp_file = NamedTempFile::new().unwrap();
        let csv = Arc::new(CsvPersistence::new(temp_file.path()));

        // Add a test task
        let mut task = TaskRecord::new(
            "qztest123".to_string(),
            5000,
            "123456789012".to_string(),
        );
        task.set_transaction_sent(
            "0x123".to_string(),
            chrono::Utc::now(),
            chrono::Utc::now() + chrono::Duration::hours(1),
        );
        csv.add_task(task).await.unwrap();

        let state = AppState { csv };
        let app = create_router(state);

        let request_body = CompleteTaskRequest {
            task_url: "123456789012".to_string(),
        };

        let response = app
            .oneshot(
                axum::http::Request::builder()
                    .uri("/complete")
                    .method("POST")
                    .header("content-type", "application/json")
                    .body(axum::body::Body::from(serde_json::to_string(&request_body).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_complete_task_not_found() {
        let temp_file = NamedTempFile::new().unwrap();
        let csv = Arc::new(CsvPersistence::new(temp_file.path()));
        let state = AppState { csv };
        let app = create_router(state);

        let request_body = CompleteTaskRequest {
            task_url: "999999999999".to_string(),
        };

        let response = app
            .oneshot(
                axum::http::Request::builder()
                    .uri("/complete")
                    .method("POST")
                    .header("content-type", "application/json")
                    .body(axum::body::Body::from(serde_json::to_string(&request_body).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn test_complete_task_invalid_format() {
        let temp_file = NamedTempFile::new().unwrap();
        let csv = Arc::new(CsvPersistence::new(temp_file.path()));
        let state = AppState { csv };
        let app = create_router(state);

        let request_body = CompleteTaskRequest {
            task_url: "invalid".to_string(),
        };

        let response = app
            .oneshot(
                axum::http::Request::builder()
                    .uri("/complete")
                    .method("POST")
                    .header("content-type", "application/json")
                    .body(axum::body::Body::from(serde_json::to_string(&request_body).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn test_get_status() {
        let temp_file = NamedTempFile::new().unwrap();
        let csv = Arc::new(CsvPersistence::new(temp_file.path()));

        // Add some test tasks with different statuses
        let mut task1 = TaskRecord::new("qztest1".to_string(), 1000, "111111111111".to_string());
        task1.mark_completed();
        csv.add_task(task1).await.unwrap();

        let task2 = TaskRecord::new("qztest2".to_string(), 2000, "222222222222".to_string());
        csv.add_task(task2).await.unwrap();

        let state = AppState { csv };
        let app = create_router(state);

        let response = app
            .oneshot(
                axum::http::Request::builder()
                    .uri("/status")
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
    }
}
