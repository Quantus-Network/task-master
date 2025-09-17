use crate::db_persistence::{DbPersistence, TaskRecord};
use chrono::Utc;
use quantus_cli::chain::client::QuantusClient;
use quantus_cli::cli::reversible::{cancel_transaction, schedule_transfer};
use quantus_cli::wallet::{QuantumKeyPair, WalletManager};
use std::sync::Arc;
use tokio::sync::RwLock;

#[derive(Debug, thiserror::Error)]
pub enum TransactionError {
    #[error("Quantus client error: {0}")]
    QuantusClient(#[from] quantus_cli::error::QuantusError),
    #[error("Wallet error: {0}")]
    Wallet(#[from] quantus_cli::error::WalletError),
    #[error("CSV error: {0}")]
    Database(#[from] crate::db_persistence::DbError),
    #[error("Transaction not found: {0}")]
    TransactionNotFound(String),
    #[error("Invalid transaction state: {0}")]
    InvalidState(String),
}

pub type TransactionResult<T> = Result<T, TransactionError>;

pub struct TransactionManager {
    client: Arc<RwLock<QuantusClient>>,
    keypair: QuantumKeyPair,
    db: Arc<DbPersistence>,
    reversal_period: chrono::Duration,
}

impl TransactionManager {
    pub async fn new(
        node_url: &str,
        wallet_name: &str,
        wallet_password: &str,
        db: Arc<DbPersistence>,
        reversal_period: chrono::Duration,
    ) -> TransactionResult<Self> {
        // Connect to Quantus node
        let client = QuantusClient::new(node_url).await?;
        let client = Arc::new(RwLock::new(client));

        // Initialize wallet manager
        let wallet_manager = WalletManager::new()?;

        // Load or create wallet
        let keypair = match wallet_manager.load_wallet(wallet_name, wallet_password) {
            Ok(wallet_data) => {
                tracing::info!("Loaded existing wallet: {}", wallet_name);
                wallet_data.keypair
            }
            Err(_) => {
                tracing::info!("Creating new wallet: {}", wallet_name);
                let wallet_info = wallet_manager
                    .create_wallet(wallet_name, Some(wallet_password))
                    .await?;
                tracing::info!("Created wallet with address: {}", wallet_info.address);

                // Load the newly created wallet
                wallet_manager
                    .load_wallet(wallet_name, wallet_password)?
                    .keypair
            }
        };

        tracing::info!(
            "Transaction manager initialized with wallet address: {}",
            keypair.to_account_id_ss58check()
        );

        Ok(Self {
            client,
            keypair,
            db,
            reversal_period,
        })
    }

    /// Send a reversible transaction for a task
    pub async fn send_reversible_transaction(&self, task_id: &str) -> TransactionResult<()> {
        let task = self
            .db
            .get_task(task_id)
            .await?
            .ok_or_else(|| TransactionError::TransactionNotFound(task_id.to_string()))?;

        tracing::info!(
            "Sending reversible transaction for task {} to {} (quan_amount: {})",
            task_id,
            task.quan_address,
            task.quan_amount
        );

        // Send the transaction
        let client = self.client.read().await;
        let tx_hash = schedule_transfer(
            &*client,
            &self.keypair,
            &task.quan_address,
            task.quan_amount as u128, // Convert to u128 for quantus-cli
        )
        .await?;

        drop(client);

        // Calculate end time (current time + reversal period)
        let send_time = Utc::now();
        let end_time = send_time + self.reversal_period;

        // Update task with transaction details
        self.db
            .update_task_transaction(task_id, format!("0x{:x}", tx_hash), send_time, end_time)
            .await?;

        tracing::info!(
            "Transaction sent successfully. Hash: 0x{:x}, End time: {}",
            tx_hash,
            end_time.format("%Y-%m-%d %H:%M:%S UTC")
        );

        Ok(())
    }

    /// Cancel/reverse a transaction
    pub async fn reverse_transaction(&self, task_id: &str) -> TransactionResult<()> {
        let task = self
            .db
            .get_task(task_id)
            .await?
            .ok_or_else(|| TransactionError::TransactionNotFound(task_id.to_string()))?;

        let reversible_tx_id = task.reversible_tx_id.as_ref().ok_or_else(|| {
            TransactionError::InvalidState(
                "Task has no reversible transaction ID to reverse".to_string(),
            )
        })?;

        // Remove "0x" prefix if present for the cancel call
        let tx_hash_str = reversible_tx_id
            .strip_prefix("0x")
            .unwrap_or(reversible_tx_id);

        tracing::info!(
            "Reversing transaction for task {} (tx: {})",
            task_id,
            reversible_tx_id
        );

        let client = self.client.read().await;
        let cancel_tx_hash = cancel_transaction(&*client, &self.keypair, tx_hash_str).await?;

        drop(client);

        // Update task status
        self.db
            .update_task_status(task_id, crate::db_persistence::TaskStatus::Reversed)
            .await?;

        tracing::info!(
            "Transaction reversed successfully. Cancel tx: 0x{:x}",
            cancel_tx_hash
        );

        Ok(())
    }

    /// Process a batch of tasks for transaction sending
    pub async fn process_task_batch(
        &self,
        tasks: Vec<TaskRecord>,
    ) -> TransactionResult<Vec<String>> {
        let mut processed_tasks = Vec::new();
        let task_count = tasks.len();

        for task in tasks {
            match self.send_reversible_transaction(&task.task_id).await {
                Ok(()) => {
                    processed_tasks.push(task.task_id.clone());
                    tracing::info!("Successfully processed task: {}", task.task_id);
                }
                Err(e) => {
                    tracing::error!("Failed to process task {}: {}", task.task_id, e);

                    // Mark task as failed
                    if let Err(db_err) = self
                        .db
                        .update_task_status(
                            &task.task_id,
                            crate::db_persistence::TaskStatus::Failed,
                        )
                        .await
                    {
                        tracing::error!("Failed to mark task as failed: {}", db_err);
                    }
                }
            }
        }

        tracing::info!(
            "Batch processing completed. {}/{} tasks processed successfully",
            processed_tasks.len(),
            task_count
        );

        Ok(processed_tasks)
    }

    /// Get wallet balance
    pub async fn get_wallet_balance(&self) -> TransactionResult<u128> {
        let client = self.client.read().await;
        let account_id = self.keypair.to_account_id_32();

        // Convert to subxt AccountId32
        let account_bytes: [u8; 32] = *account_id.as_ref();
        let subxt_account_id = subxt::utils::AccountId32::from(account_bytes);

        use quantus_cli::chain::quantus_subxt::api;
        let storage_addr = api::storage().system().account(subxt_account_id);
        let account_info = client
            .client()
            .storage()
            .at_latest()
            .await
            .map_err(|e| {
                TransactionError::QuantusClient(quantus_cli::error::QuantusError::NetworkError(
                    e.to_string(),
                ))
            })?
            .fetch_or_default(&storage_addr)
            .await
            .map_err(|e| {
                TransactionError::QuantusClient(quantus_cli::error::QuantusError::NetworkError(
                    e.to_string(),
                ))
            })?;

        Ok(account_info.data.free)
    }

    /// Get wallet address
    pub fn get_wallet_address(&self) -> String {
        self.keypair.to_account_id_ss58check()
    }

    /// Health check - verify connection to node
    pub async fn health_check(&self) -> TransactionResult<bool> {
        let client = self.client.read().await;
        match client.get_runtime_version().await {
            Ok(_) => {
                tracing::debug!("Health check passed - connected to Quantus node");
                Ok(true)
            }
            Err(e) => {
                tracing::error!("Health check failed: {}", e);
                Err(TransactionError::QuantusClient(e))
            }
        }
    }

    /// Get node info for debugging
    pub async fn get_node_info(&self) -> TransactionResult<String> {
        let client = self.client.read().await;
        let runtime_version = client.get_runtime_version().await?;
        Ok(format!(
            "Quantus Node - Runtime Version: {}.{}",
            runtime_version.0, runtime_version.1
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db_persistence::DbPersistence;
    use tempfile::NamedTempFile;

    // Note: These tests would require a running Quantus node
    // For now, they test the structure and error handling

    #[test]
    fn test_transaction_error_display() {
        let error = TransactionError::TransactionNotFound("test".to_string());
        assert!(error.to_string().contains("Transaction not found: test"));
    }

    #[tokio::test]
    async fn test_wallet_address_format() {
        // This test can run without a node connection
        // This will fail without a node, but we can test the error handling
        let temp_file = NamedTempFile::new().unwrap();
        let db_url = format!("sqlite:{}", temp_file.path().to_string_lossy());
        let db = Arc::new(DbPersistence::new(&db_url).await.unwrap());

        // This will fail with connection error
        let result = TransactionManager::new(
            "ws://invalid-url:9944",
            "test_wallet",
            "test_password",
            db,
            chrono::Duration::hours(12),
        )
        .await;

        // Should fail with connection error
        assert!(result.is_err());
    }
}
