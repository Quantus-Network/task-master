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
        ErrorResponse, HandlerError,
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
        let (status, error_message) = match self {
            AppError::Telegram(status_code_in_u16, err) => {
                let status_code =
                    StatusCode::from_u16(status_code_in_u16).unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR);

                (status_code, err)
            }

            AppError::Model(err) => (StatusCode::BAD_REQUEST, err.to_string()),

            AppError::Rusx(err) => match err {
                SdkError::Api { status, data } => {
                    tracing::error!("Rusx API error: {:?}", data);

                    (
                        StatusCode::from_u16(status).unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR),
                        data.title,
                    )
                }

                SdkError::AuthConfiguration(_) | SdkError::Http(_) | SdkError::Json(_) | SdkError::Unknown(_) => (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "An internal server error occurred".to_string(),
                ),
            },

            AppError::Handler(err) => match err {
                HandlerError::QueryParams(err) => (StatusCode::BAD_REQUEST, err.to_string()),
                HandlerError::Auth(err) => match err {
                    AuthHandlerError::Unauthrorized(err) => (StatusCode::UNAUTHORIZED, err),
                    AuthHandlerError::OAuth(err) => (StatusCode::BAD_REQUEST, err),
                },
                HandlerError::Address(err) => match err {
                    AddressHandlerError::InvalidQueryParams(err) => {
                        return (
                            StatusCode::BAD_REQUEST,
                            Json(ErrorResponse {
                                status: "fail",
                                message: err,
                            }),
                        )
                            .into_response()
                    }
                    AddressHandlerError::Unauthorized(err) => {
                        return (
                            StatusCode::UNAUTHORIZED,
                            Json(ErrorResponse {
                                status: "fail",
                                message: err,
                            }),
                        )
                            .into_response()
                    }
                },
                HandlerError::Referral(err) => match err {
                    ReferralHandlerError::ReferralNotFound(err) => (StatusCode::NOT_FOUND, err),
                    ReferralHandlerError::InvalidReferral(err) => (StatusCode::BAD_REQUEST, err),
                    ReferralHandlerError::DuplicateReferral(err) => (StatusCode::CONFLICT, err),
                },
                HandlerError::Task(err) => match err {
                    TaskHandlerError::TaskNotFound(err) => return (StatusCode::NOT_FOUND, err).into_response(),
                    TaskHandlerError::InvalidTaskUrl(err) => return (StatusCode::BAD_REQUEST, err).into_response(),
                    TaskHandlerError::StatusConflict(err) => return (StatusCode::CONFLICT, err).into_response(),
                },
            },

            AppError::Database(err) => {
                error!("{}", err);

                match err {
                    DbError::UniqueViolation(err) => (StatusCode::CONFLICT, err),

                    DbError::RecordNotFound(err) | DbError::AddressNotFound(err) | DbError::TaskNotFound(err) => {
                        (StatusCode::NOT_FOUND, err)
                    }

                    DbError::Database(_) | DbError::InvalidStatus(_) | DbError::Migration(_) => (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        "An internal server error occurred".to_string(),
                    ),
                }
            }

            AppError::Transaction(_)
            | AppError::TaskGenerator(_)
            | AppError::Reverser(_)
            | AppError::Join(_)
            | AppError::Graphql(_)
            | AppError::Config(_)
            | AppError::Http(_)
            | AppError::Server(_) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "An internal server error occurred".to_string(),
            ),
        };

        let body = Json(json!({
            "error": error_message,
        }));

        (status, body).into_response()
    }
}
