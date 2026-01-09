use crate::{
    args::Args,
    db_persistence::DbPersistence,
    errors::{AppError, AppResult},
    models::task::{Task, TaskInput},
    services::{
        alert_service::AlertService, graphql_client::GraphqlClient, raid_leaderboard_service::RaidLeaderboardService,
        reverser::start_reverser_service, task_generator::TaskGenerator, telegram_service::TelegramService,
        transaction_manager::TransactionManager, tweet_synchronizer_service::TweetSynchronizerService,
    },
};

use clap::Parser;
use rusx::RusxGateway;
use sp_core::crypto::{self, Ss58AddressFormat};
use std::sync::Arc;
use tokio::time::Duration;
use tracing::{error, info, warn};
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

    let initial_task_count = db.tasks.task_count().await?;
    info!("Loaded {} existing tasks from database", initial_task_count);

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

    if args.test_selection {
        info!("Running in test-selection mode");
        let mut task_generator = TaskGenerator::new(db.clone());

        // Load candidates from database
        if let Err(e) = task_generator.refresh_candidates_from_db().await {
            error!("Failed to refresh candidates from database: {}", e);
            return Err(AppError::TaskGenerator(e));
        }

        info!("Loaded {} candidates from database", task_generator.candidates_count());

        // Test generating tasks
        let test_count = 5; // Generate 5 test tasks
        match task_generator.generate_tasks(test_count).await {
            Ok(tasks) => {
                info!("Successfully generated {} test tasks:", tasks.len());
                for task in &tasks {
                    info!(
                        "  Task {}: {} -> {} QUAN (URL: {})",
                        task.task_id, task.quan_address.0, task.quan_amount.0, task.task_url
                    );
                }

                // Optionally save the tasks to database
                info!("Saving test tasks to database...");
                if let Err(e) = task_generator.save_tasks(tasks).await {
                    error!("Failed to save test tasks: {}", e);
                    return Err(AppError::TaskGenerator(e));
                }
                info!("Test tasks saved successfully!");
            }
            Err(e) => {
                error!("Failed to generate test tasks: {}", e);
                return Err(AppError::TaskGenerator(e));
            }
        }

        return Ok(());
    }

    if args.test_transaction {
        info!("Running in test-transaction mode");
        // Initialize transaction manager for testing
        info!("Connecting to Quantus node...");
        let transaction_manager = Arc::new(
            TransactionManager::new(
                &config.blockchain.node_url,
                &config.blockchain.wallet_name,
                &config.blockchain.wallet_password,
                db.clone(),
                config.get_reversal_period_duration(),
            )
            .await?,
        );

        // Perform health check
        if let Err(e) = transaction_manager.health_check().await {
            error!("Node health check failed: {}", e);
            return Err(AppError::Transaction(e));
        }

        let node_info = transaction_manager.get_node_info().await?;
        info!("✅ Connected to: {}", node_info);
        info!("Wallet address: {}", transaction_manager.get_wallet_address());

        // Check wallet balance
        match transaction_manager.get_wallet_balance().await {
            Ok(balance) => info!("Wallet balance: {} units", balance),
            Err(e) => warn!("Could not check wallet balance: {}", e),
        }

        // Create or get test task
        let (task_id, destination_address, amount) = if let (Some(dest), Some(amt)) = (&args.destination, args.amount) {
            // Create a temporary task for testing with custom parameters
            let task_input = TaskInput {
                quan_address: dest.clone(),
                quan_amount: amt,
                task_url: format!("test-{}", rand::random::<u32>()),
            };

            let test_task = Task::new(task_input)?;

            info!(
                "Creating temporary test task: {} -> {} (amount: {})",
                test_task.task_id, dest, amt
            );

            // Add the task to database
            db.tasks.create(&test_task).await?;

            (test_task.task_id, dest.clone(), amt)
        } else {
            // Use existing task from database
            let tasks = db.tasks.get_all_tasks().await?;
            if tasks.is_empty() {
                error!("No tasks found in database. Run --test-selection first to create some tasks, or provide --destination and --amount arguments.");
                return Err(AppError::Server("No tasks available for testing".to_string()));
            }

            let test_task = &tasks[0];
            (
                test_task.task_id.clone(),
                test_task.quan_address.0.clone(),
                test_task.quan_amount.0 as u64,
            )
        };

        info!(
            "Testing transaction with task: {} -> {} (amount: {})",
            task_id, destination_address, amount
        );

        // Send a reversible transaction
        match transaction_manager.send_reversible_transaction(&task_id).await {
            Ok(tx_hash) => {
                info!("✅ Reversible transaction sent successfully!");
                info!("Transaction hash: {}", tx_hash);
                info!("Task ID: {}", task_id);
                info!("Recipient: {}", destination_address);
                info!("Amount: {} QUAN", amount);
            }
            Err(e) => {
                error!("❌ Failed to send reversible transaction: {}", e);
                return Err(AppError::Transaction(e));
            }
        }

        return Ok(());
    }

    // Initialize transaction manager
    info!("Connecting to Quantus node...");
    let transaction_manager = Arc::new(
        TransactionManager::new(
            &config.blockchain.node_url,
            &config.blockchain.wallet_name,
            &config.blockchain.wallet_password,
            db.clone(),
            config.get_reversal_period_duration(),
        )
        .await?,
    );

    // Perform health check
    if let Err(e) = transaction_manager.health_check().await {
        error!("Node health check failed: {}", e);
        return Err(AppError::Transaction(e));
    }

    let node_info = transaction_manager.get_node_info().await?;
    info!("✅ Connected to: {}", node_info);
    info!("Wallet address: {}", transaction_manager.get_wallet_address());

    // Check wallet balance
    match transaction_manager.get_wallet_balance().await {
        Ok(balance) => info!("Wallet balance: {} units", balance),
        Err(e) => warn!("Could not check wallet balance: {}", e),
    }

    // Initialize task generator
    let mut task_generator = TaskGenerator::new(db.clone());

    // Initial candidate refresh
    info!("Fetching initial candidates...");
    if let Err(e) = task_generator.refresh_candidates(&config.candidates.graphql_url).await {
        error!("Failed to fetch initial candidates: {}", e);
        return Err(AppError::TaskGenerator(e));
    }
    info!("Loaded {} candidates", task_generator.candidates_count());

    if args.run_once {
        info!("Running in single-run mode");
        return run_once(config, task_generator, transaction_manager).await;
    }

    // Start the reverser service
    info!("Starting reverser service...");
    // Tasks will be started directly in the tokio::select! macro

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
        "Task generation interval: {} minutes",
        config.task_generation.generation_interval_minutes
    );
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
        result = start_candidates_refresh_task(
            task_generator.clone(),
            config.candidates.graphql_url.clone(),
            config.get_candidates_refresh_duration(),
        ) => {
            error!("Candidates refresh task exited: {:?}", result);
            result.await??;
        }
        result = tweet_synchronizer.spawn_tweet_synchronizer() => {
            error!("Tweet synchronizer exited: {:?}", result);
            result??;
        }
        result = raid_leaderboard_service.spawn_raid_leaderboard_synchronizer() => {
            error!("Raid leaderboard synchronizer exited: {:?}", result);
            result??;
        }
        // result = start_task_generation_task(
        //     task_generator.clone(),
        //     transaction_manager.clone(),
        //     config.task_generation.taskees_per_round,
        //     config.get_task_generation_duration(),
        // ) => {
        //     error!("Task generation task exited: {:?}", result);
        //     result.await??;
        // }
        result = start_reverser_service(
            db.clone(),
            transaction_manager.clone(),
            config.get_reverser_check_duration(),
            config.get_early_reversal_duration().num_minutes(),
        ) => {
            error!("Reverser service exited: {:?}", result);
            result.await?.map_err(AppError::Reverser)?;
        }
    }

    Ok(())
}

async fn run_once(
    config: Config,
    task_generator: TaskGenerator,
    transaction_manager: Arc<TransactionManager>,
) -> AppResult<()> {
    info!("Generating {} tasks...", config.task_generation.taskees_per_round);

    let tasks = task_generator
        .generate_and_save_tasks(config.task_generation.taskees_per_round)
        .await?;

    info!("Generated {} tasks", tasks.len());

    info!("Processing transactions...");
    let processed = transaction_manager.process_task_batch(tasks).await?;

    info!("Successfully processed {} transactions", processed.len());
    info!("Single run completed successfully");

    Ok(())
}

async fn start_candidates_refresh_task(
    mut task_generator: TaskGenerator,
    graphql_url: String,
    refresh_interval: Duration,
) -> tokio::task::JoinHandle<AppResult<()>> {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(refresh_interval);

        loop {
            interval.tick().await;

            info!("Refreshing candidates...");
            match task_generator.refresh_candidates(&graphql_url).await {
                Ok(()) => {
                    info!("Candidates refreshed: {} available", task_generator.candidates_count());
                }
                Err(e) => {
                    error!("Failed to refresh candidates: {}", e);
                    return Err(AppError::TaskGenerator(e));
                }
            }
        }
    })
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
