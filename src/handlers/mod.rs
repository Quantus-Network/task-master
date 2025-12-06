use axum::Json;
use serde::{Deserialize, Serialize};
use std::fmt::Display;

use crate::{
    handlers::{
        address::AddressHandlerError, auth::AuthHandlerError, referral::ReferralHandlerError, task::TaskHandlerError,
    },
    AppError,
};

pub mod address;
pub mod auth;
pub mod referral;
pub mod relevant_tweet;
pub mod task;
pub mod tweet_author;

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

#[derive(Debug, Serialize)]
pub struct PaginatedResponse<T> {
    pub data: Vec<T>,
    pub meta: PaginationMetadata,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SortDirection {
    Asc,
    Desc,
}

impl Display for SortDirection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SortDirection::Asc => write!(f, "ASC"),
            SortDirection::Desc => write!(f, "DESC"),
        }
    }
}

// T is the Enum defining valid sort columns for a specific resource
#[derive(Debug, Deserialize)]
pub struct ListQueryParams<T> {
    #[serde(default = "default_page")]
    pub page: u32,

    #[serde(default = "default_page_size")]
    pub page_size: u32,

    pub search: Option<String>,

    pub sort_by: Option<T>,

    #[serde(default = "default_sort_direction")]
    pub order: SortDirection,
}

#[derive(Debug, Deserialize)]
pub struct LeaderboardQueryParams {
    #[serde(default = "default_page")]
    pub page: u32,

    #[serde(default = "default_page_size")]
    pub page_size: u32,

    pub referral_code: Option<String>,
}

fn default_page() -> u32 {
    1
}
fn default_page_size() -> u32 {
    25
}
fn default_sort_direction() -> SortDirection {
    SortDirection::Desc
}

pub fn validate_pagination_query(page: u32, page_size: u32) -> Result<(), AppError> {
    if page < 1 {
        return Err(AppError::Handler(HandlerError::QueryParams(
            "Page query params must not be less than 1".to_string(),
        )));
    }

    if page_size < 1 {
        return Err(AppError::Handler(HandlerError::QueryParams(
            "Page size query params must not be less than 1".to_string(),
        )));
    }

    Ok(())
}

fn calculate_total_pages(page_size: u32, total_items: u32) -> u32 {
    let total_pages = ((total_items as f64) / (page_size as f64)).ceil() as u32;

    total_pages
}
