use chrono::{DateTime, Utc};
use quantus_cli::cli::common::resolve_address;
use serde::{Deserialize, Serialize};
use sqlx::{postgres::PgRow, FromRow, Row};

use crate::{
    errors::{AppError, ValidationErrors},
    models::address::QuanAddress,
    utils::eth_address_validator::is_valid_eth_address,
};

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
pub struct TaskId(pub String);
impl TaskId {
    pub fn from(input: &str) -> Result<Self, String> {
        if input.is_empty() {
            return Err(String::from("Task ID shouldn't be empty."));
        }

        Ok(TaskId(val.to_string()))
    }
}

#[derive(Debug, Deserialize, Serialize, Clone, sqlx::Type)]
#[sqlx(transparent)]
pub struct TokenAmount(pub i64);
impl TokenAmount {
    pub fn from(input: i64) -> Result<Self, i64> {
        if input <= 0 {
            return Err(String::from("Token amount can't be less or equal 0."));
        }

        Ok(TokenAmount(input))
    }
}

#[derive(Debug, Deserialize, Serialize, Clone, sqlx::Type)]
#[sqlx(transparent)]
pub struct TaskUrl(pub String);
impl TaskUrl {
    pub fn from(input: &str) -> Result<Self, String> {
        if input.is_empty() {
            return Err(String::from("Task url shouldn't be empty."));
        }

        Ok(TaskUrl(input.to_string()))
    }
}

#[derive(Debug, Deserialize, Serialize, Clone, sqlx::Type)]
#[sqlx(transparent)]
pub struct TxId(pub String);
impl TxId {
    pub fn from(input: &str) -> Result<Self, String> {
        if input.is_empty() {
            return Err(String::from("Tx id shouldn't be empty."));
        }

        Ok(TxId(input.to_string()))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    pub id: Option<i64>,
    pub task_id: TaskId,
    pub quan_address: QuanAddress,
    pub quan_amount: TokenAmount,
    pub usdc_amount: TokenAmount,
    pub task_url: TaskUrl,
    pub status: TaskStatus,
    pub reversible_tx_id: Option<TxId>,
    pub send_time: Option<DateTime<Utc>>,
    pub end_time: Option<DateTime<Utc>>,
    pub created_at: Option<DateTime<Utc>>,
    pub updated_at: Option<DateTime<Utc>>,
}

impl Task {
    pub fn new(input: TaskInput) -> Result<Self, AppError> {
        let mut errors = ValidationErrors::new();

        let task_id = match TaskId::from(&input.task_id) {
            Ok(task_id) => task_id,
            Err(e) => {
                errors.add("eth_address", e.to_string());
                TaskId("".to_string())
            }
        };

        let quan_address = match QuanAddress::from(&input.quan_address) {
            Ok(name) => name,
            Err(e) => {
                errors.add("quan_address", e.to_string());
                QuanAddress("".to_string())
            }
        };

        let quan_amount = match TokenAmount::from(input.quan_amount) {
            Ok(quan_amount) => quan_amount,
            Err(e) => {
                errors.add("quan_amount", e.to_string());
                TokenAmount(0)
            }
        };

        let usdc_amount = match TokenAmount::from(input.usdc_amount) {
            Ok(usdc_amount) => usdc_amount,
            Err(e) => {
                errors.add("usdc_amount", e.to_string());
                TokenAmount(0)
            }
        };

        let task_url = match TaskUrl::from(&input.task_url) {
            Ok(task_url) => task_url,
            Err(e) => {
                errors.add("task_url", e.to_string());
                TaskId("".to_string())
            }
        };

        let reversible_tx_id = match TaskUrl::from(&input.reversible_tx_id) {
            Ok(reversible_tx_id) => reversible_tx_id,
            Err(e) => {
                errors.add("reversible_tx_id", e.to_string());
                TaskId("".to_string())
            }
        };

        if errors.is_empty() {
            Ok(Task {
                task_id,
                quan_address,
                quan_amount,
                usdc_amount,
                task_url,
                reversible_tx_id,
                ..input
            })
        } else {
            Err(AppError::ValidationErrors(errors))
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

impl Task {
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

// And an unvalidated version that we can deserialize directly from JSON
#[derive(Debug, Deserialize)]
pub struct TaskInput {
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
