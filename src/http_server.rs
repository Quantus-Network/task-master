use axum::{
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::Json,
    routing::{get, post},
    Router,
};
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, sync::Arc};
use tower::ServiceBuilder;
use tower_http::{cors::CorsLayer, trace::TraceLayer};

use crate::{
    db_persistence::DbPersistence,
    models::{
        address::{Address, AddressInput},
        task::{Task, TaskStatus},
    },
    routes::{api_routes, auth::auth_routes},
    services::{
        graphql_client::GraphqlClient,
    }, utils::generate_referral_code::generate_referral_code,
};
use tokio::sync::RwLock;
use chrono::{DateTime, Utc};

#[derive(Debug, thiserror::Error)]
pub enum HttpServerError {
    #[error("Database error: {0}")]
    Database(#[from] crate::db_persistence::DbError),
    #[error("Task not found: {0}")]
    TaskNotFound(String),
    #[error("Invalid task URL format: {0}")]
    InvalidTaskUrl(String),
    #[error("Server error: {0}")]
    Server(String),
}

pub type HttpServerResult<T> = Result<T, HttpServerError>;

#[derive(Debug, Clone)]
pub struct AppState {
    pub db: Arc<DbPersistence>,
    pub sessions: Arc<RwLock<HashMap<String, Session>>>,
    pub challenges: Arc<RwLock<HashMap<String, Challenge>>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Challenge {
    pub id: String,
    pub challenge: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    pub key: String,
    pub address: String,
    pub expires_at: DateTime<Utc>,
}

#[derive(Debug, Deserialize)]
pub struct CompleteTaskRequest {
    pub task_url: String,
}

#[derive(Debug, Deserialize)]
pub struct AssociateEthAddressRequest {
    pub quan_address: String,
    pub eth_address: String,
    pub signature: String,
    pub public_key: String,
}

#[derive(Debug, Serialize)]
pub struct CompleteTaskResponse {
    pub success: bool,
    pub message: String,
    pub task_id: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct AssociateEthAddressResponse {
    pub success: bool,
    pub message: String,
}

#[derive(Debug, Serialize)]
pub struct StatusResponse {
    pub status: String,
    pub total_tasks: usize,
    pub pending_tasks: usize,
    pub completed_tasks: usize,
    pub reversed_tasks: usize,
    pub failed_tasks: usize,
}

#[derive(Debug, Serialize)]
pub struct HealthResponse {
    pub healthy: bool,
    pub service: String,
    pub version: String,
    pub timestamp: String,
}

/// Create the HTTP server router
pub fn create_router(state: AppState) -> Router {
    Router::new()
        .route("/health", get(health_check))
        .route("/status", get(get_status))
        .route("/complete", post(complete_task))
        .route("/associate-eth", post(associate_eth_address))
        .route("/referrals/add", post(add_referrer))
        .route("/sync-transfers", post(sync_transfers))
        .route("/tasks", get(list_all_tasks))
        .route("/tasks/:task_id", get(get_task))
        .nest("/api", api_routes())
        .nest("/auth", auth_routes())
        .layer(
            ServiceBuilder::new()
                .layer(TraceLayer::new_for_http())
                .layer(CorsLayer::permissive()),
        )
        .with_state(state)
}

/// Health check endpoint
async fn health_check() -> Json<HealthResponse> {
    Json(HealthResponse {
        healthy: true,
        service: "TaskMaster".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        timestamp: chrono::Utc::now().to_rfc3339(),
    })
}

pub fn auth_from_headers(headers: &HeaderMap) -> Option<String> {
    let v = headers.get("authorization")?.to_str().ok()?.trim();
    let prefix = "Session ";
    if v.starts_with(prefix) { Some(v[prefix.len()..].to_string()) } else { None }
}

pub struct AuthSession {
    pub address: String,
    pub expires_at: DateTime<Utc>,
}

#[axum::async_trait]
impl axum::extract::FromRequestParts<AppState> for AuthSession {
    type Rejection = StatusCode;

