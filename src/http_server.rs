use axum::{extract::State, http::StatusCode, response::Json, routing::get, Router};
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, sync::Arc};
use tower::ServiceBuilder;
use tower_http::{cors::CorsLayer, trace::TraceLayer};

use crate::{
    db_persistence::DbPersistence, models::task::TaskStatus, routes::api_routes, Config,
    GraphqlClient,
};
use chrono::{DateTime, Utc};
use tokio::sync::RwLock;

#[derive(Debug, Clone)]
pub struct AppState {
    pub db: Arc<DbPersistence>,
    pub graphql_client: Arc<GraphqlClient>,
    pub config: Arc<Config>,
    pub challenges: Arc<RwLock<HashMap<String, Challenge>>>,
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
        .nest("/api", api_routes(state.clone()))
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

/// Start the HTTP server
pub async fn start_server(
    db: Arc<DbPersistence>,
    graphql_client: Arc<GraphqlClient>,
    bind_address: &str,
    config: Arc<Config>,
) -> Result<(), Box<dyn std::error::Error>> {
    let state = AppState {
        db,
        graphql_client,
        config,
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
            challenges: Arc::new(RwLock::new(HashMap::new())),
        };
        create_router(state)
    }
}
