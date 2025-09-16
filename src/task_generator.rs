use crate::csv_persistence::{CsvPersistence, TaskRecord};
use rand::prelude::*;
use std::collections::HashSet;

#[derive(Debug, thiserror::Error)]
pub enum TaskGeneratorError {
    #[error("No candidates available")]
    NoCandidates,
    #[error("Not enough candidates for selection")]
    InsufficientCandidates,
    #[error("CSV error: {0}")]
    Csv(#[from] crate::csv_persistence::CsvError),
    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),
    #[error("JSON parsing error: {0}")]
    Json(#[from] serde_json::Error),
}

pub type TaskGeneratorResult<T> = Result<T, TaskGeneratorError>;

#[derive(Debug, Clone)]
pub struct TaskGenerator {
    candidates: Vec<String>,
    csv: std::sync::Arc<CsvPersistence>,
    http_client: reqwest::Client,
}

impl TaskGenerator {
    pub fn new(csv: std::sync::Arc<CsvPersistence>) -> Self {
        Self {
            candidates: Vec::new(),
            csv,
            http_client: reqwest::Client::new(),
        }
    }

    /// Fetch candidates from GraphQL endpoint
    pub async fn refresh_candidates(&mut self, graphql_url: &str) -> TaskGeneratorResult<()> {
        tracing::info!("Refreshing candidates from: {}", graphql_url);

        // Simple GraphQL query - adjust this based on your actual schema
        let query = serde_json::json!({
            "query": "{ candidates }"
        });

        let response = self
            .http_client
            .post(graphql_url)
            .json(&query)
            .send()
            .await?;

        if !response.status().is_success() {
            tracing::error!("GraphQL request failed with status: {}", response.status());
            return Err(TaskGeneratorError::Http(reqwest::Error::from(
                response.error_for_status().unwrap_err(),
            )));
        }

        let response_json: serde_json::Value = response.json().await?;

        // Extract candidates array from GraphQL response
        let candidates = response_json
            .get("data")
            .and_then(|data| data.get("candidates"))
            .and_then(|candidates| candidates.as_array())
            .ok_or_else(|| {
                TaskGeneratorError::Json(serde_json::Error::io(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "Invalid GraphQL response format",
                )))
            })?;

        let mut new_candidates = Vec::new();
        for candidate in candidates {
            if let Some(address) = candidate.as_str() {
                // Validate that it's a proper quantus address (starts with qz)
                if address.starts_with("qz") && address.len() > 10 {
                    new_candidates.push(address.to_string());
                } else {
                    tracing::warn!("Invalid candidate address format: {}", address);
                }
            }
        }

        self.candidates = new_candidates;
        tracing::info!("Refreshed {} candidates", self.candidates.len());
        Ok(())
    }

    /// Generate tasks by randomly selecting taskees
    pub async fn generate_tasks(&self, count: usize) -> TaskGeneratorResult<Vec<TaskRecord>> {
        if self.candidates.is_empty() {
            return Err(TaskGeneratorError::NoCandidates);
        }

        if self.candidates.len() < count {
            tracing::warn!(
                "Requested {} taskees but only have {} candidates",
                count,
                self.candidates.len()
            );
        }

        let mut rng = rand::rng();
        let selection_count = count.min(self.candidates.len());

        // Randomly select unique candidates
        let selected_candidates: Vec<String> = self
            .candidates
            .choose_multiple(&mut rng, selection_count)
            .cloned()
            .collect();

        let mut tasks = Vec::new();

        for recipient in selected_candidates {
            let amount = self.generate_random_amount();
            let task_url = self.generate_task_url();

            let task = TaskRecord::new(recipient, amount, task_url);
            tasks.push(task);
        }

        tracing::info!("Generated {} new tasks", tasks.len());
        Ok(tasks)
    }

    /// Save generated tasks to CSV
    pub async fn save_tasks(&self, tasks: Vec<TaskRecord>) -> TaskGeneratorResult<()> {
        for task in tasks {
            tracing::debug!(
                "Saving task: {} -> {} (amount: {}, url: {})",
                task.task_id,
                task.recipient,
                task.amount,
                task.task_url
            );
            self.csv.add_task(task).await?;
        }
        Ok(())
    }

