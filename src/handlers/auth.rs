use argon2::{Argon2, PasswordHash, PasswordVerifier};
use axum::{
    extract::{Query, State},
    http::StatusCode,
    response::{Json, Redirect},
    Extension,
};
use chrono::Utc;
use jsonwebtoken::{encode, EncodingKey, Header};
use rusx::auth::TwitterCallbackParams;
use tower_cookies::{Cookie, Cookies};
use uuid::Uuid;

use crate::{
    db_persistence::DbError,
    handlers::{HandlerError, SuccessResponse},
    http_server::{AppState, Challenge},
    models::{
        address::{Address, AddressInput},
        admin::{AdminClaims, AdminLoginPayload, AdminLoginResponse},
        auth::{
            GenerateOAuthLinkResponse, OauthTokenQuery, RequestChallengeBody, RequestChallengeResponse, TokenClaims,
            VerifyLoginBody, VerifyLoginResponse,
        },
        x_association::{XAssociation, XAssociationInput},
    },
    services::signature_service::SignatureService,
    utils::{generate_referral_code::generate_referral_code, jwt::get_default_jwt_config},
    AppError,
};
use tracing::{debug, warn};

#[derive(Debug, thiserror::Error)]
pub enum AuthHandlerError {
    #[error("Not authorized: {0}")]
    Unauthrorized(String),
    #[error("Oauth error: {0}")]
    OAuth(String),
}

pub async fn request_challenge(
    State(state): State<AppState>,
    Json(_body): Json<RequestChallengeBody>,
) -> Result<Json<RequestChallengeResponse>, StatusCode> {
    let temp_session_id = Uuid::new_v4().to_string();
    let challenge = Uuid::new_v4().to_string();
    let entry = Challenge {
        id: temp_session_id.clone(),
        challenge: challenge.clone(),
        created_at: Utc::now(),
    };
    state.challenges.write().await.insert(temp_session_id.clone(), entry);
    Ok(Json(RequestChallengeResponse {
        temp_session_id,
        challenge,
    }))
}

pub async fn verify_login(
    State(state): State<AppState>,
    Json(body): Json<VerifyLoginBody>,
) -> Result<Json<VerifyLoginResponse>, AppError> {
    let sig_len = body.signature.strip_prefix("0x").unwrap_or(&body.signature).len();
    let pk_len = body.public_key.strip_prefix("0x").unwrap_or(&body.public_key).len();
    debug!(
        temp_session_id = %body.temp_session_id,
        address = %body.address,
        signature_len = sig_len,
        public_key_len = pk_len,
        "verify_login: received payload"
    );
    let Some(chal) = state.challenges.read().await.get(&body.temp_session_id).cloned() else {
        return Err(AppError::Handler(HandlerError::Auth(AuthHandlerError::Unauthrorized(
            format!("no challenge with key {} found", &body.temp_session_id),
        ))));
    };
    let message = format!(
        "taskmaster:login:1|challenge={}|address={}",
        chal.challenge, body.address
    );
    debug!(message = %message, message_len = message.len(), message_hex = %hex::encode(message.as_bytes()), "verify_login: constructed message");

    let addr_res = SignatureService::verify_address(&body.public_key, &body.address);
    if let Err(e) = &addr_res {
        warn!(error = %e, "verify_login: verify_address error");
    }
    let addr_ok = addr_res.map_err(|_| {
        AppError::Handler(HandlerError::Auth(AuthHandlerError::Unauthrorized(format!(
            "address verification failed"
        ))))
    })?;
    if !addr_ok {
        return Err(AppError::Handler(HandlerError::Auth(AuthHandlerError::Unauthrorized(
            format!("address verification failed"),
        ))));
    }
    let sig_res = SignatureService::verify_message(message.as_bytes(), &body.signature, &body.public_key);
    if let Err(e) = &sig_res {
        warn!(error = %e, "verify_login: verify_message error");
    }
    let sig_ok = sig_res.map_err(|_| {
        AppError::Handler(HandlerError::Auth(AuthHandlerError::Unauthrorized(format!(
            "message verification failed"
        ))))
    })?;
    debug!(addr_ok = addr_ok, sig_ok = sig_ok, "verify_login: verification results");
    if !sig_ok {
        return Err(AppError::Handler(HandlerError::Auth(AuthHandlerError::Unauthrorized(
            format!("message verification failed"),
        ))));
    }

    if state.db.addresses.find_by_id(&body.address).await?.is_none() {
        tracing::info!("Address is not saved yet, proceed to saving...");

        tracing::debug!("Generating address referral code...");
        let referral_code = generate_referral_code(body.address.clone()).await?;

        tracing::debug!("Creating address struct...");
        let address = Address::new(AddressInput {
            quan_address: body.address.clone(),
            referral_code,
        })?;

        tracing::debug!("Saving address to DB...");
        state.db.addresses.create(&address).await?;
    }

    let (iat, exp) = get_default_jwt_config(&state);
    let claims: TokenClaims = TokenClaims {
        sub: body.address,
        iat,
        exp,
    };

    let access_token = encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(state.config.jwt.secret.as_ref()),
    )
    .unwrap();

    state.challenges.write().await.remove(&body.temp_session_id);
    Ok(Json(VerifyLoginResponse { access_token }))
}

