use crate::{
    args::Args,
    db_persistence::DbPersistence,
    errors::{AppError, AppResult},
    services::{
        alert_service::AlertService, graphql_client::GraphqlClient, raid_leaderboard_service::RaidLeaderboardService,
        telegram_service::TelegramService, tweet_synchronizer_service::TweetSynchronizerService,
    },
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
    let mut config = Config::load(&args.config).map_err(AppError::Config)?;

    // Apply CLI overrides
    if let Some(wallet_name) = args.wallet_name {
        config.blockchain.wallet_name = wallet_name;
    }
    if let Some(wallet_password) = args.wallet_password {
        config.blockchain.wallet_password = wallet_password;
    }
    if let Some(node_url) = args.node_url {
        config.blockchain.node_url = node_url;
    }

    crypto::set_default_ss58_version(Ss58AddressFormat::custom(189));
    // Initialize logging
    init_logging(&config.logging.level)?;

    info!("🚀 Starting TaskMaster v{}", env!("CARGO_PKG_VERSION"));
    info!("Configuration loaded from: {}", args.config);
    info!("Node URL: {}", config.blockchain.node_url);
    info!("Wallet: {}", config.blockchain.wallet_name);

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

    let twitter_gateway = Arc::new(RusxGateway::new(
        config.x_oauth.clone(),
        Some(config.tweet_sync.api_key.clone()),
    )?);
    let telegram_service = Arc::new(TelegramService::new(config.tg_bot.clone()));
    let alert_service = Arc::new(AlertService::new(config.clone(), db.tweet_pull_usage.clone()));
    let server_db = db.clone();
    let graphql_client = Arc::new(graphql_client.clone());
    let server_addr_clone = server_address.clone();
    let server_config = Arc::new(config.clone());
    let server_twitter_gateway = twitter_gateway.clone();
    let server_task = tokio::spawn(async move {
        http_server::start_server(
            server_db,
            graphql_client,
            server_twitter_gateway,
            &server_addr_clone,
            server_config,
        )
        .await
        .map_err(|e| AppError::Server(e.to_string()))
    });

    info!("🎯 TaskMaster is now running!");
    info!("HTTP API available at: http://{}", server_address);

    info!(
        "Candidates refresh interval: {} minutes",
        config.candidates.refresh_interval_minutes
    );
    info!("Reversal period: {} hours", config.blockchain.reversal_period_hours);

    // Initialize tweet sync service
    let tweet_synchronizer = TweetSynchronizerService::new(
        db.clone(),
        twitter_gateway.clone(),
        telegram_service,
        alert_service.clone(),
        Arc::new(config.clone()),
    );

    // Initialize raid leaderboard  service
    let raid_leaderboard_service =
        RaidLeaderboardService::new(db.clone(), twitter_gateway, alert_service, Arc::new(config.clone()));

    // Wait for any task to complete (they should run forever unless there's an error)
    tokio::select! {
        result = server_task => {
            error!("HTTP server exited: {:?}", result);
            result??;
        }
        result = tweet_synchronizer.spawn_tweet_synchronizer() => {
            error!("Tweet synchronizer exited: {:?}", result);
            result??;
        }
        result = raid_leaderboard_service.spawn_raid_leaderboard_synchronizer() => {
            error!("Raid leaderboard synchronizer exited: {:?}", result);
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
