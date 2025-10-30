use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{postgres::PgRow, FromRow, Row};

use crate::models::address::QuanAddress;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct OptIn {
    pub quan_address: QuanAddress,
    pub opt_in_number: i32,
    pub created_at: DateTime<Utc>,
}

impl<'r> FromRow<'r, PgRow> for OptIn {
    fn from_row(row: &'r PgRow) -> Result<Self, sqlx::Error> {
        let quan_address = row.try_get("quan_address")?;
        let opt_in_number = row.try_get("opt_in_number")?;
        let created_at = row.try_get("created_at")?;

        Ok(OptIn {
            quan_address,
            opt_in_number,
            created_at,
        })
    }
}

