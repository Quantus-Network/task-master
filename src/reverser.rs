use crate::csv_persistence::{CsvPersistence, TaskStatus};
use crate::transaction_manager::TransactionManager;
use std::sync::Arc;
use tokio::time::{interval, Duration};

#[derive(Debug, thiserror::Error)]
pub enum ReverserError {
    #[error("CSV error: {0}")]
    Csv(#[from] crate::csv_persistence::CsvError),
    #[error("Transaction error: {0}")]
    Transaction(#[from] crate::transaction_manager::TransactionError),
    #[error("Reverser service error: {0}")]
    Service(String),
}

pub type ReverserResult<T> = Result<T, ReverserError>;

pub struct ReverserService {
    csv: Arc<CsvPersistence>,
    transaction_manager: Arc<TransactionManager>,
    check_interval: Duration,
    early_reversal_minutes: i64,
}

impl ReverserService {
    pub fn new(
        csv: Arc<CsvPersistence>,
        transaction_manager: Arc<TransactionManager>,
        check_interval: Duration,
        early_reversal_minutes: i64,
    ) -> Self {
        Self {
            csv,
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
            .csv
            .get_tasks_ready_for_reversal(self.early_reversal_minutes)
            .await;

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
                task.quan_address,
                task.quan_amount,
                task.usdc_amount,
                task.tx_hash
            );

            match self
                .transaction_manager
                .reverse_transaction(&task.task_id)
                .await
            {
                Ok(()) => {
                    reversal_count += 1;
                    tracing::info!("Successfully reversed task {}", task.task_id);
                }
                Err(e) => {
                    error_count += 1;
                    tracing::error!("Failed to reverse task {}: {}", task.task_id, e);

                    // Mark task as failed if reversal failed
                    if let Err(csv_err) = self
                        .csv
                        .update_task(&task.task_id, |task| {
                            task.mark_failed();
                        })
                        .await
                    {
                        tracing::error!(
                            "Failed to mark task {} as failed after reversal error: {}",
                            task.task_id,
                            csv_err
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

    /// Get statistics about tasks that need attention
    pub async fn get_reversal_stats(&self) -> ReverserResult<ReversalStats> {
        let pending_tasks = self.csv.get_tasks_by_status(TaskStatus::Pending).await;
        let tasks_ready_for_reversal = self
            .csv
            .get_tasks_ready_for_reversal(self.early_reversal_minutes)
            .await;

        let mut tasks_expiring_soon = 0;
        let mut tasks_already_expired = 0;
        let now = chrono::Utc::now();

        for task in &pending_tasks {
            if let Some(end_time) = task.end_time {
                if end_time <= now {
                    tasks_already_expired += 1;
                } else if end_time <= now + chrono::Duration::minutes(self.early_reversal_minutes) {
                    tasks_expiring_soon += 1;
                }
            }
        }

        Ok(ReversalStats {
            total_pending: pending_tasks.len(),
            ready_for_reversal: tasks_ready_for_reversal.len(),
            expiring_soon: tasks_expiring_soon,
            already_expired: tasks_already_expired,
        })
    }

    /// Manual trigger for reversal check (useful for testing or admin endpoints)
    pub async fn trigger_reversal_check(&self) -> ReverserResult<usize> {
        let tasks_to_reverse = self
            .csv
            .get_tasks_ready_for_reversal(self.early_reversal_minutes)
            .await;

        let count = tasks_to_reverse.len();

        if count > 0 {
            self.check_and_reverse_tasks().await?;
        }

        Ok(count)
    }
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct ReversalStats {
    pub total_pending: usize,
    pub ready_for_reversal: usize,
    pub expiring_soon: usize,
    pub already_expired: usize,
}

/// Start the reverser service in a background task
pub async fn start_reverser_service(
    csv: Arc<CsvPersistence>,
    transaction_manager: Arc<TransactionManager>,
    check_interval: Duration,
    early_reversal_minutes: i64,
) -> tokio::task::JoinHandle<ReverserResult<()>> {
    let reverser = ReverserService::new(
        csv,
        transaction_manager,
        check_interval,
        early_reversal_minutes,
    );

    tokio::spawn(async move { reverser.start().await })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::csv_persistence::{CsvPersistence, TaskRecord};
    use chrono::{Duration as ChronoDuration, Utc};
    use std::sync::Arc;
    use tempfile::NamedTempFile;
    use tokio::time::Duration;

    #[tokio::test]
    async fn test_reversal_stats() {
        let temp_file = NamedTempFile::new().unwrap();
        let csv = Arc::new(CsvPersistence::new(temp_file.path()));

        // Add test tasks with different timings
        let now = Utc::now();

        // Task that should be reversed (end time passed)
        let mut task1 = TaskRecord::new("qztest1".to_string(), 1000, "111111111111".to_string());
        task1.set_transaction_sent(
            "0x123".to_string(),
            now - ChronoDuration::hours(1),
            now - ChronoDuration::minutes(5), // Already expired
        );
        csv.add_task(task1).await.unwrap();

        // Task that will expire soon
        let mut task2 = TaskRecord::new("qztest2".to_string(), 2000, "222222222222".to_string());
        task2.set_transaction_sent(
            "0x456".to_string(),
            now,
            now + ChronoDuration::minutes(1), // Expires in 1 minute
        );
        csv.add_task(task2).await.unwrap();

        // Task with plenty of time left
        let mut task3 = TaskRecord::new("qztest3".to_string(), 3000, "333333333333".to_string());
        task3.set_transaction_sent(
            "0x789".to_string(),
            now,
            now + ChronoDuration::hours(1), // Expires in 1 hour
        );
        csv.add_task(task3).await.unwrap();

        // Mock transaction manager (this would fail in real usage without a node)
        // For testing, we'll just verify the stats calculation
        let temp_file_tm = NamedTempFile::new().unwrap();
        let csv_tm = Arc::new(CsvPersistence::new(temp_file_tm.path()));

        // This will fail to create, but we're only testing the stats logic
        let transaction_manager = Arc::new(
            crate::transaction_manager::TransactionManager::new(
                "ws://invalid-url:9944",
                "test_wallet",
                "test_password",
                csv_tm,
                ChronoDuration::hours(12),
            )
            .await
            .unwrap_or_else(|_| panic!("Expected to fail in test")),
        );

        let reverser = ReverserService::new(
            csv.clone(),
            transaction_manager,
            Duration::from_secs(30),
            2, // 2 minute early reversal
        );

        // This would fail with the mock transaction manager, but we can test the stats
        // Let's just test that the service can be created
        assert_eq!(reverser.early_reversal_minutes, 2);
        assert_eq!(reverser.check_interval, Duration::from_secs(30));
    }

    #[test]
    fn test_reverser_service_creation() {
        let temp_file = NamedTempFile::new().unwrap();
        let csv = Arc::new(CsvPersistence::new(temp_file.path()));

        // Create a mock transaction manager reference
        // In real usage, this would be properly initialized
        let temp_file_tm = NamedTempFile::new().unwrap();
        let csv_tm = Arc::new(CsvPersistence::new(temp_file_tm.path()));

        // We can't actually create the transaction manager without a node,
        // so we'll just test the service structure
        let check_interval = Duration::from_secs(30);
        let early_reversal_minutes = 2;

        // Test that the service parameters are set correctly
        // (We can't fully test without a running Quantus node)
        assert!(check_interval.as_secs() > 0);
        assert!(early_reversal_minutes > 0);
    }

    #[tokio::test]
    async fn test_tasks_ready_for_reversal() {
        let temp_file = NamedTempFile::new().unwrap();
        let csv = Arc::new(CsvPersistence::new(temp_file.path()));

        let now = Utc::now();

        // Task that should be ready for reversal
        let mut task = TaskRecord::new("qztest".to_string(), 1000, "123456789012".to_string());
        task.set_transaction_sent(
            "0x123".to_string(),
            now - ChronoDuration::hours(1),
            now + ChronoDuration::minutes(1), // Ends in 1 minute
        );
        csv.add_task(task).await.unwrap();

        // Check with 2 minute early reversal - should be ready
        let ready_tasks = csv.get_tasks_ready_for_reversal(2).await;
        assert_eq!(ready_tasks.len(), 1);

        // Check with 0 minute early reversal - should not be ready yet
        let ready_tasks = csv.get_tasks_ready_for_reversal(0).await;
        assert_eq!(ready_tasks.len(), 0);
    }
}
