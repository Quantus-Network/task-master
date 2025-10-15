use axum::Json;
use serde::Serialize;

use crate::handlers::{
    address::AddressHandlerError, auth::AuthHandlerError, referral::ReferralHandlerError, task::TaskHandlerError
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
