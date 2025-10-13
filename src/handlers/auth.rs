use axum::{extract::State, http::StatusCode, response::Json};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{http_server::{AppState, Challenge, Session}, services::signature_service::SignatureService};
use tracing::{info, warn};

#[derive(Debug, Deserialize)]
pub struct RequestChallengeBody { pub address: Option<String> }

#[derive(Debug, Serialize)]
pub struct RequestChallengeResponse { pub temp_session_id: String, pub challenge: String }

pub async fn request_challenge(
    State(state): State<AppState>,
    Json(_body): Json<RequestChallengeBody>,
) -> Result<Json<RequestChallengeResponse>, StatusCode> {
    let temp_session_id = Uuid::new_v4().to_string();
    let challenge = Uuid::new_v4().to_string();
    let entry = Challenge { id: temp_session_id.clone(), challenge: challenge.clone(), created_at: Utc::now() };
    state.challenges.write().await.insert(temp_session_id.clone(), entry);
    Ok(Json(RequestChallengeResponse { temp_session_id, challenge }))
}

#[derive(Debug, Deserialize)]
pub struct VerifyLoginBody {
    pub temp_session_id: String,
    pub address: String,
    pub public_key: String,
    pub signature: String,
}

#[derive(Debug, Serialize)]
pub struct VerifyLoginResponse { pub session_key: String }

pub async fn verify_login(
    State(state): State<AppState>,
    Json(body): Json<VerifyLoginBody>,
) -> Result<Json<VerifyLoginResponse>, StatusCode> {
    let sig_len = body.signature.strip_prefix("0x").unwrap_or(&body.signature).len();
    let pk_len = body.public_key.strip_prefix("0x").unwrap_or(&body.public_key).len();
    info!(
        temp_session_id = %body.temp_session_id,
        address = %body.address,
        signature_len = sig_len,
        public_key_len = pk_len,
        "verify_login: received payload"
    );
    let Some(chal) = state.challenges.read().await.get(&body.temp_session_id).cloned() else {
        return Err(StatusCode::UNAUTHORIZED);
    };
    let message = format!("taskmaster:login:1|challenge={}|address={}", chal.challenge, body.address);
    info!(message = %message, message_len = message.len(), message_hex = %hex::encode(message.as_bytes()), "verify_login: constructed message");

    let addr_res = SignatureService::verify_address(&body.public_key, &body.address);
    if let Err(e) = &addr_res { warn!(error = %e, "verify_login: verify_address error"); }
    let addr_ok = addr_res.map_err(|_| StatusCode::UNAUTHORIZED)?;
    if !addr_ok { return Err(StatusCode::UNAUTHORIZED); }
    let sig_res = SignatureService::verify_message(message.as_bytes(), &body.signature, &body.public_key);
    if let Err(e) = &sig_res { warn!(error = %e, "verify_login: verify_message error"); }
    let sig_ok = sig_res.map_err(|_| StatusCode::UNAUTHORIZED)?;
    info!(addr_ok = addr_ok, sig_ok = sig_ok, "verify_login: verification results");
    if !sig_ok { return Err(StatusCode::UNAUTHORIZED); }
    let session_key = Uuid::new_v4().to_string();
    let expires_at = Utc::now() + chrono::Duration::hours(24);
    let session = Session { key: session_key.clone(), address: body.address.clone(), expires_at };
    state.sessions.write().await.insert(session_key.clone(), session);
    state.challenges.write().await.remove(&body.temp_session_id);
    Ok(Json(VerifyLoginResponse { session_key }))
}

#[derive(Debug, Serialize)]
pub struct AuthMeResponse { pub address: String, pub expires_at: String }

pub async fn auth_me(
    State(_state): State<AppState>,
    crate::http_server::AuthSession { address, expires_at }: crate::http_server::AuthSession,
) -> Result<Json<AuthMeResponse>, StatusCode> {
    Ok(Json(AuthMeResponse { address, expires_at: expires_at.to_rfc3339() }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{http, body::Body};
    use tower::ServiceExt;
    use crate::{db_persistence::DbPersistence, routes::auth::auth_routes};
    use std::sync::Arc;
    use sp_runtime::traits::IdentifyAccount;
    use sp_core::crypto::Ss58Codec;

    async fn test_app() -> axum::Router {
        let db = Arc::new(DbPersistence::new_unmigrated("postgres://postgres:postgres@127.0.0.1:55432/task_master").await.unwrap());
        let state = AppState {
            db,
            sessions: Arc::new(tokio::sync::RwLock::new(std::collections::HashMap::new())),
            challenges: Arc::new(tokio::sync::RwLock::new(std::collections::HashMap::new())),
        };
        auth_routes().with_state(state)
    }

    #[tokio::test]
    async fn auth_challenge_and_verify_flow() {
        let app = test_app().await;

        let resp = app.clone().oneshot(
            http::Request::builder()
                .method("POST")
                .uri("/auth/request-challenge")
                .header(http::header::CONTENT_TYPE, "application/json")
                .body(Body::from("{}"))
                .unwrap(),
        ).await.unwrap();
        assert_eq!(resp.status(), http::StatusCode::OK);
        let bytes = axum::body::to_bytes(resp.into_body(), 1024 * 1024).await.unwrap();
        let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        let temp_session_id = v["temp_session_id"].as_str().unwrap().to_string();
        let challenge = v["challenge"].as_str().unwrap().to_string();

        let kp = qp_rusty_crystals_dilithium::ml_dsa_87::Keypair::generate(None);
        let pk_hex = hex::encode(kp.public.to_bytes());
        let addr = quantus_cli::qp_dilithium_crypto::types::DilithiumPublic::try_from(kp.public.to_bytes().as_slice()).unwrap().into_account().to_ss58check();
        let msg = format!("taskmaster:login:1|challenge={}|address={}", challenge, addr);
        let sig_hex = hex::encode(kp.sign(msg.as_bytes(), None, true));

        let verify_payload = serde_json::json!({
            "temp_session_id": temp_session_id,
            "address": addr,
            "public_key": pk_hex,
            "signature": sig_hex,
        });
        let resp = app.clone().oneshot(
            http::Request::builder()
                .method("POST")
                .uri("/auth/verify")
                .header(http::header::CONTENT_TYPE, "application/json")
                .body(Body::from(serde_json::to_vec(&verify_payload).unwrap()))
                .unwrap(),
        ).await.unwrap();
        assert_eq!(resp.status(), http::StatusCode::OK);
        let bytes = axum::body::to_bytes(resp.into_body(), 1024 * 1024).await.unwrap();
        let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        let session_key = v["session_key"].as_str().unwrap();

        let resp = app.clone().oneshot(
            http::Request::builder()
                .method("GET")
                .uri("/auth/me")
                .header(http::header::AUTHORIZATION, format!("Session {}", session_key))
                .body(Body::empty())
                .unwrap(),
        ).await.unwrap();
        assert_eq!(resp.status(), http::StatusCode::OK);
    }
}