pub async fn auth_me(Extension(address): Extension<Address>) -> Result<Json<SuccessResponse<Address>>, StatusCode> {
    Ok(SuccessResponse::new(address))
}

pub async fn handle_x_oauth(
    State(state): State<AppState>,
    cookies: Cookies,
    Query(params): Query<OauthTokenQuery>,
) -> Result<Redirect, AppError> {
    tracing::info!("Handling x oauth request...");

    let quan_address = {
        let Some(address) = state.twitter_oauth_tokens.write().await.remove(&params.token) else {
            return Err(AppError::Handler(HandlerError::Auth(AuthHandlerError::OAuth(
                "Invalid or expired token".to_string(),
            ))));
        };

        address
    };

    tracing::info!("Quan address from token: {}", quan_address);

    let (auth_url, verifier) = state.twitter_gateway.generate_auth_url();
    let session_id = format!("{}|{}", quan_address, uuid::Uuid::new_v4().to_string());

    tracing::info!("Session id in cookies: {}", session_id);

    tracing::info!("Creating oauth session");
    state
        .oauth_sessions
        .lock()
        .unwrap()
        .insert(session_id.clone(), verifier);
    cookies.add(Cookie::new("oauth_session", session_id));

    tracing::info!("Returning oauth url...");

    Ok(Redirect::to(&auth_url))
}

pub async fn generate_x_oauth_link(
    State(state): State<AppState>,
    Extension(user): Extension<Address>,
) -> Result<Json<GenerateOAuthLinkResponse>, AppError> {
    tracing::info!("Generating oauth url...");

    let twitter_oauth_token = Uuid::new_v4().to_string();
    state
        .twitter_oauth_tokens
        .write()
        .await
        .insert(twitter_oauth_token.clone(), user.quan_address.0.clone());

    tracing::info!("Returning oauth request url...");
    let request_link = format!(
        "{}/auth/x?token={}",
        state.config.get_base_api_url(),
        twitter_oauth_token
    );

    Ok(Json(GenerateOAuthLinkResponse { url: request_link }))
}

pub async fn handle_x_oauth_callback(
    State(state): State<AppState>,
    cookies: Cookies,
    Query(params): Query<TwitterCallbackParams>,
) -> Result<Redirect, AppError> {
    tracing::info!("Handling x oauth callback...");

    let session_id = match cookies.get("oauth_session") {
        Some(cookie) => cookie.value().to_string(),
        None => {
            return Err(AppError::Handler(HandlerError::Auth(AuthHandlerError::OAuth(
                "No session cookie found. Please try again.".to_string(),
            ))))
        }
    };

    tracing::info!("Session id found: {}", session_id);

    let verifier = {
        let Some(chal) = state.oauth_sessions.lock().unwrap().remove(&session_id) else {
            return Err(AppError::Handler(HandlerError::Auth(AuthHandlerError::OAuth(format!(
                "Session {} expired or invalid",
                &session_id
            )))));
        };

        chal
    };

    tracing::debug!("Exchanging code {} for access token...", params.code);
    let token = state.twitter_gateway.exchange_code(params.code, verifier).await?;
    let authenticated_gateway = state.twitter_gateway.with_token(token.access_token)?;

    let user_resp = authenticated_gateway.users().get_me().await?;
    let x_handle = user_resp.data.username;

    tracing::debug!("Do X association...");
    let quan_address = {
        let Some(address) = session_id.split_once('|').map(|(left, _)| left) else {
            return Err(AppError::Handler(HandlerError::Auth(AuthHandlerError::OAuth(format!(
                "Session id malformed",
            )))));
        };

        address.to_string()
    };

    let new_association = XAssociation::new(XAssociationInput {
        quan_address,
        username: x_handle,
    })?;

    state.db.x_associations.create(&new_association).await?;
    tracing::info!(
        "Created association for quan_address {} with X username {}",
        new_association.quan_address.0,
        new_association.username
    );

    let redirect_url = format!(
        "{}/oauth?platform=x&payload={}",
        state.config.blockchain.website_url, new_association.username
    );

    Ok(Redirect::to(&redirect_url))
}

