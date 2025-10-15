use std::{collections::HashMap, str::FromStr};

use chrono::{DateTime, Utc};
use sqlx::{PgPool, Row};

use crate::{
    db_persistence::DbError,
    models::task::{Task, TaskStatus},
    repositories::DbResult,
};

#[derive(Clone, Debug)]
pub struct TaskRepository {
    pool: PgPool,
}
impl TaskRepository {
    pub fn new(pool: &PgPool) -> Self {
        Self { pool: pool.clone() }
    }

    pub async fn create(&self, new_task: &Task) -> DbResult<String> {
        let created_task_id = sqlx::query_scalar::<_, String>(
            "
            INSERT INTO tasks (
                task_id, quan_address, quan_amount, usdc_amount, task_url,
                status, reversible_tx_id, send_time, end_time
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
            RETURNING task_id
            ",
        )
        .bind(&new_task.task_id)
        .bind(&new_task.quan_address.0)
        .bind(new_task.quan_amount.0)
        .bind(new_task.usdc_amount)
        .bind(&new_task.task_url)
        .bind(new_task.status.to_string())
        .bind(&new_task.reversible_tx_id)
        .bind(new_task.send_time)
        .bind(new_task.end_time)
        .fetch_one(&self.pool)
        .await?;

        Ok(created_task_id)
    }

    pub async fn get_task(&self, task_id: &str) -> DbResult<Option<Task>> {
        let task = sqlx::query_as::<_, Task>("SELECT * FROM tasks WHERE task_id = $1")
            .bind(task_id)
            .fetch_optional(&self.pool)
            .await?;
        Ok(task)
    }

    pub async fn find_task_by_url(&self, task_url: &str) -> DbResult<Option<Task>> {
        let task = sqlx::query_as::<_, Task>("SELECT * FROM tasks WHERE task_url = $1")
            .bind(task_url)
            .fetch_optional(&self.pool)
            .await?;
        Ok(task)
    }

    pub async fn update_task_status(&self, task_id: &str, status: TaskStatus) -> DbResult<()> {
        let result =
            sqlx::query("UPDATE tasks SET status = $1, updated_at = NOW() WHERE task_id = $2")
                .bind(status.to_string())
                .bind(task_id)
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
        reversible_tx_id: &str,
        send_time: DateTime<Utc>,
        end_time: DateTime<Utc>,
    ) -> DbResult<()> {
        let result = sqlx::query(
            "
            UPDATE tasks
            SET reversible_tx_id = $1, send_time = $2, end_time = $3,
                status = $4, updated_at = NOW()
            WHERE task_id = $5
            ",
        )
        .bind(reversible_tx_id)
        .bind(send_time)
        .bind(end_time)
        .bind(TaskStatus::Pending.to_string()) // Assuming you want to set it to pending
        .bind(task_id)
        .execute(&self.pool)
        .await?;

        if result.rows_affected() == 0 {
            return Err(DbError::TaskNotFound(task_id.to_string()));
        }

        Ok(())
    }

    pub async fn get_tasks_by_status(&self, status: TaskStatus) -> DbResult<Vec<Task>> {
        let tasks =
            sqlx::query_as::<_, Task>("SELECT * FROM tasks WHERE status = $1 ORDER BY created_at")
                .bind(status.to_string())
                .fetch_all(&self.pool)
                .await?;
        Ok(tasks)
    }

    pub async fn get_tasks_ready_for_reversal(&self, early_minutes: i64) -> DbResult<Vec<Task>> {
        let cutoff_time = Utc::now() + chrono::Duration::minutes(early_minutes);

        let tasks = sqlx::query_as::<_, Task>(
            "SELECT * FROM tasks WHERE status = $1 AND end_time IS NOT NULL AND end_time <= $2",
        )
        .bind(TaskStatus::Pending.to_string())
        .bind(cutoff_time)
        .fetch_all(&self.pool)
        .await?;
        Ok(tasks)
    }

    pub async fn get_all_tasks(&self) -> DbResult<Vec<Task>> {
        let tasks = sqlx::query_as::<_, Task>("SELECT * FROM tasks ORDER BY created_at DESC")
            .fetch_all(&self.pool)
            .await?;
        Ok(tasks)
    }

    pub async fn task_count(&self) -> DbResult<i64> {
        let count = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM tasks")
            .fetch_one(&self.pool)
            .await?;
        Ok(count)
    }

    pub async fn status_counts(&self) -> DbResult<HashMap<TaskStatus, usize>> {
        let rows = sqlx::query("SELECT status, COUNT(*) as count FROM tasks GROUP BY status")
            .fetch_all(&self.pool)
            .await?;

        let mut counts = HashMap::new();
        for row in rows {
            let status_str: String = row.get("status");
            let count: i64 = row.get("count");

            if let Ok(status) = TaskStatus::from_str(&status_str) {
                counts.insert(status, count as usize);
            }
        }

        Ok(counts)
    }

    pub async fn get_address_stats(&self) -> DbResult<Vec<(String, i64)>> {
        let stats = sqlx::query_as::<_, (String, i64)>(
            r#"
        SELECT
            a.quan_address,
            COUNT(t.id) as task_count
        FROM
            addresses a
        LEFT JOIN
            tasks t ON a.quan_address = t.quan_address
        GROUP BY
            a.quan_address
        ORDER BY
            a.quan_address
        "#,
        )
        .fetch_all(&self.pool)
        .await?;

        Ok(stats)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        config::Config,
        models::{
            address::{Address, AddressInput},
            task::TaskInput,
        },
        repositories::address::AddressRepository,
        utils::test_db::reset_database,
    };
    use uuid::Uuid;

    // Helper to set up repositories and clean all tables.
    async fn setup_test_repositories() -> (AddressRepository, TaskRepository) {
        let config = Config::load_test_env().expect("Failed to load configuration for tests");
        let pool = PgPool::connect(config.get_database_url())
            .await
            .expect("Failed to create pool.");

        reset_database(&pool).await;

        (AddressRepository::new(&pool), TaskRepository::new(&pool))
    }

    // Helper to create a persisted address.
    async fn create_persisted_address(repo: &AddressRepository, id: &str) -> Address {
        let input = AddressInput {
            quan_address: format!("qz_test_address_{}", id),
            eth_address: None,
            referral_code: format!("REF{}", id),
        };
        let address = Address::new(input).unwrap();
        repo.create(&address).await.unwrap();
        address
    }

    // Helper to create a mock Task object.
    fn create_mock_task_object(quan_address: &str) -> Task {
        let input = TaskInput {
            quan_address: quan_address.to_string(),
            quan_amount: 1000,
            task_url: format!("http://example.com/task/{}", Uuid::new_v4()),
        };
        Task::new(input).unwrap()
    }

    #[tokio::test]
    async fn test_create_and_get_task() {
        let (address_repo, task_repo) = setup_test_repositories().await;
        let address = create_persisted_address(&address_repo, "001").await;
        let new_task = create_mock_task_object(&address.quan_address.0);

        let created_id = task_repo.create(&new_task).await.unwrap();
        assert_eq!(created_id, new_task.task_id);

        let fetched_task = task_repo.get_task(&created_id).await.unwrap().unwrap();
        assert_eq!(fetched_task.task_id, new_task.task_id);
        assert_eq!(fetched_task.status, TaskStatus::Pending);
    }

    #[tokio::test]
    async fn test_update_task_status() {
        let (address_repo, task_repo) = setup_test_repositories().await;
        let address = create_persisted_address(&address_repo, "002").await;
        let new_task = create_mock_task_object(&address.quan_address.0);
        task_repo.create(&new_task).await.unwrap();

        task_repo
            .update_task_status(&new_task.task_id, TaskStatus::Completed)
            .await
            .unwrap();

        let fetched_task = task_repo
            .get_task(&new_task.task_id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(fetched_task.status, TaskStatus::Completed);
    }

    #[tokio::test]
    async fn test_update_task_transaction() {
        let (address_repo, task_repo) = setup_test_repositories().await;
        let address = create_persisted_address(&address_repo, "003").await;
        let new_task = create_mock_task_object(&address.quan_address.0);
        task_repo.create(&new_task).await.unwrap();

        let tx_id = "0x123abc";
        let send_time = Utc::now();
        let end_time = send_time + chrono::Duration::hours(1);

        task_repo
            .update_task_transaction(&new_task.task_id, tx_id, send_time, end_time)
            .await
            .unwrap();

        let updated_task = task_repo
            .get_task(&new_task.task_id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(updated_task.reversible_tx_id, Some(tx_id.to_string()));
        assert!(updated_task.send_time.is_some());
        assert!(updated_task.end_time.is_some());
    }

    #[tokio::test]
    async fn test_get_tasks_by_status() {
        let (address_repo, task_repo) = setup_test_repositories().await;
        let address = create_persisted_address(&address_repo, "004").await;
        
        let mut task1 = create_mock_task_object(&address.quan_address.0);
        task1.status = TaskStatus::Pending;
        task_repo.create(&task1).await.unwrap();

        let mut task2 = create_mock_task_object(&address.quan_address.0);
        task2.status = TaskStatus::Completed;
        task_repo.create(&task2).await.unwrap();
        
        let pending_tasks = task_repo.get_tasks_by_status(TaskStatus::Pending).await.unwrap();
        assert_eq!(pending_tasks.len(), 1);
        assert_eq!(pending_tasks[0].task_id, task1.task_id);
    }

    #[tokio::test]
    async fn test_get_tasks_ready_for_reversal() {
        let (address_repo, task_repo) = setup_test_repositories().await;
        let address = create_persisted_address(&address_repo, "005").await;

        // This task's end time is soon, so it should be picked up
        let mut task1 = create_mock_task_object(&address.quan_address.0);
        task_repo.create(&task1).await.unwrap();
        let end_time1 = Utc::now() + chrono::Duration::minutes(5);
        task_repo.update_task_transaction(&task1.task_id, "tx1", Utc::now(), end_time1).await.unwrap();
        
        // This task's end time is far in the future
        let mut task2 = create_mock_task_object(&address.quan_address.0);
        task_repo.create(&task2).await.unwrap();
        let end_time2 = Utc::now() + chrono::Duration::minutes(30);
        task_repo.update_task_transaction(&task2.task_id, "tx2", Utc::now(), end_time2).await.unwrap();

        // Looking for tasks ending within the next 10 minutes
        let reversible_tasks = task_repo.get_tasks_ready_for_reversal(10).await.unwrap();
        assert_eq!(reversible_tasks.len(), 1);
        assert_eq!(reversible_tasks[0].task_id, task1.task_id);
    }

    #[tokio::test]
    async fn test_counts() {
        let (address_repo, task_repo) = setup_test_repositories().await;
        let address = create_persisted_address(&address_repo, "006").await;
        
        let mut task1 = create_mock_task_object(&address.quan_address.0);
        task1.status = TaskStatus::Pending;
        task_repo.create(&task1).await.unwrap();

        let mut task2 = create_mock_task_object(&address.quan_address.0);
        task2.status = TaskStatus::Pending;
        task_repo.create(&task2).await.unwrap();

        let mut task3 = create_mock_task_object(&address.quan_address.0);
        task3.status = TaskStatus::Completed;
        task_repo.create(&task3).await.unwrap();

        // Test total count
        let total = task_repo.task_count().await.unwrap();
        assert_eq!(total, 3);
        
        // Test status counts
        let counts = task_repo.status_counts().await.unwrap();
        assert_eq!(counts.get(&TaskStatus::Pending), Some(&2));
        assert_eq!(counts.get(&TaskStatus::Completed), Some(&1));
        assert_eq!(counts.get(&TaskStatus::Failed), None);
    }
}