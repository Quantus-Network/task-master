use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde_json::json;
use tracing::error;

use crate::{
    db_persistence::DbError,
    handlers::{
        address::AddressHandlerError, auth::AuthHandlerError, referral::ReferralHandlerError, task::TaskHandlerError, HandlerError
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
}

pub type AppResult<T> = Result<T, AppError>;

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, error_message) = match self {
            AppError::Model(err) => (StatusCode::BAD_REQUEST, err.to_string()),

            AppError::Handler(err) => match err {
                HandlerError::Auth(err) => match err {
                    AuthHandlerError::Unauthrorized(err) => (StatusCode::UNAUTHORIZED, err),
                },
                HandlerError::Address(err) => match err {
                    AddressHandlerError::InvalidSignature(err) => {
                        return (StatusCode::BAD_REQUEST, err).into_response()
                    }
                    AddressHandlerError::Unauthrorized(err) => {
                        return (StatusCode::UNAUTHORIZED, err).into_response()
                    }
                },
                HandlerError::Referral(err) => match err {
                    
                    ReferralHandlerError::ReferralNotFound(err) => (StatusCode::NOT_FOUND, err),
                    ReferralHandlerError::InvalidReferral(err) => (StatusCode::BAD_REQUEST, err),
                },
                HandlerError::Task(err) => match err {
                    TaskHandlerError::TaskNotFound(err) => {
                        return (StatusCode::NOT_FOUND, err).into_response()
                    }
                    TaskHandlerError::InvalidTaskUrl(err) => {
                        return (StatusCode::BAD_REQUEST, err).into_response()
                    }
                    TaskHandlerError::StatusConflict(err) => {
                        return (StatusCode::CONFLICT, err).into_response()
                    }
                },
            },

            AppError::Database(err) => {
                error!("{}", err);

                match err {
                    DbError::RecordNotFound(err)
                    | DbError::AddressNotFound(err)
                    | DbError::TaskNotFound(err) => (StatusCode::NOT_FOUND, err),

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