pub async fn handle_admin_login(
    State(state): State<AppState>,
    Json(body): Json<AdminLoginPayload>,
) -> Result<Json<AdminLoginResponse>, AppError> {
    tracing::info!("Handling admin login...");

    let admin = state
        .db
        .admin
        .find_by_username(&body.username)
        .await?
        .ok_or(AppError::Database(DbError::RecordNotFound(format!(
            "Admin with username {} is not exist",
            &body.username,
        ))))?;

    let parsed_hash =
        PasswordHash::new(&admin.password).map_err(|_| AppError::Server("Failed generating token".to_string()))?;

    Argon2::default()
        .verify_password(body.password.as_bytes(), &parsed_hash)
        .map_err(|_| {
            HandlerError::Auth(AuthHandlerError::Unauthrorized(
                "Invalid username or password".to_string(),
            ))
        })?;

    let (iat, exp) = get_default_jwt_config(&state);
    let claims: AdminClaims = AdminClaims {
        sub: admin.id.to_string(),
        iat,
        exp,
    };

    tracing::info!("Generating admin token...");

    let access_token = encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(state.config.jwt.admin_secret.as_ref()),
    )
    .unwrap();

    Ok(Json(AdminLoginResponse { access_token }))
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use crate::{
        handlers::auth::handle_x_oauth_callback,
        http_server::AppState,
        routes::auth::auth_routes,
        utils::{
            test_app_state::create_test_app_state,
            test_db::{create_persisted_address, reset_database},
        },
    };
    use axum::{body::Body, http, routing::get};
    use rusx::{
        auth::TwitterToken,
        resources::user::{User, UserApi, UserResponse},
        MockTwitterGateway, MockUserApi, PkceCodeVerifier, TwitterGateway,
    };
    use sp_core::crypto::{self, Ss58AddressFormat, Ss58Codec};
    use sp_runtime::traits::IdentifyAccount;
    use tower::ServiceExt;
    use tower_cookies::CookieManagerLayer;

    async fn test_app() -> axum::Router {
        let state = create_test_app_state().await;
        auth_routes(state.clone()).with_state(state)
    }

    fn auth_callback_router(state: AppState) -> axum::Router {
        axum::Router::new()
            .route("/auth/x/callback", get(handle_x_oauth_callback))
            .layer(CookieManagerLayer::new()) // Crucial for testing Cookies
            .with_state(state)
    }

    #[tokio::test]
    async fn test_x_oauth_callback_invalid_session() {
        let state = create_test_app_state().await;
        let app = auth_callback_router(state);

        // We send a cookie, but we DO NOT add anything to state.oauth_sessions
        let response = app
            .oneshot(
                http::Request::builder()
                    .method("GET")
                    .uri("/auth/x/callback?code=123&state=abc")
                    .header(http::header::COOKIE, "oauth_session=invalid_session_id")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        // Should fail because session id is not in the HashMap
        assert_ne!(response.status(), http::StatusCode::SEE_OTHER);
    }

    #[tokio::test]
    async fn test_x_oauth_callback_missing_cookie() {
        let state = create_test_app_state().await;
        let app = auth_callback_router(state);

        let response = app
            .oneshot(
                http::Request::builder()
                    .method("GET")
                    // No Cookie Header
                    .uri("/auth/x/callback?code=123&state=abc")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        // Should fail because cookie is missing
        // Assuming AppError maps to something other than 307 Redirect (likely 400 or 500)
        assert_ne!(response.status(), http::StatusCode::SEE_OTHER);
        assert_ne!(response.status(), http::StatusCode::OK);
    }

    #[tokio::test]
    async fn test_x_oauth_callback_success() {
        // 1. Setup Data
        let mut state = create_test_app_state().await;
        reset_database(&state.db.pool).await;

        let test_user = create_persisted_address(&state.db.addresses, "101").await;
        let session_uuid = "random-uuid";
        let session_id = format!("{}|{}", test_user.quan_address.0, session_uuid);
        let verifier = PkceCodeVerifier::new("random".to_string());
        let expected_username = "quantus";

        // 2. Prepare Mocks

        // A. Mock the User API
        let mut mock_user_api = MockUserApi::new();
        mock_user_api.expect_get_me().times(1).returning(move || {
            Ok(UserResponse {
                data: User {
                    id: "101".to_string(),
                    name: "Quantus Network".to_string(),
                    username: expected_username.to_string(),
                },
            })
        });

        // B. Mock the Authenticated Gateway
        let mut mock_auth_gateway = MockTwitterGateway::new();

        // Explicit cast to Arc<dyn UserApi> for return_const
        let user_api_arc: Arc<dyn UserApi> = Arc::new(mock_user_api);
        mock_auth_gateway.expect_users().times(1).return_const(user_api_arc);

        // Prepare the gateway to be returned by with_token
        // Note: We also cast this to Arc<dyn TwitterGateway> to ensure the closure return type matches perfectly
        let auth_gateway_arc: Arc<dyn TwitterGateway> = Arc::new(mock_auth_gateway);

        // C. Mock the Main Gateway (Entry point)
        let mut mock_main_gateway = MockTwitterGateway::new();

        // Expect code exchange
        mock_main_gateway
            .expect_exchange_code()
            // Use .to_string() inside eq() because the function argument is String
            .with(
                mockall::predicate::eq("valid_auth_code".to_string()),
                mockall::predicate::always(),
            )
            .times(1)
            .returning(|_, _| {
                Ok(TwitterToken {
                    access_token: "mock_access_token".to_string(),
                    refresh_token: None,
                    expires_in: None,
                })
            });

        // Expect transition to authenticated state
        let result_gateway = auth_gateway_arc.clone();
        mock_main_gateway
            .expect_with_token()
            // Use .to_string() inside eq() here as well
            .with(mockall::predicate::eq("mock_access_token".to_string()))
            .times(1)
            // returning expects a closure that returns SdkResult<Arc<dyn TwitterGateway>>
            // Since we cast auth_gateway_arc above, this now matches perfectly
            .returning(move |_| Ok(result_gateway.clone()));

        // 3. Inject the mock!
        state.twitter_gateway = Arc::new(mock_main_gateway);

        // Populate the session store
        state
            .oauth_sessions
            .lock()
            .unwrap()
            .insert(session_id.clone(), verifier);

        // 4. Create Router
        let app = auth_callback_router(state.clone());

        // 5. Execute Request
        let response = app
            .oneshot(
                http::Request::builder()
                    .method("GET")
                    .uri("/auth/x/callback?code=valid_auth_code&state=xyz")
                    .header(http::header::COOKIE, format!("oauth_session={}", session_id))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        // 6. Assertions
        assert_eq!(response.status(), http::StatusCode::SEE_OTHER);

        // Check Redirect Location
        let location = response.headers().get("location").unwrap().to_str().unwrap();
        assert!(location.contains(&format!("payload={}", expected_username)));

        // Check DB Side Effects
        let saved_assoc = state
            .db
            .x_associations
            .find_by_username(expected_username)
            .await
            .unwrap();

        assert!(saved_assoc.is_some());
        assert_eq!(saved_assoc.unwrap().quan_address.0, test_user.quan_address.0);
    }

    #[tokio::test]
    async fn auth_challenge_and_verify_flow() {
        crypto::set_default_ss58_version(Ss58AddressFormat::custom(189));
        let app = test_app().await;

        let resp = app
            .clone()
            .oneshot(
                http::Request::builder()
                    .method("POST")
                    .uri("/auth/request-challenge")
                    .header(http::header::CONTENT_TYPE, "application/json")
                    .body(Body::from("{}"))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), http::StatusCode::OK);
        let bytes = axum::body::to_bytes(resp.into_body(), 1024 * 1024).await.unwrap();
        let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        let temp_session_id = v["temp_session_id"].as_str().unwrap().to_string();
        let challenge = v["challenge"].as_str().unwrap().to_string();
        let entropy = [3u8; 32];
        let kp = qp_rusty_crystals_dilithium::ml_dsa_87::Keypair::generate(&entropy);
        let pk_hex = hex::encode(kp.public.to_bytes());
        let addr = quantus_cli::qp_dilithium_crypto::types::DilithiumPublic::try_from(kp.public.to_bytes().as_slice())
            .unwrap()
            .into_account()
            .to_ss58check();
        let msg = format!("taskmaster:login:1|challenge={}|address={}", challenge, addr);
        let sig_hex = hex::encode(kp.sign(msg.as_bytes(), None, Some([7u8; 32])));

        let verify_payload = serde_json::json!({
            "temp_session_id": temp_session_id,
            "address": addr,
            "public_key": pk_hex,
            "signature": sig_hex,
        });
        let resp = app
            .clone()
            .oneshot(
                http::Request::builder()
                    .method("POST")
                    .uri("/auth/verify")
                    .header(http::header::CONTENT_TYPE, "application/json")
                    .body(Body::from(serde_json::to_vec(&verify_payload).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), http::StatusCode::OK);
        let bytes = axum::body::to_bytes(resp.into_body(), 1024 * 1024).await.unwrap();
        let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        let access_token = v["access_token"].as_str().unwrap();

        let resp = app
            .clone()
            .oneshot(
                http::Request::builder()
                    .method("GET")
                    .uri("/auth/me")
                    .header(http::header::AUTHORIZATION, format!("Bearer {}", access_token))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), http::StatusCode::OK);
    }
}
