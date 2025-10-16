use axum::{extract::State, http::StatusCode, response::Json, Extension};
use chrono::Utc;
use jsonwebtoken::{encode, EncodingKey, Header};
use uuid::Uuid;

use crate::{
    handlers::{HandlerError, SuccessResponse},
    http_server::{AppState, Challenge},
    models::{
        address::{Address, AddressInput},
        auth::{
            RequestChallengeBody, RequestChallengeResponse, TokenClaims, VerifyLoginBody,
            VerifyLoginResponse,
        },
    },
    services::signature_service::SignatureService,
    utils::generate_referral_code::generate_referral_code,
    AppError,
};
use tracing::{debug, info, warn};

#[derive(Debug, thiserror::Error)]
pub enum AuthHandlerError {
    #[error("Not authorized: {0}")]
    Unauthrorized(String),
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
    state
        .challenges
        .write()
        .await
        .insert(temp_session_id.clone(), entry);
    Ok(Json(RequestChallengeResponse {
        temp_session_id,
        challenge,
    }))
}

pub async fn verify_login(
    State(state): State<AppState>,
    Json(body): Json<VerifyLoginBody>,
) -> Result<Json<VerifyLoginResponse>, AppError> {
    let sig_len = body
        .signature
        .strip_prefix("0x")
        .unwrap_or(&body.signature)
        .len();
    let pk_len = body
        .public_key
        .strip_prefix("0x")
        .unwrap_or(&body.public_key)
        .len();
    debug!(
        temp_session_id = %body.temp_session_id,
        address = %body.address,
        signature_len = sig_len,
        public_key_len = pk_len,
        "verify_login: received payload"
    );
    let Some(chal) = state
        .challenges
        .read()
        .await
        .get(&body.temp_session_id)
        .cloned()
    else {
        return Err(AppError::Handler(HandlerError::Auth(
            AuthHandlerError::Unauthrorized(format!(
                "no challenge with key {} found",
                &body.temp_session_id
            )),
        )));
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
        AppError::Handler(HandlerError::Auth(AuthHandlerError::Unauthrorized(
            format!("address verification failed"),
        )))
    })?;
    if !addr_ok {
        return Err(AppError::Handler(HandlerError::Auth(
            AuthHandlerError::Unauthrorized(format!("address verification failed")),
        )));
    }
    let sig_res =
        SignatureService::verify_message(message.as_bytes(), &body.signature, &body.public_key);
    if let Err(e) = &sig_res {
        warn!(error = %e, "verify_login: verify_message error");
    }
    let sig_ok = sig_res.map_err(|_| {
        AppError::Handler(HandlerError::Auth(AuthHandlerError::Unauthrorized(
            format!("message verification failed"),
        )))
    })?;
    debug!(
        addr_ok = addr_ok,
        sig_ok = sig_ok,
        "verify_login: verification results"
    );
    if !sig_ok {
        return Err(AppError::Handler(HandlerError::Auth(
            AuthHandlerError::Unauthrorized(format!("message verification failed")),
        )));
    }

    if state
        .db
        .addresses
        .find_by_id(&body.address)
        .await?
        .is_none()
    {
        tracing::info!("Address is not saved yet, proceed to saving...");

        tracing::debug!("Generating address referral code...");
        let referral_code = generate_referral_code(body.address.clone()).await?;

        tracing::debug!("Creating address struct...");
        let address = Address::new(AddressInput {
            quan_address: body.address.clone(),
            eth_address: None,
            referral_code,
        })?;

        tracing::debug!("Saving address to DB...");
        state.db.addresses.create(&address).await?;
    }

    let now = chrono::Utc::now();
    let iat = now.timestamp() as usize;
    let exp = now
        .checked_add_signed(state.config.get_jwt_expiration())
        .expect("valid timestamp")
        .timestamp() as usize;
    let claims: TokenClaims = TokenClaims {
        sub: body.address,
        exp,
        iat,
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

pub async fn auth_me(
    Extension(address): Extension<Address>,
) -> Result<Json<SuccessResponse<Address>>, StatusCode> {
    Ok(SuccessResponse::new(address))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{db_persistence::DbPersistence, routes::auth::auth_routes, Config, GraphqlClient};
    use axum::{body::Body, http};
    use sp_core::crypto::{self, Ss58AddressFormat, Ss58Codec};
    use sp_runtime::traits::IdentifyAccount;
    use std::sync::Arc;
    use tower::ServiceExt;

    async fn test_app() -> axum::Router {
        let config = Config::load_test_env().expect("Failed to load test configuration");
        let db = DbPersistence::new_unmigrated(config.get_database_url())
            .await
            .unwrap();
        let graphql_client = GraphqlClient::new(db.clone(), config.candidates.graphql_url.clone());

        let state = AppState {
            db: Arc::new(db),
            graphql_client: Arc::new(graphql_client),
            config: Arc::new(config),
            challenges: Arc::new(tokio::sync::RwLock::new(std::collections::HashMap::new())),
        };
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
        let bytes = axum::body::to_bytes(resp.into_body(), 1024 * 1024)
            .await
            .unwrap();
        let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        let temp_session_id = v["temp_session_id"].as_str().unwrap().to_string();
        let challenge = v["challenge"].as_str().unwrap().to_string();

        let kp = qp_rusty_crystals_dilithium::ml_dsa_87::Keypair::generate(None);
        let pk_hex = hex::encode(kp.public.to_bytes());
        let addr = quantus_cli::qp_dilithium_crypto::types::DilithiumPublic::try_from(
            kp.public.to_bytes().as_slice(),
        )
        .unwrap()
        .into_account()
        .to_ss58check();
        let msg = format!(
            "taskmaster:login:1|challenge={}|address={}",
            challenge, addr
        );
        let sig_hex = hex::encode(kp.sign(msg.as_bytes(), None, true));

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
        let bytes = axum::body::to_bytes(resp.into_body(), 1024 * 1024)
            .await
            .unwrap();
        let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        let access_token = v["access_token"].as_str().unwrap();

        let resp = app
            .clone()
            .oneshot(
                http::Request::builder()
                    .method("GET")
                    .uri("/auth/me")
                    .header(
                        http::header::AUTHORIZATION,
                        format!("Bearer {}", access_token),
                    )
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), http::StatusCode::OK);
    }
}
