use chrono::{DateTime, Utc};
use rand::Rng;
use serde::{Deserialize, Serialize};
use sqlx::{postgres::PgRow, FromRow, Row};
use uuid::Uuid;

use crate::models::{address::QuanAddress, ModelError, ModelResult};

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

#[derive(Debug, Deserialize, Serialize, Clone, sqlx::Type)]
#[sqlx(transparent)]
pub struct TokenAmount(pub i64);
impl TokenAmount {
    pub fn from(input: i64) -> Result<Self, String> {
        if input <= 0 {
            return Err(String::from("Token amount can't be less or equal 0."));
        }

        Ok(TokenAmount(input))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    pub id: Option<i32>,
    pub task_id: String,
    pub quan_address: QuanAddress,
    pub quan_amount: TokenAmount,
    pub usdc_amount: i64,
    pub task_url: String,
    pub status: TaskStatus,
    pub reversible_tx_id: Option<String>,
    pub send_time: Option<DateTime<Utc>>,
    pub end_time: Option<DateTime<Utc>>,
    pub created_at: Option<DateTime<Utc>>,
    pub updated_at: Option<DateTime<Utc>>,
}

impl Task {
    pub fn new(input: TaskInput) -> ModelResult<Self> {
        let quan_address = match QuanAddress::from(&input.quan_address) {
            Ok(name) => name,
            Err(e) => {
                tracing::error!(error = %e, "Invalid quan address input for task");
                return Err(ModelError::InvalidInput);
            }
        };

        let quan_amount = match TokenAmount::from(input.quan_amount as i64) {
            Ok(quan_amount) => quan_amount,
            Err(e) => {
                tracing::error!(error = %e, "Invalid token amount input for task");
                return Err(ModelError::InvalidInput);
            }
        };

        let task_url = input.task_url;

        let mut rng = rand::rng();
        let usdc_amount = rng.random_range(1..=25);
        let task_id = Uuid::new_v4().to_string();

        Ok(Task {
            id: None,
            task_id,
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
        })
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
impl<'r> FromRow<'r, PgRow> for Task {
    fn from_row(row: &'r PgRow) -> Result<Self, sqlx::Error> {
        let status_str: String = row.try_get("status")?;
        let status = status_str.parse::<TaskStatus>().map_err(|e| {
            sqlx::Error::Decode(Box::new(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                e,
            )))
        })?;

        Ok(Task {
            id: row.try_get("id")?,
            task_id: row.try_get("task_id")?,
            quan_address: row.try_get("quan_address")?,
            quan_amount: row.try_get("quan_amount")?,
            usdc_amount: row.try_get("usdc_amount")?,
            task_url: row.try_get("task_url")?,
            status,
            reversible_tx_id: row.try_get("reversible_tx_id")?,
            send_time: row.try_get("send_time")?,
            end_time: row.try_get("end_time")?,
            created_at: row.try_get("created_at")?,
            updated_at: row.try_get("updated_at")?,
        })
    }
}

// An unvalidated version that we can deserialize directly from JSON
#[derive(Debug, Deserialize)]
pub struct TaskInput {
    pub quan_address: String,
    pub quan_amount: u64,
    pub task_url: String,
}

#[derive(Debug, Deserialize)]
pub struct CompleteTaskRequest {
    pub task_url: String,
}

#[derive(Debug, Serialize)]
pub struct CompleteTaskResponse {
    pub success: bool,
    pub message: String,
    pub task_id: Option<String>,
}