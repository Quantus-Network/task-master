use crate::{
    db_persistence::{DbError, DbPersistence},
    models::{
        address::{Address, AddressInput, QuanAddress},
        task::{Task, TaskInput},
    },
    utils::generate_referral_code::generate_referral_code,
};
use rand::prelude::*;

#[derive(Debug, thiserror::Error)]
pub enum TaskGeneratorError {
    #[error("Task input data contain one of more invalid value")]
    ValidationError,
    #[error("No candidates available")]
    NoCandidates,
    #[error("CSV error: {0}")]
    Database(#[from] DbError),
    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),
    #[error("JSON parsing error: {0}")]
    Json(#[from] serde_json::Error),
}

pub type TaskGeneratorResult<T> = Result<T, TaskGeneratorError>;

#[derive(Debug, Clone)]
pub struct TaskGenerator {
    candidates: Vec<String>,
    db: std::sync::Arc<DbPersistence>,
    http_client: reqwest::Client,
}

impl TaskGenerator {
    pub fn new(db: std::sync::Arc<DbPersistence>) -> Self {
        Self {
            candidates: Vec::new(),
            db,
            http_client: reqwest::Client::new(),
        }
    }

    /// Fetch candidates from GraphQL endpoint
    pub async fn refresh_candidates(&mut self, graphql_url: &str) -> TaskGeneratorResult<()> {
        tracing::info!("Refreshing candidates from: {}", graphql_url);

        // Simple GraphQL query - adjust this based on your actual schema
        let query = serde_json::json!({
            "query": "{   
                accounts {
                    id
                } 
            }"});

        let response = self.http_client.post(graphql_url).json(&query).send().await?;

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
            .and_then(|data| data.get("accounts"))
            .and_then(|accounts| accounts.as_array())
            .ok_or_else(|| {
                TaskGeneratorError::Json(serde_json::Error::io(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "Invalid GraphQL response format",
                )))
            })?;

        let mut new_candidates = Vec::new();
        for candidate in candidates {
            if let Some(address) = candidate.get("id").and_then(|id| id.as_str()) {
                // Validate that it's a proper quantus address (starts with qz)
                if let Ok(valid_address) = QuanAddress::from(address) {
                    new_candidates.push(valid_address.0.to_string());
                } else {
                    tracing::warn!("Invalid candidate address format: {}", address);
                }
            }
        }

        self.candidates = new_candidates;
        tracing::info!("Refreshed {} candidates", self.candidates.len());
        Ok(())
    }

    /// Refresh candidates from local database addresses
    pub async fn refresh_candidates_from_db(&mut self) -> TaskGeneratorResult<()> {
        tracing::info!("Refreshing candidates from local database");

        let addresses = self.db.addresses.find_all().await?;

        let mut new_candidates = Vec::new();
        for address in addresses {
            // Validate that it's a proper quantus address (starts with qz)

            new_candidates.push(address.quan_address.0);
        }

        self.candidates = new_candidates;
        tracing::info!("Refreshed {} candidates from database", self.candidates.len());
        Ok(())
    }

    /// Generate tasks by randomly selecting taskees
    pub async fn generate_tasks(&self, count: usize) -> TaskGeneratorResult<Vec<Task>> {
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

        for quan_address in selected_candidates {
            let quan_amount = self.generate_random_quan_amount();
            let task_url = self.generate_task_url();
            let task_input = TaskInput {
                quan_address,
                quan_amount,
                task_url,
            };

            if let Ok(task) = Task::new(task_input) {
                tasks.push(task);
            } else {
                return Err(TaskGeneratorError::ValidationError);
            };
        }

        tracing::info!("Generated {} new tasks", tasks.len());
        Ok(tasks)
    }

    /// Save generated tasks to database
    pub async fn save_tasks(&self, tasks: Vec<Task>) -> TaskGeneratorResult<()> {
        for task in tasks {
            tracing::debug!(
                "Saving task: {} -> {} (quan_amount: {}, usdc_amount: {}, url: {})",
                task.task_id,
                task.quan_address.0,
                task.quan_amount.0,
                task.usdc_amount,
                task.task_url
            );

            if let Ok(referral_code) = generate_referral_code(task.quan_address.0.clone()).await {
                if let Ok(address) = Address::new(AddressInput {
                    quan_address: task.quan_address.0.clone(),
                    referral_code,
                }) {
                    // Ensure address exists in database
                    self.db.addresses.create(&address).await?;
                    self.db.tasks.create(&task).await?;
                } else {
                    return Err(TaskGeneratorError::ValidationError);
                }
            }
        }
        Ok(())
    }

    /// Generate and save tasks in one operation
    pub async fn generate_and_save_tasks(&self, count: usize) -> TaskGeneratorResult<Vec<Task>> {
        let mut tasks = self.generate_tasks(count).await?;
        self.ensure_unique_task_urls(&mut tasks).await?;
        self.save_tasks(tasks.clone()).await?;
        Ok(tasks)
    }

    /// Get current candidates count
    pub fn candidates_count(&self) -> usize {
        self.candidates.len()
    }

