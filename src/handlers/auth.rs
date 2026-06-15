use argon2::{Argon2, PasswordHash, PasswordVerifier};
use axum::{extract::State, http::StatusCode, response::Json, Extension};
use chrono::Utc;
use jsonwebtoken::{encode, EncodingKey, Header};
use uuid::Uuid;

use crate::{
    db_persistence::DbError,
    handlers::{HandlerError, SuccessResponse},
    http_server::{AppState, Challenge},
    models::{
        address::{Address, AddressInput},
        admin::{Admin, AdminAuthCheckResponse, AdminClaims, AdminLoginPayload, AdminLoginResponse},
        auth::{RequestChallengeBody, RequestChallengeResponse, TokenClaims, VerifyLoginBody, VerifyLoginResponse},
    },
    services::signature_service::SignatureService,
    utils::{generate_referral_code::generate_referral_code, jwt::get_default_jwt_config},
    AppError,
};
use tracing::{debug, warn};

#[derive(Debug, thiserror::Error)]
pub enum AuthHandlerError {
    #[error("Not authorized: {0}")]
    Unauthorized(String),
}

pub async fn request_challenge(
    State(state): State<AppState>,
    Json(_body): Json<RequestChallengeBody>,
) -> Result<Json<RequestChallengeResponse>, StatusCode> {
    let temp_session_id = Uuid::new_v4().to_string();
    let challenge = Uuid::new_v4().to_string();
    let entry = Challenge {
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
        return Err(AppError::Handler(HandlerError::Auth(AuthHandlerError::Unauthorized(
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
        AppError::Handler(HandlerError::Auth(AuthHandlerError::Unauthorized(
            "address verification failed".to_string(),
        )))
    })?;
    if !addr_ok {
        return Err(AppError::Handler(HandlerError::Auth(AuthHandlerError::Unauthorized(
            "address verification failed".to_string(),
        ))));
    }
    let sig_res = SignatureService::verify_message(message.as_bytes(), &body.signature, &body.public_key);
    if let Err(e) = &sig_res {
        warn!(error = %e, "verify_login: verify_message error");
    }
    let sig_ok = sig_res.map_err(|_| {
        AppError::Handler(HandlerError::Auth(AuthHandlerError::Unauthorized(
            "message verification failed".to_string(),
        )))
    })?;
    debug!(addr_ok = addr_ok, sig_ok = sig_ok, "verify_login: verification results");
    if !sig_ok {
        return Err(AppError::Handler(HandlerError::Auth(AuthHandlerError::Unauthorized(
            "message verification failed".to_string(),
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
            HandlerError::Auth(AuthHandlerError::Unauthorized(
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

pub async fn auth_admin(
    Extension(admin): Extension<Admin>,
) -> Result<Json<SuccessResponse<AdminAuthCheckResponse>>, StatusCode> {
    Ok(SuccessResponse::new(AdminAuthCheckResponse {
        id: admin.id,
        username: admin.username,
    }))
}

#[cfg(test)]
mod tests {
    use crate::{routes::auth::auth_routes, utils::test_app_state::create_test_app_state};
    use axum::{body::Body, http};
    use qp_rusty_crystals_dilithium::SensitiveBytes32;
    use sp_core::crypto::{self, Ss58AddressFormat, Ss58Codec};
    use sp_runtime::traits::IdentifyAccount;
    use tower::ServiceExt;

    async fn test_app() -> axum::Router {
        let state = create_test_app_state().await;
        auth_routes(state.clone()).with_state(state)
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
        let entropy = SensitiveBytes32::from(&mut [3u8; 32]);
        let kp = qp_rusty_crystals_dilithium::ml_dsa_87::Keypair::generate(entropy);
        let pk_hex = hex::encode(kp.public.to_bytes());
        let addr = quantus_cli::qp_dilithium_crypto::types::DilithiumPublic::try_from(kp.public.to_bytes().as_slice())
            .unwrap()
            .into_account()
            .to_ss58check();
        let msg = format!("taskmaster:login:1|challenge={}|address={}", challenge, addr);
        let sig_hex = hex::encode(kp.sign(msg.as_bytes(), None, Some([7u8; 32])).unwrap());

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
