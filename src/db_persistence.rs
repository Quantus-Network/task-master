use chrono::{DateTime, Utc};
use rand::prelude::*;
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, Row, SqlitePool};
use std::collections::HashMap;
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum TaskStatus {
    #[serde(rename = "pending")]
    Pending,
    #[serde(rename = "completed")]
    Completed,
    #[serde(rename = "reversed")]
    Reversed,
    #[serde(rename = "failed")]
    Failed,
}

impl std::fmt::Display for TaskStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TaskStatus::Pending => write!(f, "pending"),
            TaskStatus::Completed => write!(f, "completed"),
            TaskStatus::Reversed => write!(f, "reversed"),
            TaskStatus::Failed => write!(f, "failed"),
        }
    }
}

impl std::str::FromStr for TaskStatus {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "pending" => Ok(TaskStatus::Pending),
            "completed" => Ok(TaskStatus::Completed),
            "reversed" => Ok(TaskStatus::Reversed),
            "failed" => Ok(TaskStatus::Failed),
            _ => Err(format!("Invalid task status: {}", s)),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Address {
    pub id: i64,
    pub quan_address: String,
    pub eth_address: Option<String>,
    pub created_at: DateTime<Utc>,
    pub last_selected_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskRecord {
    pub id: Option<i64>,
    pub task_id: String,
    pub quan_address: String,
    pub quan_amount: u64,
    pub usdc_amount: u64,
    pub task_url: String,
    pub status: TaskStatus,
    pub reversible_tx_id: Option<String>,
    pub send_time: Option<DateTime<Utc>>,
    pub end_time: Option<DateTime<Utc>>,
    pub created_at: Option<DateTime<Utc>>,
    pub updated_at: Option<DateTime<Utc>>,
}

impl FromRow<'_, sqlx::sqlite::SqliteRow> for TaskRecord {
    fn from_row(row: &sqlx::sqlite::SqliteRow) -> Result<Self, sqlx::Error> {
        let status_str: String = row.try_get("status")?;
        let status = status_str.parse::<TaskStatus>().map_err(|e| {
            sqlx::Error::Decode(Box::new(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                e,
            )))
        })?;

        Ok(TaskRecord {
            id: Some(row.try_get("id")?),
            task_id: row.try_get("task_id")?,
            quan_address: row.try_get("quan_address")?,
            quan_amount: row.try_get::<i64, _>("quan_amount")? as u64,
            usdc_amount: row.try_get::<i64, _>("usdc_amount")? as u64,
            task_url: row.try_get("task_url")?,
            status,
            reversible_tx_id: row.try_get("reversible_tx_id")?,
            send_time: row.try_get("send_time")?,
            end_time: row.try_get("end_time")?,
            created_at: Some(row.try_get("created_at")?),
            updated_at: Some(row.try_get("updated_at")?),
        })
    }
}

impl TaskRecord {
    pub fn new(quan_address: String, quan_amount: u64, task_url: String) -> Self {
        let mut rng = rand::rng();
        let usdc_amount = rng.random_range(1..=25);

        Self {
            id: None,
            task_id: Uuid::new_v4().to_string(),
            quan_address,
            quan_amount,
            usdc_amount,
            task_url,
            status: TaskStatus::Pending,
            reversible_tx_id: None,
            send_time: None,
            end_time: None,
            created_at: None,
            updated_at: None,
        }
    }

    pub fn set_transaction_sent(
        &mut self,
        reversible_tx_id: String,
        send_time: DateTime<Utc>,
        end_time: DateTime<Utc>,
    ) {
        self.reversible_tx_id = Some(reversible_tx_id);
        self.send_time = Some(send_time);
        self.end_time = Some(end_time);
        self.status = TaskStatus::Pending;
    }

    pub fn mark_completed(&mut self) {
        self.status = TaskStatus::Completed;
    }

    pub fn mark_reversed(&mut self) {
        self.status = TaskStatus::Reversed;
    }

    pub fn mark_failed(&mut self) {
        self.status = TaskStatus::Failed;
    }

