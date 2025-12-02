use axum::Json;
use serde::{Deserialize, Serialize};

use crate::{
    handlers::{
        address::AddressHandlerError, auth::AuthHandlerError, referral::ReferralHandlerError, task::TaskHandlerError,
    },
    AppError,
};

pub mod address;
pub mod auth;
pub mod referral;
pub mod task;

#[derive(Debug, thiserror::Error)]
pub enum HandlerError {
    #[error("Task handler error")]
    Task(#[from] TaskHandlerError),
    #[error("Referral handler error")]
    Referral(#[from] ReferralHandlerError),
    #[error("Address handler error")]
    Address(#[from] AddressHandlerError),
    #[error("Auth handler error")]
    Auth(#[from] AuthHandlerError),

    #[error("{0}")]
    QueryParams(String),
}

#[derive(Debug, Serialize)]
pub struct SuccessResponse<T> {
    data: T,
}
impl<T> SuccessResponse<T> {
    pub fn new(data: T) -> Json<Self> {
        Json(Self { data })
    }
}

#[derive(Debug, Serialize)]
pub struct ErrorResponse {
    pub status: &'static str,
    pub message: String,
}

#[derive(Debug, Serialize)]
pub struct PaginationMetadata {
    pub page: u32,
    pub page_size: u32,
    pub total_items: u32,
    pub total_pages: u32,
}

#[derive(Debug, Deserialize)]
pub struct QueryParams {
    // Pagination
    #[serde(default = "default_page")]
    pub page: u32,

    #[serde(default = "default_page_size")]
    pub page_size: u32,

    pub referral_code: Option<String>,
}

// Default values for query params
fn default_page() -> u32 {
    1
}
fn default_page_size() -> u32 {
    25
}

pub fn validate_pagination_query(params: &QueryParams) -> Result<(), AppError> {
    if params.page < 1 {
        return Err(AppError::Handler(HandlerError::QueryParams(
            "Page query params must not be less than 1".to_string(),
        )));
    }

    if params.page_size < 1 {
        return Err(AppError::Handler(HandlerError::QueryParams(
            "Page size query params must not be less than 1".to_string(),
        )));
    }

    Ok(())
}
