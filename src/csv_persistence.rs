use chrono::{DateTime, Utc};
use csv::{ReaderBuilder, WriterBuilder};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs::{File, OpenOptions};
use std::path::Path;
use tokio::sync::RwLock;
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskRecord {
    pub task_id: String,
    pub recipient: String,
    pub amount: u64,
    #[serde(with = "chrono::serde::ts_seconds_option")]
    pub send_time: Option<DateTime<Utc>>,
    #[serde(with = "chrono::serde::ts_seconds_option")]
    pub end_time: Option<DateTime<Utc>>,
    pub task_url: String,
    pub eth_address: String,
    pub status: TaskStatus,
    pub tx_hash: String,
}

impl TaskRecord {
    pub fn new(recipient: String, amount: u64, task_url: String) -> Self {
        Self {
            task_id: Uuid::new_v4().to_string(),
            recipient,
            amount,
            send_time: None,
            end_time: None,
            task_url,
            eth_address: String::new(),
            status: TaskStatus::Pending,
            tx_hash: String::new(),
        }
    }

    pub fn set_transaction_sent(
        &mut self,
        tx_hash: String,
        send_time: DateTime<Utc>,
        end_time: DateTime<Utc>,
    ) {
        self.tx_hash = tx_hash;
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
pub enum CsvError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("CSV error: {0}")]
    Csv(#[from] csv::Error),
    #[error("Task not found: {0}")]
    TaskNotFound(String),
    #[error("Invalid task status: {0}")]
    InvalidStatus(String),
}

pub type CsvResult<T> = Result<T, CsvError>;

#[derive(Debug)]
pub struct CsvPersistence {
    file_path: std::path::PathBuf,
    tasks: RwLock<HashMap<String, TaskRecord>>,
}

impl CsvPersistence {
    pub fn new<P: AsRef<Path>>(file_path: P) -> Self {
        Self {
            file_path: file_path.as_ref().to_path_buf(),
            tasks: RwLock::new(HashMap::new()),
        }
    }

    pub async fn load(&self) -> CsvResult<()> {
        if !self.file_path.exists() {
            tracing::info!("CSV file does not exist, starting with empty task list");
            return Ok(());
        }

        let file = File::open(&self.file_path)?;
        let mut reader = ReaderBuilder::new().has_headers(true).from_reader(file);

        let mut tasks = HashMap::new();
        for result in reader.deserialize() {
            let record: TaskRecord = result?;
            tasks.insert(record.task_id.clone(), record);
        }

        let mut task_map = self.tasks.write().await;
        *task_map = tasks;

        tracing::info!("Loaded {} tasks from CSV", task_map.len());
        Ok(())
    }

    pub async fn save(&self) -> CsvResult<()> {
        let tasks = self.tasks.read().await;

        // Create parent directory if it doesn't exist
        if let Some(parent) = self.file_path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let file = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(&self.file_path)?;

        let mut writer = WriterBuilder::new().has_headers(true).from_writer(file);

        for task in tasks.values() {
            writer.serialize(task)?;
        }

        writer.flush()?;
        tracing::debug!("Saved {} tasks to CSV", tasks.len());
        Ok(())
    }

    pub async fn add_task(&self, task: TaskRecord) -> CsvResult<()> {
        let mut tasks = self.tasks.write().await;
        tasks.insert(task.task_id.clone(), task);
        drop(tasks);

        self.save().await?;
        Ok(())
    }

    pub async fn update_task<F>(&self, task_id: &str, update_fn: F) -> CsvResult<()>
    where
        F: FnOnce(&mut TaskRecord),
    {
        let mut tasks = self.tasks.write().await;
        let task = tasks
            .get_mut(task_id)
            .ok_or_else(|| CsvError::TaskNotFound(task_id.to_string()))?;

        update_fn(task);
        drop(tasks);

        self.save().await?;
        Ok(())
    }

    pub async fn get_task(&self, task_id: &str) -> Option<TaskRecord> {
        let tasks = self.tasks.read().await;
        tasks.get(task_id).cloned()
    }

    pub async fn find_task_by_url(&self, task_url: &str) -> Option<TaskRecord> {
        let tasks = self.tasks.read().await;
        tasks
            .values()
            .find(|task| task.task_url == task_url)
            .cloned()
    }

    pub async fn get_tasks_by_status(&self, status: TaskStatus) -> Vec<TaskRecord> {
        let tasks = self.tasks.read().await;
        tasks
            .values()
            .filter(|task| task.status == status)
            .cloned()
            .collect()
    }

    pub async fn get_tasks_ready_for_reversal(&self, early_minutes: i64) -> Vec<TaskRecord> {
        let tasks = self.tasks.read().await;
        tasks
            .values()
            .filter(|task| task.is_ready_for_reversal(early_minutes))
            .cloned()
            .collect()
    }

    pub async fn get_all_tasks(&self) -> Vec<TaskRecord> {
        let tasks = self.tasks.read().await;
        tasks.values().cloned().collect()
    }

    pub async fn task_count(&self) -> usize {
        let tasks = self.tasks.read().await;
        tasks.len()
    }

    pub async fn status_counts(&self) -> HashMap<TaskStatus, usize> {
        let tasks = self.tasks.read().await;
        let mut counts = HashMap::new();

        for task in tasks.values() {
            *counts.entry(task.status.clone()).or_insert(0) += 1;
        }

        counts
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;

    #[tokio::test]
    async fn test_csv_persistence() {
        let temp_file = NamedTempFile::new().unwrap();
        let csv = CsvPersistence::new(temp_file.path());

        // Test loading empty file
        csv.load().await.unwrap();
        assert_eq!(csv.task_count().await, 0);

        // Test adding task
        let task = TaskRecord::new("qztest123".to_string(), 5000, "123456789012".to_string());
        let task_id = task.task_id.clone();
        csv.add_task(task).await.unwrap();

        assert_eq!(csv.task_count().await, 1);

        // Test finding task
        let found = csv.get_task(&task_id).await.unwrap();
        assert_eq!(found.recipient, "qztest123");
        assert_eq!(found.amount, 5000);
        assert_eq!(found.task_url, "123456789012");

        // Test updating task
        csv.update_task(&task_id, |task| {
            task.mark_completed();
        })
        .await
        .unwrap();

        let updated = csv.get_task(&task_id).await.unwrap();
        assert_eq!(updated.status, TaskStatus::Completed);

        // Test reload from file
        let csv2 = CsvPersistence::new(temp_file.path());
        csv2.load().await.unwrap();
        assert_eq!(csv2.task_count().await, 1);

        let reloaded = csv2.get_task(&task_id).await.unwrap();
        assert_eq!(reloaded.status, TaskStatus::Completed);
    }

    #[test]
    fn test_task_ready_for_reversal() {
        let mut task = TaskRecord::new("qztest".to_string(), 1000, "123456789012".to_string());

        // Not ready when no end_time set
        assert!(!task.is_ready_for_reversal(2));

        // Set end time to 1 minute from now
        let now = Utc::now();
        let end_time = now + chrono::Duration::minutes(1);
        task.set_transaction_sent("0x123".to_string(), now, end_time);

        // Should be ready for reversal (1 minute left < 2 minute early warning)
        assert!(task.is_ready_for_reversal(2));

        // Should not be ready with shorter early warning
        assert!(!task.is_ready_for_reversal(0));

        // Should not be ready if completed
        task.mark_completed();
        assert!(!task.is_ready_for_reversal(2));
    }
}