    pub fn is_ready_for_reversal(&self, early_minutes: i64) -> bool {
        if self.status != TaskStatus::Pending {
            return false;
        }

        if let Some(end_time) = self.end_time {
            let reversal_time = end_time - chrono::Duration::minutes(early_minutes);
            Utc::now() >= reversal_time
        } else {
            false
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum DbError {
    #[error("Database error: {0}")]
    Database(#[from] sqlx::Error),
    #[error("Migration error: {0}")]
    Migration(#[from] sqlx::migrate::MigrateError),
    #[error("Task not found: {0}")]
    TaskNotFound(String),
    #[error("Address not found: {0}")]
    AddressNotFound(String),
    #[error("Invalid task status: {0}")]
    InvalidStatus(String),
}

pub type DbResult<T> = Result<T, DbError>;

#[derive(Debug, Clone)]
pub struct DbPersistence {
    pool: SqlitePool,
}

impl DbPersistence {
    pub async fn new(database_url: &str) -> DbResult<Self> {
        let pool = SqlitePool::connect(database_url).await?;

        // Run migrations
        sqlx::migrate!("./migrations").run(&pool).await?;

        Ok(Self { pool })
    }

    // Address operations
    pub async fn add_address(
        &self,
        quan_address: String,
        eth_address: Option<String>,
    ) -> DbResult<i64> {
        let result = sqlx::query!(
            "INSERT OR IGNORE INTO addresses (quan_address, eth_address) VALUES (?, ?)",
            quan_address,
            eth_address
        )
        .execute(&self.pool)
        .await?;

        if result.rows_affected() > 0 {
            Ok(result.last_insert_rowid())
        } else {
            // Address already exists, get its ID
            let row = sqlx::query!(
                "SELECT id FROM addresses WHERE quan_address = ?",
                quan_address
            )
            .fetch_one(&self.pool)
            .await?;
            Ok(row.id.unwrap_or(0))
        }
    }

    pub async fn add_addresses(&self, addresses: Vec<(String, Option<String>)>) -> DbResult<()> {
        let mut tx = self.pool.begin().await?;

        for (quan_address, eth_address) in addresses {
            sqlx::query!(
                "INSERT OR IGNORE INTO addresses (quan_address, eth_address) VALUES (?, ?)",
                quan_address,
                eth_address
            )
            .execute(&mut *tx)
            .await?;
        }

        tx.commit().await?;
        Ok(())
    }

    pub async fn get_all_addresses(&self) -> DbResult<Vec<Address>> {
        let addresses = sqlx::query_as::<_, Address>("SELECT * FROM addresses ORDER BY created_at")
            .fetch_all(&self.pool)
            .await?;
        Ok(addresses)
    }

    pub async fn update_address_last_selected(&self, quan_address: &str) -> DbResult<()> {
        sqlx::query!(
            "UPDATE addresses SET last_selected_at = CURRENT_TIMESTAMP WHERE quan_address = ?",
            quan_address
        )
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    // Task operations
    /// Add a task
    pub async fn add_task(&self, task: TaskRecord) -> DbResult<i64> {
        let quan_amount = task.quan_amount as i64;
        let usdc_amount = task.usdc_amount as i64;
        let status = task.status.to_string();

        let result = sqlx::query!(
            r#"
            INSERT INTO tasks (
                task_id, quan_address, quan_amount, usdc_amount, task_url,
                status, reversible_tx_id, send_time, end_time
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#,
            task.task_id,
            task.quan_address,
            quan_amount,
            usdc_amount,
            task.task_url,
            status,
            task.reversible_tx_id,
            task.send_time,
            task.end_time
        )
        .execute(&self.pool)
        .await?;

        Ok(result.last_insert_rowid())
    }

    pub async fn get_task(&self, task_id: &str) -> DbResult<Option<TaskRecord>> {
        let task = sqlx::query_as::<_, TaskRecord>("SELECT * FROM tasks WHERE task_id = ?")
            .bind(task_id)
            .fetch_optional(&self.pool)
            .await?;
        Ok(task)
    }

    pub async fn find_task_by_url(&self, task_url: &str) -> DbResult<Option<TaskRecord>> {
        let task = sqlx::query_as::<_, TaskRecord>("SELECT * FROM tasks WHERE task_url = ?")
            .bind(task_url)
            .fetch_optional(&self.pool)
            .await?;
        Ok(task)
    }

    pub async fn update_task_status(&self, task_id: &str, status: TaskStatus) -> DbResult<()> {
        let status_str = status.to_string();
        let result = sqlx::query!(
            "UPDATE tasks SET status = ?, updated_at = CURRENT_TIMESTAMP WHERE task_id = ?",
            status_str,
            task_id
        )
        .execute(&self.pool)
        .await?;

        if result.rows_affected() == 0 {
            return Err(DbError::TaskNotFound(task_id.to_string()));
        }

        Ok(())
    }

    pub async fn update_task_transaction(
        &self,
        task_id: &str,
        reversible_tx_id: String,
        send_time: DateTime<Utc>,
        end_time: DateTime<Utc>,
    ) -> DbResult<()> {
        let status_str = TaskStatus::Pending.to_string();
        let result = sqlx::query!(
            r#"
            UPDATE tasks
            SET reversible_tx_id = ?, send_time = ?, end_time = ?,
                status = ?, updated_at = CURRENT_TIMESTAMP
            WHERE task_id = ?
            "#,
            reversible_tx_id,
            send_time,
            end_time,
            status_str,
            task_id
        )
        .execute(&self.pool)
        .await?;

        if result.rows_affected() == 0 {
            return Err(DbError::TaskNotFound(task_id.to_string()));
        }

        Ok(())
    }

    pub async fn get_tasks_by_status(&self, status: TaskStatus) -> DbResult<Vec<TaskRecord>> {
        let tasks = sqlx::query_as::<_, TaskRecord>(
            "SELECT * FROM tasks WHERE status = ? ORDER BY created_at",
        )
        .bind(status.to_string())
        .fetch_all(&self.pool)
        .await?;
        Ok(tasks)
    }

    pub async fn get_tasks_ready_for_reversal(
        &self,
        early_minutes: i64,
    ) -> DbResult<Vec<TaskRecord>> {
        let cutoff_time = Utc::now() + chrono::Duration::minutes(early_minutes);

        let tasks = sqlx::query_as::<_, TaskRecord>(
            "SELECT * FROM tasks WHERE status = ? AND end_time IS NOT NULL AND end_time <= ?",
        )
        .bind(TaskStatus::Pending.to_string())
        .bind(cutoff_time)
        .fetch_all(&self.pool)
        .await?;
        Ok(tasks)
    }

    pub async fn get_all_tasks(&self) -> DbResult<Vec<TaskRecord>> {
        let tasks = sqlx::query_as::<_, TaskRecord>("SELECT * FROM tasks ORDER BY created_at DESC")
            .fetch_all(&self.pool)
            .await?;
        Ok(tasks)
    }

    pub async fn task_count(&self) -> DbResult<i64> {
        let row = sqlx::query!("SELECT COUNT(*) as count FROM tasks")
            .fetch_one(&self.pool)
            .await?;
        Ok(row.count as i64)
    }

    pub async fn status_counts(&self) -> DbResult<HashMap<TaskStatus, usize>> {
        let rows = sqlx::query!("SELECT status, COUNT(*) as count FROM tasks GROUP BY status")
            .fetch_all(&self.pool)
            .await?;

        let mut counts = HashMap::new();
        for row in rows {
            if let Ok(status) = row.status.parse::<TaskStatus>() {
                counts.insert(status, row.count as usize);
            }
        }

        Ok(counts)
    }

    pub async fn get_address_stats(&self) -> DbResult<Vec<(String, i64)>> {
        let rows = sqlx::query!(
            r#"
            SELECT a.quan_address, COUNT(t.id) as task_count
            FROM addresses a
            LEFT JOIN tasks t ON a.quan_address = t.quan_address
            GROUP BY a.quan_address
            ORDER BY task_count DESC
            "#
        )
        .fetch_all(&self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(|row| (row.quan_address, row.task_count))
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;

    async fn create_test_db() -> DbPersistence {
        let temp_file = NamedTempFile::new().unwrap();
        let db_url = format!("sqlite:{}", temp_file.path().to_string_lossy());
        DbPersistence::new(&db_url).await.unwrap()
    }

    #[tokio::test]
    async fn test_add_and_get_address() {
        let db = create_test_db().await;

        let id = db
            .add_address("qztest123".to_string(), Some("0x123".to_string()))
            .await
            .unwrap();
        assert!(id > 0);

        let addresses = db.get_all_addresses().await.unwrap();
        assert_eq!(addresses.len(), 1);
        assert_eq!(addresses[0].quan_address, "qztest123");
        assert_eq!(addresses[0].eth_address, Some("0x123".to_string()));
    }

    #[tokio::test]
    async fn test_add_and_get_task() {
        let db = create_test_db().await;

        // Add address first
        db.add_address("qztest123".to_string(), None).await.unwrap();

        // Add task
        let task = TaskRecord::new("qztest123".to_string(), 5000, "123456789012".to_string());
        let task_id = task.task_id.clone();
        db.add_task(task).await.unwrap();

        // Get task
        let found = db.get_task(&task_id).await.unwrap().unwrap();
        assert_eq!(found.quan_address, "qztest123");
        assert_eq!(found.quan_amount, 5000);
        assert!(found.usdc_amount >= 1 && found.usdc_amount <= 25);
        assert_eq!(found.task_url, "123456789012");
        assert_eq!(found.status, TaskStatus::Pending);
    }

    #[tokio::test]
    async fn test_update_task_status() {
        let db = create_test_db().await;

        // Add address and task
        db.add_address("qztest123".to_string(), None).await.unwrap();
        let task = TaskRecord::new("qztest123".to_string(), 5000, "123456789012".to_string());
        let task_id = task.task_id.clone();
        db.add_task(task).await.unwrap();

        // Update status
        db.update_task_status(&task_id, TaskStatus::Completed)
            .await
            .unwrap();

        // Verify update
        let updated = db.get_task(&task_id).await.unwrap().unwrap();
        assert_eq!(updated.status, TaskStatus::Completed);
    }

    #[tokio::test]
    async fn test_find_task_by_url() {
        let db = create_test_db().await;

        db.add_address("qztest123".to_string(), None).await.unwrap();
        let task = TaskRecord::new("qztest123".to_string(), 5000, "123456789012".to_string());
        db.add_task(task).await.unwrap();

        let found = db.find_task_by_url("123456789012").await.unwrap().unwrap();
        assert_eq!(found.quan_address, "qztest123");
        assert_eq!(found.task_url, "123456789012");

        let not_found = db.find_task_by_url("999999999999").await.unwrap();
        assert!(not_found.is_none());
    }

    #[tokio::test]
    async fn test_status_counts() {
        let db = create_test_db().await;

        db.add_address("qztest1".to_string(), None).await.unwrap();
        db.add_address("qztest2".to_string(), None).await.unwrap();

        let task1 = TaskRecord::new("qztest1".to_string(), 1000, "111111111111".to_string());
        let task2 = TaskRecord::new("qztest2".to_string(), 2000, "222222222222".to_string());

        let task1_id = task1.task_id.clone();
        db.add_task(task1).await.unwrap();
        db.add_task(task2).await.unwrap();

        // Update one task to completed
        db.update_task_status(&task1_id, TaskStatus::Completed)
            .await
            .unwrap();

        let counts = db.status_counts().await.unwrap();
        assert_eq!(*counts.get(&TaskStatus::Pending).unwrap_or(&0), 1);
        assert_eq!(*counts.get(&TaskStatus::Completed).unwrap_or(&0), 1);
    }
}