    /// Check for duplicate task URLs to avoid collisions
    pub async fn ensure_unique_task_urls(&self, tasks: &mut [Task]) -> TaskGeneratorResult<()> {
        for task in tasks {
            // Keep checking if URL exists and regenerate if needed
            while let Some(_existing_task) = self.db.tasks.find_task_by_url(&task.task_url).await? {
                tracing::warn!("Task URL collision detected, regenerating: {}", task.task_url);
                task.task_url = self.generate_task_url();
            }
        }

        Ok(())
    }

    fn generate_random_quan_amount(&self) -> u64 {
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
    use crate::config::Config;
    use crate::utils::test_db::reset_database;
    use std::sync::Arc;
    use wiremock::{matchers::method, Mock, MockServer, ResponseTemplate};

    // Helper to set up a test generator with a real PostgreSQL test database.
    async fn setup_test_generator() -> TaskGenerator {
        let config = Config::load_test_env().expect("Failed to load test configuration");
        let db = Arc::new(DbPersistence::new(config.get_database_url()).await.unwrap());

        reset_database(&db.pool).await;

        TaskGenerator::new(db)
    }

    /// Get current candidates list (for debugging/status)
    fn get_candidates(task_generator: &TaskGenerator) -> &[String] {
        &task_generator.candidates
    }

    #[tokio::test]
    async fn test_generate_random_quan_amount() {
        let generator = setup_test_generator().await;
        for _ in 0..100 {
            let amount = generator.generate_random_quan_amount();
            assert!((1000..=9999).contains(&amount));
        }
    }

    #[tokio::test]
    async fn test_generate_task_url() {
        let generator = setup_test_generator().await;
        for _ in 0..100 {
            let url = generator.generate_task_url();
            assert_eq!(url.len(), 12);
            assert!(url.chars().all(|c| c.is_ascii_digit()));
        }
    }

    #[tokio::test]
    async fn test_refresh_candidates_from_db() {
        let mut generator = setup_test_generator().await;

        // Create and save some addresses to the DB.
        // The dummy addresses must be > 10 characters to pass validation.
        let addr1 = Address::new(AddressInput {
            quan_address: "qz_a_valid_test_address_1".to_string(),
            referral_code: "REF1".to_string(),
        })
        .unwrap();
        let addr2 = Address::new(AddressInput {
            quan_address: "qz_a_valid_test_address_2".to_string(),
            referral_code: "REF2".to_string(),
        })
        .unwrap();
        generator.db.addresses.create(&addr1).await.unwrap();
        generator.db.addresses.create(&addr2).await.unwrap();

        // Refresh candidates from the database.
        generator.refresh_candidates_from_db().await.unwrap();

        assert_eq!(generator.candidates_count(), 2);
        assert!(get_candidates(&generator).contains(&addr1.quan_address.0));
        assert!(get_candidates(&generator).contains(&addr2.quan_address.0));
    }

    #[tokio::test]
    async fn test_refresh_candidates_with_mock_server() {
        // Start a mock server.
        let server = MockServer::start().await;
        let mut generator = setup_test_generator().await;

        // Create a mock GraphQL response.
        let mock_response = serde_json::json!({
            "data": {
                "accounts": [
                    { "id": "qz_a_valid_test_address_1" },
                    { "id": "invalid_addr" }, // Should be filtered out
                    { "id": "qz_a_valid_test_address_2" }
                ]
            }
        });
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(200).set_body_json(mock_response))
            .mount(&server)
            .await;

        // Call the function with the mock server's URI.
        generator.refresh_candidates(&server.uri()).await.unwrap();

        // Assert that only valid candidates were added.
        assert_eq!(generator.candidates_count(), 2);
        assert!(get_candidates(&generator).contains(&"qz_a_valid_test_address_1".to_string()));
        assert!(get_candidates(&generator).contains(&"qz_a_valid_test_address_2".to_string()));
    }

    #[tokio::test]
    async fn test_generate_tasks_and_save() {
        let mut generator = setup_test_generator().await;

        // Populate candidates manually for the test.
        generator.candidates = vec![
            "qz_a_valid_test_address_1".to_string(),
            "qz_a_valid_test_address_2".to_string(),
            "qz_a_valid_test_address_3".to_string(),
        ];

        // Generate and save 2 tasks.
        let tasks = generator.generate_and_save_tasks(2).await.unwrap();
        assert_eq!(tasks.len(), 2);

        // Verify the state after the first call.
        let db_tasks = generator.db.tasks.get_all_tasks().await.unwrap();
        assert_eq!(db_tasks.len(), 2);

        // Generate and save 3 more tasks (capped by the 3 candidates).
        generator.generate_and_save_tasks(5).await.unwrap();
        let db_tasks_total = generator.db.tasks.get_all_tasks().await.unwrap();

        // The database now contains the original 2 tasks PLUS the 3 new ones.
        // The total should be 5.
        assert_eq!(db_tasks_total.len(), 5);
    }

    #[tokio::test]
    async fn test_no_candidates_error() {
        let generator = setup_test_generator().await; // Candidates list is empty.
        let result = generator.generate_tasks(1).await;
        assert!(matches!(result, Err(TaskGeneratorError::NoCandidates)));
    }
}
