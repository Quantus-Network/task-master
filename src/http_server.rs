use axum::{extract::State, http::StatusCode, middleware, response::Json, routing::get, Router};
use rusx::{PkceCodeVerifier, TwitterGateway};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};
use tower::ServiceBuilder;
use tower_cookies::CookieManagerLayer;
use tower_http::{cors::CorsLayer, trace::TraceLayer};

use crate::{
    db_persistence::DbPersistence,
    metrics::{metrics_handler, track_metrics, Metrics},
    models::task::TaskStatus,
    routes::api_routes,
    Config, GraphqlClient,
};
use chrono::{DateTime, Utc};
use tokio::sync::RwLock;

#[derive(Debug, Clone)]
pub struct AppState {
    pub db: Arc<DbPersistence>,
    pub metrics: Arc<Metrics>,
    pub graphql_client: Arc<GraphqlClient>,
    pub config: Arc<Config>,
    pub challenges: Arc<RwLock<HashMap<String, Challenge>>>,
    pub oauth_sessions: Arc<Mutex<HashMap<String, PkceCodeVerifier>>>,
    pub twitter_oauth_tokens: Arc<RwLock<HashMap<String, String>>>,
    pub twitter_gateway: Arc<dyn TwitterGateway>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Challenge {
    pub id: String,
    pub challenge: String,
    pub created_at: DateTime<Utc>,
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
        .route("/metrics", get(metrics_handler))
        .nest("/api", api_routes(state.clone()))
        .layer(middleware::from_fn(track_metrics))
        .layer(
            ServiceBuilder::new()
                .layer(TraceLayer::new_for_http())
                .layer(CorsLayer::permissive()),
        )
        .layer(CookieManagerLayer::new()) // Enable Cookie support
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
        pending_tasks: status_counts.get(&TaskStatus::Pending).copied().unwrap_or(0),
        completed_tasks: status_counts.get(&TaskStatus::Completed).copied().unwrap_or(0),
        reversed_tasks: status_counts.get(&TaskStatus::Reversed).copied().unwrap_or(0),
        failed_tasks: status_counts.get(&TaskStatus::Failed).copied().unwrap_or(0),
    };

    Ok(Json(response))
}

/// Start the HTTP server
pub async fn start_server(
    db: Arc<DbPersistence>,
    graphql_client: Arc<GraphqlClient>,
    twitter_gateway: Arc<dyn TwitterGateway>,
    bind_address: &str,
    config: Arc<Config>,
) -> Result<(), Box<dyn std::error::Error>> {
    let state = AppState {
        db,
        metrics: Arc::new(Metrics::new()),
        graphql_client,
        config,
        twitter_gateway,
        challenges: Arc::new(RwLock::new(HashMap::new())),
        oauth_sessions: Arc::new(Mutex::new(HashMap::new())),
        twitter_oauth_tokens: Arc::new(RwLock::new(HashMap::new())),
    };
    let app = create_router(state);

    tracing::info!("Starting HTTP server on {}", bind_address);

    let listener = tokio::net::TcpListener::bind(bind_address).await?;
    axum::serve(listener, app).await?;

    Ok(())
}
