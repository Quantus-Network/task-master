use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{postgres::PgRow, FromRow, Row};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct RaidQuest {
    pub id: i32,
    pub name: String,
    pub start_date: DateTime<Utc>,
    pub end_date: Option<DateTime<Utc>>,
    pub updated_at: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
}

impl<'r> FromRow<'r, PgRow> for RaidQuest {
    fn from_row(row: &'r PgRow) -> Result<Self, sqlx::Error> {
        let id = row.try_get("id")?;
        let name = row.try_get("name")?;
        let start_date = row.try_get("start_date")?;
        let end_date = row.try_get("end_date")?;
        let updated_at = row.try_get("updated_at")?;
        let created_at = row.try_get("created_at")?;

        Ok(RaidQuest {
            id,
            name,
            start_date,
            end_date,
            updated_at,
            created_at,
        })
    }
}

#[derive(Debug, Deserialize, Serialize)]
pub struct CreateRaidQuest {
    pub name: String,
    pub start_date: Option<DateTime<Utc>>,
    pub end_date: Option<DateTime<Utc>>,
}
