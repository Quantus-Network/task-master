use axum::{
    extract::{Request, State},
    http::{header, StatusCode},
    middleware::Next,
    response::IntoResponse,
    Json,
};

use jsonwebtoken::{decode, DecodingKey, Validation};
use serde::Serialize;

use crate::{
    http_server::AppState,
    models::auth::{TokenClaims, TokenPurpose},
};

#[derive(Debug, Serialize)]
pub struct ErrorResponse {
    pub status: &'static str,
    pub message: String,
}

pub async fn jwt_auth(
    State(state): State<AppState>,
    mut req: Request,
    next: Next,
) -> Result<impl IntoResponse, (StatusCode, Json<ErrorResponse>)> {
    let token = req
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|auth_header| auth_header.to_str().ok())
        .and_then(|auth_value| {
            if auth_value.starts_with("Bearer ") {
                Some(auth_value[7..].to_owned())
            } else {
                None
            }
        });

    let token = token.ok_or_else(|| {
        let json_error = ErrorResponse {
            status: "fail",
            message: "You are not logged in, please provide token".to_string(),
        };
        (StatusCode::UNAUTHORIZED, Json(json_error))
    })?;

    let claims = decode::<TokenClaims>(
        &token,
        &DecodingKey::from_secret(state.config.jwt.secret.as_ref()),
        &Validation::default(),
    )
    .map_err(|_| {
        let json_error = ErrorResponse {
            status: "fail",
            message: "Invalid token".to_string(),
        };
        (StatusCode::UNAUTHORIZED, Json(json_error))
    })?
    .claims;

    if claims.purpose != TokenPurpose::Auth {
        let json_error = ErrorResponse {
            status: "fail",
            message: "Invalid token purpose".to_string(),
        };

        return Err((StatusCode::UNAUTHORIZED, Json(json_error)));
    }

    let user_id = &claims.sub;

    let user = state.db.addresses.find_by_id(user_id).await.map_err(|e| {
        let json_error = ErrorResponse {
            status: "fail",
            message: format!("Error fetching user from database: {}", e),
        };
        (StatusCode::INTERNAL_SERVER_ERROR, Json(json_error))
    })?;

    let user = user.ok_or_else(|| {
        let json_error = ErrorResponse {
            status: "fail",
            message: "The user belonging to this token not exists".to_string(),
        };
        (StatusCode::UNAUTHORIZED, Json(json_error))
    })?;

    req.extensions_mut().insert(user);
    Ok(next.run(req).await)
}
