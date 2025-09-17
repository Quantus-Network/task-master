use clap::Parser;
use std::sync::Arc;
use tokio::time::Duration;
use tracing::{error, info, warn};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

mod config;
mod db_persistence;
mod http_server;
mod reverser;
mod signature_verification;
mod task_generator;
mod transaction_manager;

use config::Config;
use db_persistence::DbPersistence;
use reverser::start_reverser_service;
use task_generator::TaskGenerator;
use transaction_manager::TransactionManager;

#[derive(Parser, Debug)]
#[command(name = "task-master")]
#[command(about = "Task management server with reversible blockchain transactions")]
struct Args {
    /// Configuration file path
    #[arg(short, long, default_value = "config/default.toml")]
    config: String,

    /// Wallet name override
    #[arg(long)]
    wallet_name: Option<String>,

    /// Wallet password override
    #[arg(long)]
    wallet_password: Option<String>,

    /// Node URL override
    #[arg(long)]
    node_url: Option<String>,

    /// Run once and exit (for testing)
    #[arg(long)]
    run_once: bool,
}

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("Configuration error: {0}")]
    Config(#[from] ::config::ConfigError),
    #[error("Database error: {0}")]
    Database(#[from] db_persistence::DbError),
    #[error("Transaction manager error: {0}")]
    Transaction(#[from] transaction_manager::TransactionError),
    #[error("Task generator error: {0}")]
    TaskGenerator(#[from] task_generator::TaskGeneratorError),
    #[error("Reverser error: {0}")]
    Reverser(#[from] reverser::ReverserError),
    #[error("Server error: {0}")]
    Server(String),
    #[error("Join error: {0}")]
    Join(#[from] tokio::task::JoinError),
}

type AppResult<T> = Result<T, AppError>;

#[tokio::main]
async fn main() -> AppResult<()> {
    let args = Args::parse();

    // Load configuration
    let mut config = Config::load().map_err(AppError::Config)?;

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

    let initial_task_count = db.task_count().await?;
    info!("Loaded {} existing tasks from database", initial_task_count);

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
    info!(
        "Wallet address: {}",
        transaction_manager.get_wallet_address()
    );

    // Check wallet balance
    match transaction_manager.get_wallet_balance().await {
        Ok(balance) => info!("Wallet balance: {} units", balance),
        Err(e) => warn!("Could not check wallet balance: {}", e),
    }

    // Initialize task generator
    let mut task_generator = TaskGenerator::new(db.clone());

    // Initial candidate refresh
    info!("Fetching initial candidates...");
    if let Err(e) = task_generator
        .refresh_candidates(&config.candidates.graphql_url)
        .await
    {
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

    let server_db = db.clone();
    let server_addr_clone = server_address.clone();
    let server_task = tokio::spawn(async move {
        http_server::start_server(server_db, &server_addr_clone)
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
    info!(
        "Reversal period: {} hours",
        config.blockchain.reversal_period_hours
    );

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
        result = start_task_generation_task(
            task_generator.clone(),
            transaction_manager.clone(),
            config.task_generation.taskees_per_round,
            config.get_task_generation_duration(),
        ) => {
            error!("Task generation task exited: {:?}", result);
            result.await??;
        }
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
    info!(
        "Generating {} tasks...",
        config.task_generation.taskees_per_round
    );

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
                    info!(
                        "Candidates refreshed: {} available",
                        task_generator.candidates_count()
                    );
                }
                Err(e) => {
                    error!("Failed to refresh candidates: {}", e);
                    return Err(AppError::TaskGenerator(e));
                }
            }
        }
    })
}

async fn start_task_generation_task(
    task_generator: TaskGenerator,
    transaction_manager: Arc<TransactionManager>,
    taskees_per_round: usize,
    generation_interval: Duration,
) -> tokio::task::JoinHandle<AppResult<()>> {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(generation_interval);

        loop {
            interval.tick().await;

            info!("Generating new batch of {} tasks...", taskees_per_round);

            let tasks = match task_generator
                .generate_and_save_tasks(taskees_per_round)
                .await
            {
                Ok(tasks) => tasks,
                Err(e) => {
                    error!("Failed to generate tasks: {}", e);
                    return Err(AppError::TaskGenerator(e));
                }
            };

            info!(
                "Generated {} tasks, processing transactions...",
                tasks.len()
            );

            match transaction_manager.process_task_batch(tasks).await {
                Ok(processed) => {
                    info!("Successfully processed {} transactions", processed.len());
                }
                Err(e) => {
                    error!("Failed to process transaction batch: {}", e);
                    return Err(AppError::Transaction(e));
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
