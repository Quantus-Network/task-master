// Add Axum-related imports for handling HTTP responses
use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde_json::json;

// Bring necessary error types into scope from their respective modules
use crate::{
    db_persistence::DbError,
    models::ModelError,
    services::{
        graphql_client::GraphqlError, reverser::ReverserError,
        task_generator::TaskGeneratorError, transaction_manager::TransactionError,
    },
};

// Make the enum public so it can be used throughout your crate
#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("Configuration error: {0}")]
    Config(#[from] ::config::ConfigError),
    #[error("Data model error: {0}")]
    Model(#[from] ModelError),
    #[error("Database error: {0}")]
    Database(#[from] DbError),
    #[error("Transaction manager error: {0}")]
    Transaction(#[from] TransactionError),
    #[error("Task generator error: {0}")]
    TaskGenerator(#[from] TaskGeneratorError),
    #[error("Reverser error: {0}")]
    Reverser(#[from] ReverserError),
    #[error("Server error: {0}")]
    Server(String),
    #[error("Join error: {0}")]
    Join(#[from] tokio::task::JoinError),
    #[error("GraphQL error: {0}")]
    Graphql(#[from] GraphqlError),
    #[error("HTTP server error: {0}")]
    Http(#[from] axum::http::Error),
}

// Define the result type using the public AppError
pub type AppResult<T> = Result<T, AppError>;

// NEW: Implement IntoResponse for AppError
impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, error_message) = match self {
            // User-facing errors (e.g., bad input)
            AppError::Model(err) => (StatusCode::BAD_REQUEST, err.to_string()),

            // Internal server errors that shouldn't expose details
            AppError::Database(_)
            | AppError::Transaction(_)
            | AppError::TaskGenerator(_)
            | AppError::Reverser(_)
            | AppError::Join(_)
            | AppError::Graphql(_)
            | AppError::Config(_)
            | AppError::Http(_)
            | AppError::Server(_) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "An internal server error occurred".to_string(), // Hide sensitive details
            ),
        };

        let body = Json(json!({
            "error": error_message,
        }));

        (status, body).into_response()
    }
}