    /// Generate and save tasks in one operation
    pub async fn generate_and_save_tasks(
        &self,
        count: usize,
    ) -> TaskGeneratorResult<Vec<TaskRecord>> {
        let tasks = self.generate_tasks(count).await?;
        self.save_tasks(tasks.clone()).await?;
        Ok(tasks)
    }

    /// Get current candidates count
    pub fn candidates_count(&self) -> usize {
        self.candidates.len()
    }

    /// Get current candidates list (for debugging/status)
    pub fn get_candidates(&self) -> &[String] {
        &self.candidates
    }

    /// Check for duplicate task URLs to avoid collisions
    pub async fn ensure_unique_task_urls(
        &self,
        tasks: &mut [TaskRecord],
    ) -> TaskGeneratorResult<()> {
        let all_tasks = self.csv.get_all_tasks().await;
        let existing_urls: HashSet<String> = all_tasks.iter().map(|t| t.task_url.clone()).collect();

        let _rng = rand::rng();

        for task in tasks {
            while existing_urls.contains(&task.task_url) {
                tracing::warn!(
                    "Task URL collision detected, regenerating: {}",
                    task.task_url
                );
                task.task_url = self.generate_task_url();
            }
        }

        Ok(())
    }

    fn generate_random_amount(&self) -> u64 {
        let mut rng = rand::rng();
        rng.random_range(1000..=9999)
    }

    fn generate_task_url(&self) -> String {
        let mut rng = rand::rng();
        // Generate 12 digit random number
        let task_url: u64 = rng.random_range(100_000_000_000..=999_999_999_999);
        task_url.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::csv_persistence::CsvPersistence;
    use std::sync::Arc;
    use tempfile::NamedTempFile;

    #[test]
    fn test_generate_random_amount() {
        let temp_file = NamedTempFile::new().unwrap();
        let csv = Arc::new(CsvPersistence::new(temp_file.path()));
        let generator = TaskGenerator::new(csv);

        for _ in 0..100 {
            let amount = generator.generate_random_amount();
            assert!(amount >= 1000 && amount <= 9999);
        }
    }

    #[test]
    fn test_generate_task_url() {
        let temp_file = NamedTempFile::new().unwrap();
        let csv = Arc::new(CsvPersistence::new(temp_file.path()));
        let generator = TaskGenerator::new(csv);

        for _ in 0..100 {
            let task_url = generator.generate_task_url();
            assert_eq!(task_url.len(), 12);
            assert!(task_url.chars().all(|c| c.is_ascii_digit()));
        }
    }

    #[tokio::test]
    async fn test_generate_tasks() {
        let temp_file = NamedTempFile::new().unwrap();
        let csv = Arc::new(CsvPersistence::new(temp_file.path()));
        let mut generator = TaskGenerator::new(csv);

        // Add some test candidates
        generator.candidates = vec![
            "qztest1".to_string(),
            "qztest2".to_string(),
            "qztest3".to_string(),
        ];

        let tasks = generator.generate_tasks(2).await.unwrap();
        assert_eq!(tasks.len(), 2);

        for task in &tasks {
            assert!(task.recipient.starts_with("qztest"));
            assert!(task.amount >= 1000 && task.amount <= 9999);
            assert_eq!(task.task_url.len(), 12);
            assert!(task.task_url.chars().all(|c| c.is_ascii_digit()));
        }

        // Test requesting more tasks than candidates
        let tasks = generator.generate_tasks(5).await.unwrap();
        assert_eq!(tasks.len(), 3); // Should cap at number of candidates
    }

    #[tokio::test]
    async fn test_no_candidates() {
        let temp_file = NamedTempFile::new().unwrap();
        let csv = Arc::new(CsvPersistence::new(temp_file.path()));
        let generator = TaskGenerator::new(csv);

        let result = generator.generate_tasks(1).await;
        assert!(matches!(result, Err(TaskGeneratorError::NoCandidates)));
    }
}
