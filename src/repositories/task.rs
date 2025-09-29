use std::{collections::HashMap, str::FromStr};

use chrono::{DateTime, Utc};
use sqlx::{PgPool, Row};

use crate::{
    models::task::{Task, TaskStatus},
    repositories::DbResult, db_persistence::DbError,
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
            COUNT(t.id)::BIGINT as task_count
        FROM
            address a
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
