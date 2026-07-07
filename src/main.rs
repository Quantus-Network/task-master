use crate::{
    args::Args,
    db_persistence::DbPersistence,
    errors::{AppError, AppResult},
    services::graphql_client::GraphqlClient,
};

use clap::Parser;
use rusx::RusxGateway;
use sp_core::crypto::{self, Ss58AddressFormat};
use std::sync::Arc;

use tracing::{error, info};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

mod args;
mod config;
mod db_persistence;
mod errors;
mod handlers;
mod http_server;
mod metrics;
mod middlewares;
mod models;
mod repositories;
mod routes;
mod services;
mod utils;

use config::Config;

#[tokio::main]
async fn main() -> AppResult<()> {
    let args = Args::parse();

    // Load configuration from --config path (defaults to config/default.toml)
    let config = Config::load(&args.config).map_err(AppError::Config)?;

    crypto::set_default_ss58_version(Ss58AddressFormat::custom(189));
    // Initialize logging
    init_logging(&config.logging.level)?;

    info!("🚀 Starting TaskMaster v{}", env!("CARGO_PKG_VERSION"));
    info!("Configuration loaded from: {}", args.config);

    // Initialize database persistence
    let db_url = config.get_database_url();
    info!("Database URL: {}", db_url);
    let db = Arc::new(DbPersistence::new(db_url).await?);

    // Initialize graphql client
    let graphql_client = GraphqlClient::new((*db).clone(), config.candidates.graphql_url.clone());

    if args.sync_transfers {
        info!("Running in sync-transfers mode");
        let (transfer_count, address_count) = graphql_client.sync_transfers_and_addresses().await?;
        info!(
            "Sync completed successfully: {} transfers processed, {} addresses stored",
            transfer_count, address_count
        );
        return Ok(());
    }

    // Start HTTP server
    let server_address = config.get_server_address();
    info!("Starting HTTP server on {}", server_address);

    let twitter_gateway = Arc::new(RusxGateway::new(config.x_oauth.clone(), None)?);
    let server_db = db.clone();
    let server_addr_clone = server_address.clone();
    let server_config = Arc::new(config.clone());
    let server_twitter_gateway = twitter_gateway.clone();
    let server_task = tokio::spawn(async move {
        http_server::start_server(server_db, server_twitter_gateway, &server_addr_clone, server_config)
            .await
            .map_err(|e| AppError::Server(e.to_string()))
    });

    info!("🎯 TaskMaster is now running!");
    info!("HTTP API available at: http://{}", server_address);

    // Wait for any task to complete (they should run forever unless there's an error)
    tokio::select! {
        result = server_task => {
            error!("HTTP server exited: {:?}", result);
            result??;
        }
    }

    Ok(())
}

fn init_logging(level: &str) -> AppResult<()> {
    let log_level = match level.to_lowercase().as_str() {
        "error" => tracing::Level::ERROR,
        "warn" => tracing::Level::WARN,
        "info" => tracing::Level::INFO,
        "debug" => tracing::Level::DEBUG,
        "trace" => tracing::Level::TRACE,
        _ => {
            eprintln!("Invalid log level: {}, defaulting to info", level);
            tracing::Level::INFO
        }
    };

    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| format!("task_master={}", log_level).into()),
        )
        .with(tracing_subscriber::fmt::layer().with_target(false))
        .init();

    Ok(())
}