    async fn from_request_parts(
        parts: &mut axum::http::request::Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let Some(token) = auth_from_headers(&parts.headers) else { return Err(StatusCode::UNAUTHORIZED) };
        let mut sessions = state.sessions.write().await;
        let Some(s) = sessions.get_mut(&token) else { return Err(StatusCode::UNAUTHORIZED) };
        if s.expires_at < Utc::now() { sessions.remove(&token); return Err(StatusCode::UNAUTHORIZED) };
        s.expires_at = Utc::now() + chrono::Duration::hours(24);
        Ok(AuthSession { address: s.address.clone(), expires_at: s.expires_at })
    }
}

#[derive(Debug, Deserialize)]
struct AddReferrerBody { referred: String, referrer: String }

#[derive(Debug, Serialize)]
struct AddReferrerResponse { success: bool }

async fn add_referrer(
    State(state): State<AppState>,
    AuthSession { address: _auth, expires_at: _ }: AuthSession,
    Json(body): Json<AddReferrerBody>,
) -> Result<Json<AddReferrerResponse>, StatusCode> {
    let referred = body.referred;
    let referrer = body.referrer;
    let addresses = state.db.addresses.find_all().await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let have_referred = addresses.iter().any(|a| a.quan_address.0 == referred);
    let have_referrer = addresses.iter().any(|a| a.quan_address.0 == referrer);
    if !have_referred {
        let referral_code = generate_referral_code(referred.clone()).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        let addr = Address::new(AddressInput { quan_address: referred.clone(), eth_address: None, referral_code }).map_err(|_| StatusCode::BAD_REQUEST)?;
        state.db.addresses.create(&addr).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    }
    if !have_referrer {
        let referral_code = generate_referral_code(referrer.clone()).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        let addr = Address::new(AddressInput { quan_address: referrer.clone(), eth_address: None, referral_code }).map_err(|_| StatusCode::BAD_REQUEST)?;
        state.db.addresses.create(&addr).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    }
    use crate::models::referrals::{Referral, ReferralData};
    let referral = Referral::new(ReferralData { referrer_address: referrer, referee_address: referred }).map_err(|_| StatusCode::BAD_REQUEST)?;
    state.db.referrals.create(&referral).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(AddReferrerResponse { success: true }))
}

/// Get service status and task counts
async fn get_status(State(state): State<AppState>) -> Result<Json<StatusResponse>, StatusCode> {
    let status_counts = state
        .db
        .tasks
        .status_counts()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let total_tasks = state
        .db
        .tasks
        .task_count()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let response = StatusResponse {
        status: "running".to_string(),
        total_tasks: total_tasks as usize,
        pending_tasks: status_counts
            .get(&TaskStatus::Pending)
            .copied()
            .unwrap_or(0),
        completed_tasks: status_counts
            .get(&TaskStatus::Completed)
            .copied()
            .unwrap_or(0),
        reversed_tasks: status_counts
            .get(&TaskStatus::Reversed)
            .copied()
            .unwrap_or(0),
        failed_tasks: status_counts.get(&TaskStatus::Failed).copied().unwrap_or(0),
    };

    Ok(Json(response))
}

/// Complete a task by marking it as completed
async fn complete_task(
    State(state): State<AppState>,
    Json(payload): Json<CompleteTaskRequest>,
) -> Result<Json<CompleteTaskResponse>, (StatusCode, Json<CompleteTaskResponse>)> {
    tracing::info!(
        "Received task completion request for URL: {}",
        payload.task_url
    );

    // Validate task URL format (12 digits)
    if payload.task_url.len() != 12 || !payload.task_url.chars().all(|c| c.is_ascii_digit()) {
        let response = CompleteTaskResponse {
            success: false,
            message: format!("Invalid task URL format: {}", payload.task_url),
            task_id: None,
        };
        return Err((StatusCode::BAD_REQUEST, Json(response)));
    }

    // Find task by URL
    let task = match state.db.tasks.find_task_by_url(&payload.task_url).await {
        Ok(Some(task)) => task,
        Ok(None) => {
            let response = CompleteTaskResponse {
                success: false,
                message: format!("Task not found with URL: {}", payload.task_url),
                task_id: None,
            };
            return Err((StatusCode::NOT_FOUND, Json(response)));
        }
        Err(_) => {
            let response = CompleteTaskResponse {
                success: false,
                message: "Database error".to_string(),
                task_id: None,
            };
            return Err((StatusCode::INTERNAL_SERVER_ERROR, Json(response)));
        }
    };

    // Check if task is in a valid state for completion
    match task.status {
        TaskStatus::Pending => {
            // Task can be completed
        }
        TaskStatus::Completed => {
            let response = CompleteTaskResponse {
                success: false,
                message: "Task is already completed".to_string(),
                task_id: Some(task.task_id.clone()),
            };
            return Err((StatusCode::CONFLICT, Json(response)));
        }
        TaskStatus::Reversed => {
            let response = CompleteTaskResponse {
                success: false,
                message: "Task has already been reversed".to_string(),
                task_id: Some(task.task_id.clone()),
            };
            return Err((StatusCode::CONFLICT, Json(response)));
        }
        TaskStatus::Failed => {
            let response = CompleteTaskResponse {
                success: false,
                message: "Task has failed and cannot be completed".to_string(),
                task_id: Some(task.task_id.clone()),
            };
            return Err((StatusCode::CONFLICT, Json(response)));
        }
    }

    // Mark task as completed
    match state
        .db
        .tasks
        .update_task_status(&task.task_id, TaskStatus::Completed)
        .await
    {
        Ok(()) => {
            tracing::info!("Task {} marked as completed", task.task_id);
            let response = CompleteTaskResponse {
                success: true,
                message: "Task completed successfully".to_string(),
                task_id: Some(task.task_id.clone()),
            };
            Ok(Json(response))
        }
        Err(e) => {
            tracing::error!("Failed to update task {}: {}", task.task_id, e);
            let response = CompleteTaskResponse {
                success: false,
                message: format!("Failed to update task: {}", e),
                task_id: Some(task.task_id.clone()),
            };
            Err((StatusCode::INTERNAL_SERVER_ERROR, Json(response)))
        }
    }
}

/// Associate an Ethereum address with a Quantus address using signature verification
async fn associate_eth_address(
    State(state): State<AppState>,
    Json(payload): Json<AssociateEthAddressRequest>,
) -> Result<Json<AssociateEthAddressResponse>, (StatusCode, Json<AssociateEthAddressResponse>)> {
    tracing::info!(
        "Received ETH address association request for quan_address: {} -> eth_address: {} (pubkey: {})",
        payload.quan_address,
        payload.eth_address,
        payload.public_key
    );

    // Verify the signature
    match crate::services::ethereum_service::verify_dilithium_signature(
        &payload.quan_address,
        &payload.eth_address,
        &payload.signature,
        &payload.public_key,
    ) {
        Ok(true) => {
            tracing::info!("Signature verification successful");
        }
        Ok(false) => {
            let response = AssociateEthAddressResponse {
                success: false,
                message: "Signature verification failed".to_string(),
            };
            return Err((StatusCode::UNAUTHORIZED, Json(response)));
        }
        Err(crate::services::ethereum_service::SignatureError::VerificationFailed) => {
            tracing::warn!("Dilithium signature verification failed");
            let response = AssociateEthAddressResponse {
                success: false,
                message: "Dilithium signature verification failed".to_string(),
            };
            return Err((StatusCode::UNAUTHORIZED, Json(response)));
        }
        Err(e) => {
            tracing::error!("Signature verification error: {}", e);
            let response = AssociateEthAddressResponse {
                success: false,
                message: format!("Signature verification failed: {}", e),
            };
            return Err((StatusCode::BAD_REQUEST, Json(response)));
        }
    }

    // Check if the quan_address exists in the database
    let addresses = match state.db.addresses.find_all().await {
        Ok(addrs) => addrs,
        Err(_) => {
            let response = AssociateEthAddressResponse {
                success: false,
                message: "Database error while checking addresses".to_string(),
            };
            return Err((StatusCode::INTERNAL_SERVER_ERROR, Json(response)));
        }
    };

    let quan_address_exists = addresses
        .iter()
        .any(|addr| addr.quan_address.0 == payload.quan_address);

    if !quan_address_exists {
        if let Ok(referral_code) = generate_referral_code(payload.quan_address.clone()).await {

            let new_address_input = AddressInput {
                quan_address: payload.quan_address.clone(),
                eth_address: Some(payload.eth_address.clone()),
                referral_code,
            };

            if let Ok(new_address) = Address::new(new_address_input) {
                // Add the quan_address to the database if it doesn't exist
                if let Err(_) = state.db.addresses.create(&new_address).await {
                    let response = AssociateEthAddressResponse {
                        success: false,
                        message: "Failed to add address to database".to_string(),
                    };
                    return Err((StatusCode::INTERNAL_SERVER_ERROR, Json(response)));
                }
                tracing::info!(
                    "Added new quan_address {} with eth_address {}",
                    payload.quan_address,
                    payload.eth_address
                );
            } else {
                let response = AssociateEthAddressResponse {
                    success: false,
                    message: "Failed to update address in database".to_string(),
                };

                return Err((StatusCode::BAD_REQUEST, Json(response)));
            };
        } else {
            let response = AssociateEthAddressResponse {
                success: false,
                message: "Failed to generate referral code for new address".to_string(),
            };

            return Err((StatusCode::INTERNAL_SERVER_ERROR, Json(response)));
        }
    } else {
        // Update existing address with eth_address
        match state
            .db
            .addresses
            .update_address_eth(&payload.quan_address, &payload.eth_address)
            .await
        {
            Ok(_) => {
                tracing::info!(
                    "Updated quan_address {} with eth_address {}",
                    payload.quan_address,
                    payload.eth_address
                );
            }
            Err(_) => {
                let response = AssociateEthAddressResponse {
                    success: false,
                    message: "Failed to update address in database".to_string(),
                };
                return Err((StatusCode::INTERNAL_SERVER_ERROR, Json(response)));
            }
        }
    }

    let response = AssociateEthAddressResponse {
        success: true,
        message: "Ethereum address associated successfully".to_string(),
    };

    Ok(Json(response))
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SyncTransfersResponse {
    pub success: bool,
    pub message: String,
    pub transfers_processed: Option<usize>,
    pub addresses_stored: Option<usize>,
}

/// Sync transfers from GraphQL endpoint and store addresses
async fn sync_transfers(
    State(state): State<AppState>,
) -> Result<Json<SyncTransfersResponse>, (StatusCode, Json<SyncTransfersResponse>)> {
    tracing::info!("Received request to sync transfers from GraphQL endpoint");

    let graphql_client = GraphqlClient::new((*state.db).clone());

    match graphql_client.sync_transfers_and_addresses().await {
        Ok((transfer_count, address_count)) => {
            tracing::info!(
                "Transfer sync completed successfully: {} transfers, {} addresses",
                transfer_count,
                address_count
            );

            let response = SyncTransfersResponse {
                success: true,
                message: format!(
                    "Successfully processed {} transfers and stored {} addresses",
                    transfer_count, address_count
                ),
                transfers_processed: Some(transfer_count),
                addresses_stored: Some(address_count),
            };

            Ok(Json(response))
        }
        Err(e) => {
            tracing::error!("Failed to sync transfers: {}", e);

            let response = SyncTransfersResponse {
                success: false,
                message: format!("Failed to sync transfers: {}", e),
                transfers_processed: None,
                addresses_stored: None,
            };

            Err((StatusCode::INTERNAL_SERVER_ERROR, Json(response)))
        }
    }
}

/// List all tasks (for debugging/monitoring)
async fn list_all_tasks(State(state): State<AppState>) -> Result<Json<Vec<Task>>, StatusCode> {
    let tasks = state
        .db
        .tasks
        .get_all_tasks()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(tasks))
}

/// Get a specific task by ID
async fn get_task(
    State(state): State<AppState>,
    Path(task_id): Path<String>,
) -> Result<Json<Task>, StatusCode> {
    match state.db.tasks.get_task(&task_id).await {
        Ok(Some(task)) => Ok(Json(task)),
        Ok(None) => Err(StatusCode::NOT_FOUND),
        Err(_) => Err(StatusCode::INTERNAL_SERVER_ERROR),
    }
}

/// Start the HTTP server
pub async fn start_server(
    db: Arc<DbPersistence>,
    bind_address: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let state = AppState {
        db,
        sessions: Arc::new(RwLock::new(HashMap::new())),
        challenges: Arc::new(RwLock::new(HashMap::new())),
    };
    let app = create_router(state);

    tracing::info!("Starting HTTP server on {}", bind_address);

    let listener = tokio::net::TcpListener::bind(bind_address).await?;
    axum::serve(listener, app).await?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::{self, StatusCode};

    async fn test_app() -> axum::Router {
        let db = Arc::new(DbPersistence::new_unmigrated("postgres://postgres:postgres@127.0.0.1:55432/task_master").await.unwrap());
        let state = AppState {
            db,
            sessions: Arc::new(RwLock::new(HashMap::new())),
            challenges: Arc::new(RwLock::new(HashMap::new())),
        };
        create_router(state)
    }
}