use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use rusx::error::SdkError;
use serde_json::json;
use tracing::error;

use crate::{
    db_persistence::DbError,
    handlers::{
        address::AddressHandlerError, auth::AuthHandlerError, referral::ReferralHandlerError, task::TaskHandlerError,
        HandlerError,
    },
    models::ModelError,
    services::{
        graphql_client::GraphqlError, reverser::ReverserError, task_generator::TaskGeneratorError,
        transaction_manager::TransactionError,
    },
};

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("Configuration error: {0}")]
    Config(#[from] ::config::ConfigError),
    #[error("Handler error")]
    Handler(#[from] HandlerError),
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
    #[error("Rusx error: {0}")]
    Rusx(#[from] SdkError),
    #[error("Telegram API error: {1}")]
    Telegram(u16, String),
}

pub type AppResult<T> = Result<T, AppError>;

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, message) = match self {
            // --- Telegram ---
            AppError::Telegram(code, err) => (
                StatusCode::from_u16(code).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR),
                err,
            ),

            // --- Model ---
            AppError::Model(err) => (StatusCode::BAD_REQUEST, err.to_string()),

            // --- Rusx ---
            AppError::Rusx(err) => map_rusx_error(err),

            // --- Handler ---
            AppError::Handler(err) => map_handler_error(err),

            // --- Database ---
            AppError::Database(err) => map_db_error(err),

            // --- Everything else ---
            e @ (AppError::Transaction(_)
            | AppError::TaskGenerator(_)
            | AppError::Reverser(_)
            | AppError::Join(_)
            | AppError::Graphql(_)
            | AppError::Config(_)
            | AppError::Http(_)
            | AppError::Server(_)) => {
                tracing::error!("Internal server error: {:?}", e.to_string());

                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "An internal server error occurred".to_string(),
                )
            }
        };

        error_response(status, message)
    }
}

fn error_response(status: StatusCode, message: impl Into<String>) -> Response {
    let message = message.into();

    let message = if message.is_empty() {
        "An error occurred".to_string()
    } else {
        message
    };

    (
        status,
        Json(json!({
            "error": message
        })),
    )
        .into_response()
}

fn map_rusx_error(err: SdkError) -> (StatusCode, String) {
    match err {
        SdkError::Api { status, data } => {
            let message = data.title;

            (
                StatusCode::from_u16(status).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR),
                message,
            )
        }

        _ => (
            StatusCode::INTERNAL_SERVER_ERROR,
            "An internal server error occurred".to_string(),
        ),
    }
}

fn map_handler_error(err: HandlerError) -> (StatusCode, String) {
    match err {
        HandlerError::InvalidBody(err) | HandlerError::QueryParams(err) => (StatusCode::BAD_REQUEST, err),

        HandlerError::Auth(err) => match err {
            AuthHandlerError::Unauthorized(err) => (StatusCode::UNAUTHORIZED, err),
            AuthHandlerError::OAuth(err) => (StatusCode::BAD_REQUEST, err),
        },

        HandlerError::Address(err) => match err {
            AddressHandlerError::InvalidQueryParams(err) => (StatusCode::BAD_REQUEST, err),
            AddressHandlerError::Unauthorized(err) => (StatusCode::UNAUTHORIZED, err),
        },

        HandlerError::Referral(err) => match err {
            ReferralHandlerError::ReferralNotFound(err) => (StatusCode::NOT_FOUND, err),
            ReferralHandlerError::InvalidReferral(err) => (StatusCode::BAD_REQUEST, err),
            ReferralHandlerError::DuplicateReferral(err) => (StatusCode::CONFLICT, err),
        },

        HandlerError::Task(err) => match err {
            TaskHandlerError::TaskNotFound(err) => (StatusCode::NOT_FOUND, err.message.clone()),
            TaskHandlerError::InvalidTaskUrl(err) => (StatusCode::BAD_REQUEST, err.message.clone()),
            TaskHandlerError::StatusConflict(err) => (StatusCode::CONFLICT, err.message.clone()),
        },
    }
}

fn map_db_error(err: DbError) -> (StatusCode, String) {
    match err {
        DbError::UniqueViolation(err) => (StatusCode::CONFLICT, err),
        DbError::RecordNotFound(err) | DbError::AddressNotFound(err) | DbError::TaskNotFound(err) => {
            (StatusCode::NOT_FOUND, err)
        }

        DbError::Database(err) => {
            error!("Database error: {}", err);
            let msg = err.to_string();

            if msg.contains("duplicate key value violates unique constraint") {
                (
                    StatusCode::CONFLICT,
                    format!("The given value is conflicting with existing record"),
                )
            } else {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "An internal server error occurred".to_string(),
                )
            }
        }

        DbError::Migration(_) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            "An internal server error occurred".to_string(),
        ),
    }
}
