use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{postgres::PgRow, FromRow, Row};

use crate::{
    errors::{AppError, ValidationErrors},
    models::address::QuanAddress,
};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Referral {
    pub referrer_address: QuanAddress,
    pub referee_address: QuanAddress,
    pub created_at: Option<DateTime<Utc>>,
}
impl Referral {
    pub fn new(input: ReferralInput) -> Result<Self, AppError> {
        let mut errors = ValidationErrors::new();

        let referrer_address = match QuanAddress::from(&input.referrer_address) {
            Ok(name) => name,
            Err(e) => {
                errors.add("referrer_address", e.to_string());
                QuanAddress("".to_string())
            }
        };

        let referee_address = match QuanAddress::from(&input.referee_address) {
            Ok(name) => name,
            Err(e) => {
                errors.add("referee_address", e.to_string());
                QuanAddress("".to_string())
            }
        };

        if errors.is_empty() {
            Ok(Referral {
                referrer_address,
                referee_address,
                created_at: None,
            })
        } else {
            Err(AppError::ValidationErrors(errors))
        }
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
