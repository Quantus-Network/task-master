use axum::{
    extract::Request,
    http::{header, StatusCode},
    Json,
};

use crate::{handlers::ErrorResponse, http_server::AppState};

pub fn get_default_jwt_config(state: &AppState) -> (usize, usize) {
    let now = chrono::Utc::now();
    let iat = now.timestamp() as usize;
    let exp = now
        .checked_add_signed(state.config.get_jwt_expiration())
        .expect("valid timestamp")
        .timestamp() as usize;

    (iat, exp)
}

pub fn extract_jwt_token_from_request(req: &Request) -> Result<String, (StatusCode, Json<ErrorResponse>)> {
    let token = req
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|auth_header| auth_header.to_str().ok())
        .and_then(|auth_value| auth_value.strip_prefix("Bearer ").map(|s| s.to_owned()));

    token.ok_or_else(|| {
        let json_error = ErrorResponse {
            status: "fail",
            message: "You are not logged in, please provide token".to_string(),
        };

        (StatusCode::UNAUTHORIZED, Json(json_error))
    })
}
