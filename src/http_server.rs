use axum::http::Method;
use axum::{middleware, response::Json, routing::get, Router};
use rusx::TwitterGateway;
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, sync::Arc};
use tower::ServiceBuilder;
use tower_http::{
    cors::{AllowHeaders, CorsLayer},
    trace::TraceLayer,
};

use crate::services::exchange_rate_service::ExchangeRateService;
use crate::{
    db_persistence::DbPersistence,
    metrics::{metrics_handler, track_metrics, Metrics},
    routes::api_routes,
    services::{risk_checker_service::RiskCheckerService, wallet_config_service::WalletConfigService},
    Config,
};
use chrono::{DateTime, Utc};
use tokio::sync::RwLock;

#[derive(Debug, Clone)]
pub struct AppState {
    pub db: Arc<DbPersistence>,
    pub metrics: Arc<Metrics>,
    pub wallet_config_service: Arc<WalletConfigService>,
    pub risk_checker_service: Arc<RiskCheckerService>,
    pub exchange_rate_service: Arc<ExchangeRateService>,
    pub config: Arc<Config>,
    pub challenges: Arc<RwLock<HashMap<String, Challenge>>>,
    pub twitter_gateway: Arc<dyn TwitterGateway>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Challenge {
    pub challenge: String,
    pub created_at: DateTime<Utc>,
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
        .route("/metrics", get(metrics_handler))
        .nest("/api", api_routes(state.clone()))
        .layer(middleware::from_fn(track_metrics))
        .layer(
            ServiceBuilder::new().layer(TraceLayer::new_for_http()).layer(
                CorsLayer::new()
                    .allow_origin(state.config.get_cors_allowed_origins())
                    .allow_methods([Method::GET, Method::POST, Method::PUT, Method::DELETE, Method::OPTIONS])
                    .allow_headers(AllowHeaders::mirror_request())
                    .allow_credentials(true),
            ),
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

/// Start the HTTP server
pub async fn start_server(
    db: Arc<DbPersistence>,
    twitter_gateway: Arc<dyn TwitterGateway>,
    bind_address: &str,
    config: Arc<Config>,
) -> Result<(), Box<dyn std::error::Error>> {
    let state = AppState {
        db,
        metrics: Arc::new(Metrics::new()),
        wallet_config_service: Arc::new(WalletConfigService::new(
            config.remote_configs.wallet_configs_file.clone(),
        )?),
        risk_checker_service: Arc::new(RiskCheckerService::new(&config.risk_checker)),
        exchange_rate_service: Arc::new(ExchangeRateService::new(&config.exchange_rate.api_key)),
        config,
        twitter_gateway,
        challenges: Arc::new(RwLock::new(HashMap::new())),
    };
    let app = create_router(state);

    tracing::info!("Starting HTTP server on {}", bind_address);

    let listener = tokio::net::TcpListener::bind(bind_address).await?;
    axum::serve(listener, app).await?;

    Ok(())
}
