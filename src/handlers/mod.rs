use axum::Json;
use serde::Serialize;

pub mod referral;
pub mod address;
pub mod auth;

#[derive(Debug, Serialize)]
pub struct SuccessResponse<T> {
    data: T,
}
impl<T> SuccessResponse<T> {
    pub fn new(data: T) -> Json<Self> {
        Json(Self { data })
    }
}
