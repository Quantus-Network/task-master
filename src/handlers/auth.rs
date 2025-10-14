use axum::{extract::State, http::StatusCode, response::Json, Extension};
use chrono::Utc;
use jsonwebtoken::{encode, EncodingKey, Header};
use uuid::Uuid;

use crate::{
    handlers::SuccessResponse,
    http_server::{AppState, Challenge},
    models::{
        address::Address,
        auth::{
            RequestChallengeBody, RequestChallengeResponse, TokenClaims, VerifyLoginBody,
            VerifyLoginResponse,
        },
    },
    services::signature_service::SignatureService,
};
use tracing::{info, warn};

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
) -> Result<Json<VerifyLoginResponse>, StatusCode> {
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
    info!(
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
        return Err(StatusCode::UNAUTHORIZED);
    };
    let message = format!(
        "taskmaster:login:1|challenge={}|address={}",
        chal.challenge, body.address
    );
    info!(message = %message, message_len = message.len(), message_hex = %hex::encode(message.as_bytes()), "verify_login: constructed message");

    let addr_res = SignatureService::verify_address(&body.public_key, &body.address);
    if let Err(e) = &addr_res {
        warn!(error = %e, "verify_login: verify_address error");
    }
    let addr_ok = addr_res.map_err(|_| StatusCode::UNAUTHORIZED)?;
    if !addr_ok {
        return Err(StatusCode::UNAUTHORIZED);
    }
    let sig_res =
        SignatureService::verify_message(message.as_bytes(), &body.signature, &body.public_key);
    if let Err(e) = &sig_res {
        warn!(error = %e, "verify_login: verify_message error");
    }
    let sig_ok = sig_res.map_err(|_| StatusCode::UNAUTHORIZED)?;
    info!(
        addr_ok = addr_ok,
        sig_ok = sig_ok,
        "verify_login: verification results"
    );
    if !sig_ok {
        return Err(StatusCode::UNAUTHORIZED);
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
    use crate::{
        db_persistence::DbPersistence, models::address::AddressInput,
        repositories::address::AddressRepository, routes::auth::auth_routes, Config,
    };
    use axum::{body::Body, http};
    use sp_core::crypto::{self, Ss58AddressFormat,Ss58Codec};
    use sp_runtime::traits::IdentifyAccount;
    use std::sync::Arc;
    use tower::ServiceExt;

    async fn test_app() -> (AppState, axum::Router) {
        let config = Config::load().expect("Failed to load test configuration");
        let db = DbPersistence::new_unmigrated(config.get_database_url())
            .await
            .unwrap();

        let state = AppState {
            db: Arc::new(db),
            config: Arc::new(config),
            challenges: Arc::new(tokio::sync::RwLock::new(std::collections::HashMap::new())),
        };
        (state.clone(), auth_routes(state.clone()).with_state(state))
    }

    // Helper to create a persisted address for tests.
    async fn create_persisted_address(repo: &AddressRepository, address: String) -> Address {
        let input = AddressInput {
            quan_address: address.clone(),
            eth_address: None,
            referral_code: format!("REF{}", address),
        };
        let address = Address::new(input).unwrap();
        repo.create(&address).await.unwrap();
        address
    }

    #[tokio::test]
    async fn auth_challenge_and_verify_flow() {
        crypto::set_default_ss58_version(Ss58AddressFormat::custom(189));
        let (state, app) = test_app().await;

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
        create_persisted_address(&state.db.addresses, addr.clone()).await;

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
