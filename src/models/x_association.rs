use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{postgres::PgRow, FromRow, Row};

use crate::models::{address::QuanAddress, ModelError, ModelResult};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct XAssociation {
    pub quan_address: QuanAddress,
    pub username: String,
    pub created_at: Option<DateTime<Utc>>,
}

impl XAssociation {
    pub fn new(input: XAssociationInput) -> ModelResult<Self> {
        let quan_address = match QuanAddress::from(&input.quan_address) {
            Ok(name) => name,
            Err(e) => {
                tracing::error!(error = %e, "Invalid quan address input");
                return Err(ModelError::InvalidInput);
            }
        };

        Ok(XAssociation {
            quan_address,
            username: input.username,
            created_at: None,
        })
    }
}

impl<'r> FromRow<'r, PgRow> for XAssociation {
    fn from_row(row: &'r PgRow) -> Result<Self, sqlx::Error> {
        let quan_address = row.try_get("quan_address")?;
        let username = row.try_get("username")?;
        let created_at = row.try_get("created_at")?;

        Ok(XAssociation {
            quan_address,
            username,
            created_at,
        })
    }
}

// An unvalidated version that we can deserialize directly from JSON
#[derive(Debug, Deserialize)]
pub struct XAssociationInput {
    pub quan_address: String,
    pub username: String,
}

#[derive(Debug, Deserialize)]
pub struct AssociateXHandleRequest {
    pub username: String,
}
