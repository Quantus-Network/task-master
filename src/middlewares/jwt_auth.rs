use axum::{
    extract::{Request, State},
    http::StatusCode,
    middleware::Next,
    response::IntoResponse,
    Json,
};

use jsonwebtoken::{decode, DecodingKey, Validation};
use uuid::Uuid;

use crate::{
    handlers::ErrorResponse,
    http_server::AppState,
    models::{
        admin::AdminClaims,
        auth::{TokenClaims, TokenPurpose},
    },
    utils::jwt::extract_jwt_token_from_request,
};

pub async fn jwt_auth(
    State(state): State<AppState>,
    mut req: Request,
    next: Next,
) -> Result<impl IntoResponse, (StatusCode, Json<ErrorResponse>)> {
    let token = extract_jwt_token_from_request(&req)?;

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

pub async fn jwt_admin_auth(
    State(state): State<AppState>,
    mut req: Request,
    next: Next,
) -> Result<impl IntoResponse, (StatusCode, Json<ErrorResponse>)> {
    let token = extract_jwt_token_from_request(&req)?;

    let claims = decode::<AdminClaims>(
        &token,
        &DecodingKey::from_secret(state.config.jwt.admin_secret.as_ref()),
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

    let admin_id = Uuid::parse_str(&claims.sub).map_err(|_| {
        let json_error = ErrorResponse {
            status: "fail",
            message: "Invalid token".to_string(),
        };
        (StatusCode::UNAUTHORIZED, Json(json_error))
    })?;

    let admin = state.db.admin.find_by_id(&admin_id).await.map_err(|e| {
        let json_error = ErrorResponse {
            status: "fail",
            message: format!("Error fetching admin from database: {}", e),
        };
        (StatusCode::INTERNAL_SERVER_ERROR, Json(json_error))
    })?;

    let admin = admin.ok_or_else(|| {
        let json_error = ErrorResponse {
            status: "fail",
            message: "The admin belonging to this token not exists".to_string(),
        };
        (StatusCode::UNAUTHORIZED, Json(json_error))
    })?;

    req.extensions_mut().insert(admin);
    Ok(next.run(req).await)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        models::{
            address::Address,
            admin::Admin,
            auth::{TokenClaims, TokenPurpose},
        },
        utils::{
            test_app_state::{create_test_app_state, generate_test_token}, // Assuming you have these from previous context
            test_db::{create_persisted_address, reset_database},
        },
    };
    use axum::{
        body::Body,
        http::{self, Request, StatusCode},
        middleware::from_fn_with_state,
        routing::get,
        Extension, Router,
    };
    use chrono::{Duration, Utc};
    use jsonwebtoken::{encode, EncodingKey, Header};
    use tower::ServiceExt;

    // Helper handler to verify middleware passed
    async fn protected_handler(Extension(user): Extension<Address>) -> impl IntoResponse {
        format!("Welcome {}", user.quan_address.0)
    }

    // Helper handler for admin
    async fn protected_admin_handler(Extension(admin): Extension<Admin>) -> impl IntoResponse {
        format!("Welcome Admin {}", admin.id) // Adjust 'id' to whatever field Admin has
    }

    // Helper to generate a token with specific purpose (for failure tests)
    fn generate_token_with_purpose(secret: &str, sub: &str, purpose: TokenPurpose) -> String {
        let claims = TokenClaims {
            sub: sub.to_string(),
            purpose,
            exp: (Utc::now() + Duration::hours(1)).timestamp() as usize,
            iat: Utc::now().timestamp() as usize,
        };

        encode(
            &Header::default(),
            &claims,
            &EncodingKey::from_secret(secret.as_bytes()),
        )
        .unwrap()
    }

    #[tokio::test]
    async fn test_jwt_auth_success() {
        let state = create_test_app_state().await;
        reset_database(&state.db.pool).await;

        // 1. Setup User
        let user = create_persisted_address(&state.db.addresses, "auth_user_1").await;

        // 2. Generate Valid Token
        let token = generate_test_token(&state.config.jwt.secret, &user.quan_address.0);

        // 3. Setup Router with Middleware
        let router = Router::new()
            .route("/protected", get(protected_handler))
            .layer(from_fn_with_state(state.clone(), jwt_auth))
            .with_state(state);

        // 4. Send Request
        let response = router
            .oneshot(
                Request::builder()
                    .uri("/protected")
                    .header(http::header::AUTHORIZATION, format!("Bearer {}", token))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        // 5. Assert Success
        assert_eq!(response.status(), StatusCode::OK);

        let body_bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let body_str = String::from_utf8(body_bytes.to_vec()).unwrap();
        assert_eq!(body_str, format!("Welcome {}", user.quan_address.0));
    }

    #[tokio::test]
    async fn test_jwt_auth_fails_invalid_token() {
        let state = create_test_app_state().await;

        let router = Router::new()
            .route("/protected", get(protected_handler))
            .layer(from_fn_with_state(state.clone(), jwt_auth))
            .with_state(state);

        let response = router
            .oneshot(
                Request::builder()
                    .uri("/protected")
                    .header(http::header::AUTHORIZATION, "Bearer invalid_token_string")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn test_jwt_auth_fails_wrong_purpose() {
        let state = create_test_app_state().await;
        reset_database(&state.db.pool).await;

        let user = create_persisted_address(&state.db.addresses, "auth_user_2").await;

        // Generate token with WRONG purpose (e.g., Refresh instead of Auth)
        // Assuming TokenPurpose::Refresh exists in your enum
        let token = generate_token_with_purpose(&state.config.jwt.secret, &user.quan_address.0, TokenPurpose::Oauth);

        let router = Router::new()
            .route("/protected", get(protected_handler))
            .layer(from_fn_with_state(state.clone(), jwt_auth))
            .with_state(state);

        let response = router
            .oneshot(
                Request::builder()
                    .uri("/protected")
                    .header(http::header::AUTHORIZATION, format!("Bearer {}", token))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

        let body_bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let body_json: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
        assert_eq!(body_json["message"], "Invalid token purpose");
    }

    #[tokio::test]
    async fn test_jwt_auth_fails_user_not_found_in_db() {
        let state = create_test_app_state().await;
        reset_database(&state.db.pool).await;

        // Generate token for a user ID that does NOT exist in DB
        let token = generate_test_token(&state.config.jwt.secret, "non_existent_user_id");

        let router = Router::new()
            .route("/protected", get(protected_handler))
            .layer(from_fn_with_state(state.clone(), jwt_auth))
            .with_state(state);

        let response = router
            .oneshot(
                Request::builder()
                    .uri("/protected")
                    .header(http::header::AUTHORIZATION, format!("Bearer {}", token))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

        let body_bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let body_json: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
        assert_eq!(body_json["message"], "The user belonging to this token not exists");
    }

    #[tokio::test]
    async fn test_jwt_auth_fails_missing_header() {
        let state = create_test_app_state().await;

        let router = Router::new()
            .route("/protected", get(protected_handler))
            .layer(from_fn_with_state(state.clone(), jwt_auth))
            .with_state(state);

        // Request WITHOUT Authorization header
        let response = router
            .oneshot(Request::builder().uri("/protected").body(Body::empty()).unwrap())
            .await
            .unwrap();

        // Expecting whatever extract_jwt_token_from_request returns (usually 400 or 401)
        // Assuming 401 for this assert, check your extraction logic if it returns 400
        assert!(response.status() == StatusCode::UNAUTHORIZED || response.status() == StatusCode::BAD_REQUEST);
    }

    // --- ADMIN TESTS ---

    #[tokio::test]
    async fn test_jwt_admin_auth_success() {
        let state = create_test_app_state().await;
        reset_database(&state.db.pool).await;

        // 1. Setup Admin in DB
        // Use your actual method to create an admin.
        // If you don't have a helper, raw SQL works too.
        let admin_id = Uuid::new_v4();
        // Assuming you have a way to insert this. Example using raw query if helper missing:
        sqlx::query("INSERT INTO admins (id, username, password) VALUES ($1, $2, $3)")
            .bind(admin_id)
            .bind("admin_user")
            .bind("hash")
            .execute(&state.db.pool)
            .await
            .expect("Failed to seed admin");

        // 2. Generate Admin Token (Using ADMIN secret)
        let claims = AdminClaims {
            sub: admin_id.to_string(),
            exp: (Utc::now() + Duration::hours(1)).timestamp() as usize,
            iat: Utc::now().timestamp() as usize,
        };
        let token = encode(
            &Header::default(),
            &claims,
            &EncodingKey::from_secret(state.config.jwt.admin_secret.as_bytes()),
        )
        .unwrap();

        // 3. Router
        let router = Router::new()
            .route("/admin/protected", get(protected_admin_handler))
            .layer(from_fn_with_state(state.clone(), jwt_admin_auth))
            .with_state(state);

        // 4. Request
        let response = router
            .oneshot(
                Request::builder()
                    .uri("/admin/protected")
                    .header(http::header::AUTHORIZATION, format!("Bearer {}", token))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        // 5. Assert
        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_jwt_admin_auth_fails_wrong_secret() {
        let state = create_test_app_state().await;

        // Create a token using the USER secret, not the ADMIN secret
        let claims = AdminClaims {
            sub: "some_admin".to_string(),
            exp: (Utc::now() + Duration::hours(1)).timestamp() as usize,
            iat: Utc::now().timestamp() as usize,
        };

        // Mismatch: Encoding with regular secret
        let token = encode(
            &Header::default(),
            &claims,
            &EncodingKey::from_secret(state.config.jwt.secret.as_bytes()),
        )
        .unwrap();

        let router = Router::new()
            .route("/admin/protected", get(protected_admin_handler))
            .layer(from_fn_with_state(state.clone(), jwt_admin_auth))
            .with_state(state);

        let response = router
            .oneshot(
                Request::builder()
                    .uri("/admin/protected")
                    .header(http::header::AUTHORIZATION, format!("Bearer {}", token))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }
}
