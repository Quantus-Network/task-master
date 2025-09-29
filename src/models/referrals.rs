use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{postgres::PgRow, FromRow, Row};

use crate::models::{address::QuanAddress, ModelError, ModelResult};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Referral {
    pub referrer_address: QuanAddress,
    pub referee_address: QuanAddress,
    pub created_at: Option<DateTime<Utc>>,
}
impl Referral {
    pub fn new(input: ReferralInput) -> ModelResult<Self> {
        let referrer_address = match QuanAddress::from(&input.referrer_address) {
            Ok(name) => name,
            Err(e) => return Err(ModelError::InvalidInput),
        };

        let referee_address = match QuanAddress::from(&input.referee_address) {
            Ok(name) => name,
            Err(e) => return Err(ModelError::InvalidInput),
        };

        Ok(Referral {
            referrer_address,
            referee_address,
            created_at: None,
        })
    }
}
impl<'r> FromRow<'r, PgRow> for Referral {
    fn from_row(row: &'r PgRow) -> Result<Self, sqlx::Error> {
        let referrer_address = row.try_get("referrer_address")?;
        let referee_address = row.try_get("referee_address")?;
        let created_at = row.try_get("created_at")?;

        Ok(Referral {
            referrer_address,
            referee_address,
            created_at,
        })
    }
}

// And an unvalidated version that we can deserialize directly from JSON
#[derive(Debug, Deserialize)]
pub struct ReferralInput {
    pub referrer_address: String,
    pub referee_address: String,
}
