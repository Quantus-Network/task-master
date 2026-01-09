use crate::{
    db_persistence::{DbError, DbPersistence},
    models::task::TaskStatus,
    services::transaction_manager::TransactionManager,
};
use std::sync::Arc;
use subxt::error::TransactionError;
use tokio::time::{interval, Duration};

#[derive(Debug, thiserror::Error)]
pub enum ReverserError {
    #[error("Database error: {0}")]
    Database(#[from] DbError),
    #[error("Transaction error: {0}")]
    Transaction(#[from] TransactionError),
    #[error("Reverser service error: {0}")]
    Service(String),
}

pub type ReverserResult<T> = Result<T, ReverserError>;

pub struct ReverserService {
    db: Arc<DbPersistence>,
    transaction_manager: Arc<TransactionManager>,
    check_interval: Duration,
    early_reversal_minutes: i64,
}

impl ReverserService {
    pub fn new(
        db: Arc<DbPersistence>,
        transaction_manager: Arc<TransactionManager>,
        check_interval: Duration,
        early_reversal_minutes: i64,
    ) -> Self {
        Self {
            db,
            transaction_manager,
            check_interval,
            early_reversal_minutes,
        }
    }

    /// Start the reverser service monitoring loop
    pub async fn start(&self) -> ReverserResult<()> {
        tracing::info!(
            "Starting reverser service with {} minute early reversal and {} second check interval",
            self.early_reversal_minutes,
            self.check_interval.as_secs()
        );

        let mut interval_timer = interval(self.check_interval);

        loop {
            interval_timer.tick().await;

            if let Err(e) = self.check_and_reverse_tasks().await {
                tracing::error!("Error in reverser service: {}", e);
                // For now, log and die as requested
                return Err(e);
            }
        }
    }

    /// Check for tasks that need to be reversed and reverse them
    async fn check_and_reverse_tasks(&self) -> ReverserResult<()> {
        let tasks_to_reverse = self
            .db
            .tasks
            .get_tasks_ready_for_reversal(self.early_reversal_minutes)
            .await?;

        if tasks_to_reverse.is_empty() {
            tracing::debug!("No tasks ready for reversal");
            return Ok(());
        }

        tracing::info!("Found {} tasks ready for reversal", tasks_to_reverse.len());

        let mut reversal_count = 0;
        let mut error_count = 0;

        for task in tasks_to_reverse {
            tracing::info!(
                "Reversing task {} (quan_address: {}, quan_amount: {}, usdc_amount: {}, tx: {})",
                task.task_id,
                task.quan_address.0,
                task.quan_amount.0,
                task.usdc_amount,
                task.reversible_tx_id.as_deref().unwrap_or("none")
            );

            match self.transaction_manager.reverse_transaction(&task.task_id).await {
                Ok(()) => {
                    reversal_count += 1;
                    tracing::info!("Successfully reversed task {}", task.task_id);
                }
                Err(e) => {
                    error_count += 1;
                    tracing::error!("Failed to reverse task {}: {}", task.task_id, e);

                    // Mark task as failed if reversal failed
                    if let Err(db_err) = self
                        .db
                        .tasks
                        .update_task_status(&task.task_id, TaskStatus::Failed)
                        .await
                    {
                        tracing::error!(
                            "Failed to mark task {} as failed after reversal error: {}",
                            task.task_id,
                            db_err
                        );
                    }
                }
            }
        }

        tracing::info!(
            "Reversal batch completed: {} successful, {} errors",
            reversal_count,
            error_count
        );

        // If there were any errors, return an error to trigger the "log and die" behavior
        if error_count > 0 {
            return Err(ReverserError::Service(format!(
                "Failed to reverse {} out of {} tasks",
                error_count,
                reversal_count + error_count
            )));
        }

        Ok(())
    }
}

/// Start the reverser service in a background task
pub async fn start_reverser_service(
    db: Arc<DbPersistence>,
    transaction_manager: Arc<TransactionManager>,
    check_interval: Duration,
    early_reversal_minutes: i64,
) -> tokio::task::JoinHandle<ReverserResult<()>> {
    let reverser = ReverserService::new(db, transaction_manager, check_interval, early_reversal_minutes);

    tokio::spawn(async move { reverser.start().await })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        config::Config,
        db_persistence::DbPersistence,
        models::address::{Address, AddressInput},
        models::task::{Task, TaskInput, TaskStatus},
        services::transaction_manager::TransactionManager,
        utils::generate_referral_code::generate_referral_code,
        utils::test_db::reset_database,
    };
    use chrono::{Duration as ChronoDuration, Utc};
    use quantus_cli::wallet::WalletManager;
    use uuid::Uuid;

    // Helper to set up a full test environment with a DB, TransactionManager, and ReverserService.
    // NOTE: Requires a local Quantus node running.
    async fn setup_test_reverser() -> (ReverserService, Arc<TransactionManager>, Arc<DbPersistence>) {
        let config = Config::load_test_env().expect("Failed to load test configuration");
        std::env::set_var("TASKMASTER_USE_DEV_ALICE", "1");
        let db = Arc::new(DbPersistence::new(config.get_database_url()).await.unwrap());

        reset_database(&db.pool).await;

        let wallet_name = "//Alice";
        let transaction_manager = Arc::new(
            TransactionManager::new(
                &config.blockchain.node_url,
                &wallet_name,
                "password",
                db.clone(),
                ChronoDuration::seconds(60),
            )
            .await
            .unwrap(),
        );

        let reverser = ReverserService::new(
            db.clone(),
            transaction_manager.clone(),
            Duration::from_secs(10),
            5, // 5 minute early reversal window for tests
        );

        (reverser, transaction_manager, db)
    }

    // Helper to create a task that is ready for reversal
    async fn create_reversable_task(
        db: &DbPersistence,
        tm: &TransactionManager,
        id: &str, // Used to keep task_url unique
    ) -> Task {
        let wallet_manager = WalletManager::new().unwrap();
        let recipient_wallet_name = format!("test_recipient_{}", Uuid::new_v4());
        let recipient_info = wallet_manager
            .create_wallet(&recipient_wallet_name, Some("password"))
            .await
            .unwrap();
        // This is a real, valid SS58 address that the node will accept.
        let quan_address = recipient_info.address;

        // Create and save the Address and Task objects using the valid address.
        let referral_code = generate_referral_code(quan_address.clone()).await.unwrap();
        let address = Address::new(AddressInput {
            quan_address,
            referral_code,
        })
        .unwrap();
        db.addresses.create(&address).await.unwrap();

        let task = Task::new(TaskInput {
            quan_address: address.quan_address.0,
            quan_amount: 1000,
            task_url: format!("http://example.com/{}", id),
        })
        .unwrap();
        let task_id = db.tasks.create(&task).await.unwrap();

        tm.send_reversible_transaction(&task_id).await.unwrap();

        // Manually update the task's end_time to be within the reversal window.
        let new_end_time = Utc::now() + ChronoDuration::minutes(2);
        sqlx::query("UPDATE tasks SET end_time = $1 WHERE task_id = $2")
            .bind(new_end_time)
            .bind(&task.task_id)
            .execute(&db.pool)
            .await
            .unwrap();

        // Return the fully prepared task.
        db.tasks.get_task(&task_id).await.unwrap().unwrap()
    }

    #[tokio::test]
    async fn chain_test_check_and_reverse_tasks_success() {
        let (reverser, tm, db) = setup_test_reverser().await;

        // Arrange: Create a task that is ready to be reversed.
        let task = create_reversable_task(&db, &tm, "001").await;
        assert_eq!(task.status, TaskStatus::Pending);

        // Act: Run the reversal check.
        reverser.check_and_reverse_tasks().await.unwrap();

        // Assert: The task status in the DB should now be 'Reversed'.
        let reversed_task = db.tasks.get_task(&task.task_id).await.unwrap().unwrap();
        assert_eq!(reversed_task.status, TaskStatus::Reversed);
    }

    #[tokio::test]
    async fn chain_test_check_and_reverse_does_nothing_if_no_tasks_ready() {
        let (reverser, tm, db) = setup_test_reverser().await;

        // Arrange: Create a task, send its transaction, but its end_time is far in the future.
        let task = create_reversable_task(&db, &tm, "002").await;
        let future_end_time = Utc::now() + ChronoDuration::hours(1);
        sqlx::query("UPDATE tasks SET end_time = $1 WHERE task_id = $2")
            .bind(future_end_time)
            .bind(&task.task_id)
            .execute(&db.pool)
            .await
            .unwrap();

        // Act: Run the reversal check.
        reverser.check_and_reverse_tasks().await.unwrap();

        // Assert: The task should not have been reversed.
        let not_reversed_task = db.tasks.get_task(&task.task_id).await.unwrap().unwrap();
        assert_eq!(not_reversed_task.status, TaskStatus::Pending);
    }
}
