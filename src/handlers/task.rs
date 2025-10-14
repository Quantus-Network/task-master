use axum::{
    extract::{Path, State},
    Json,
};

use crate::{
    db_persistence::DbError,
    handlers::HandlerError,
    http_server::AppState,
    models::task::{CompleteTaskRequest, CompleteTaskResponse, Task, TaskStatus},
    AppError,
};

use super::SuccessResponse;

#[derive(Debug, thiserror::Error)]
pub enum TaskHandlerError {
    #[error("Task not found")]
    TaskNotFound(Json<CompleteTaskResponse>),
    #[error("Invalid task URL format")]
    InvalidTaskUrl(Json<CompleteTaskResponse>),
    #[error("Invalid task status")]
    StatusConflict(Json<CompleteTaskResponse>),
}

pub async fn list_all_tasks(
    State(state): State<AppState>,
) -> Result<Json<SuccessResponse<Vec<Task>>>, AppError> {
    let tasks = state.db.tasks.get_all_tasks().await?;

    Ok(SuccessResponse::new(tasks))
}

pub async fn get_task(
    State(state): State<AppState>,
    Path(task_id): Path<String>,
) -> Result<Json<SuccessResponse<Task>>, AppError> {
    let task = state.db.tasks.get_task(&task_id).await?;

    match task {
        Some(task) => Ok(SuccessResponse::new(task)),
        None => Err(AppError::Database(DbError::TaskNotFound("".to_string()))),
    }
}

pub async fn complete_task(
    State(state): State<AppState>,
    Json(payload): Json<CompleteTaskRequest>,
) -> Result<Json<CompleteTaskResponse>, AppError> {
    tracing::info!(
        "Received task completion request for URL: {}",
        payload.task_url
    );

    // Validate task URL format (12 digits)
    if payload.task_url.len() != 12 || !payload.task_url.chars().all(|c| c.is_ascii_digit()) {
        let response = CompleteTaskResponse {
            success: false,
            message: format!("Invalid task URL format: {}", payload.task_url),
            task_id: None,
        };
        return Err(AppError::Handler(HandlerError::Task(
            TaskHandlerError::InvalidTaskUrl(Json(response)),
        )));
    }

    // Find task by URL
    let task = match state.db.tasks.find_task_by_url(&payload.task_url).await {
        Ok(Some(task)) => task,
        Ok(None) => {
            let response = CompleteTaskResponse {
                success: false,
                message: format!("Task not found with URL: {}", payload.task_url),
                task_id: None,
            };
            return Err(AppError::Handler(HandlerError::Task(
                TaskHandlerError::TaskNotFound(Json(response)),
            )));
        }
        Err(db_err) => {
            return Err(AppError::Database(db_err));
        }
    };

    // Check if task is in a valid state for completion
    match task.status {
        TaskStatus::Pending => {
            // Task can be completed
        }
        TaskStatus::Completed => {
            let response = CompleteTaskResponse {
                success: false,
                message: "Task is already completed".to_string(),
                task_id: Some(task.task_id.clone()),
            };
            return Err(AppError::Handler(HandlerError::Task(
                TaskHandlerError::StatusConflict(Json(response)),
            )));
        }
        TaskStatus::Reversed => {
            let response = CompleteTaskResponse {
                success: false,
                message: "Task has already been reversed".to_string(),
                task_id: Some(task.task_id.clone()),
            };
            return Err(AppError::Handler(HandlerError::Task(
                TaskHandlerError::StatusConflict(Json(response)),
            )));
        }
        TaskStatus::Failed => {
            let response = CompleteTaskResponse {
                success: false,
                message: "Task has failed and cannot be completed".to_string(),
                task_id: Some(task.task_id.clone()),
            };
            return Err(AppError::Handler(HandlerError::Task(
                TaskHandlerError::StatusConflict(Json(response)),
            )));
        }
    }

    // Mark task as completed
    match state
        .db
        .tasks
        .update_task_status(&task.task_id, TaskStatus::Completed)
        .await
    {
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

            return Err(AppError::Database(e));
        }
    }
}
