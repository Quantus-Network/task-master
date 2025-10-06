use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{postgres::PgRow, FromRow, Row};

use crate::{
    models::{ModelError, ModelResult},
    utils::eth_address_validator::is_valid_eth_address,
};

#[derive(Debug, Deserialize, Serialize, Clone, sqlx::Type)]
#[sqlx(transparent)]
pub struct QuanAddress(pub String);
impl QuanAddress {
    pub fn from(input: &str) -> Result<Self, String> {
        if input.is_empty() {
            return Err(String::from("Quan address shouldn't be empty."));
        }

        if !input.starts_with("qz") || input.len() <= 10 {
            return Err(String::from("Invalid address format."));
        }

        Ok(QuanAddress(input.to_string()))
    }
}

#[derive(Debug, Deserialize, Serialize, Clone, sqlx::Type)]
#[sqlx(transparent)]
pub struct ETHAddress(pub Option<String>);
impl ETHAddress {
    pub fn from(input: Option<&str>) -> Result<Self, String> {
        if let Some(val) = input {
            if !is_valid_eth_address(val) {
                return Err(String::from("Invalid ETH address"));
            }

            Ok(ETHAddress(Some(val.to_string())))
        } else {
            Ok(ETHAddress(None))
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Address {
    pub quan_address: QuanAddress,
    pub eth_address: ETHAddress,
    pub referral_code: String,
    pub referrals_count: i32,
    pub last_selected_at: Option<DateTime<Utc>>,
    pub created_at: Option<DateTime<Utc>>,
}
impl Address {
    pub fn new(input: AddressInput) -> ModelResult<Self> {
        let quan_address = match QuanAddress::from(&input.quan_address) {
            Ok(name) => name,
            Err(e) => return Err(ModelError::InvalidInput),
        };

        let eth_address = match ETHAddress::from(input.eth_address.as_deref()) {
            Ok(eth_address) => eth_address,
            Err(e) => return Err(ModelError::InvalidInput),
        };

        Ok(Address {
            quan_address,
            eth_address,
            referral_code: input.referral_code,
            referrals_count: 0,
            created_at: None,
            last_selected_at: None,
        })
    }
}
impl<'r> FromRow<'r, PgRow> for Address {
    fn from_row(row: &'r PgRow) -> Result<Self, sqlx::Error> {
        let quan_address = row.try_get("quan_address")?;
        let eth_address = row.try_get("eth_address")?;
        let referral_code = row.try_get("referral_code")?;
        let referrals_count = row.try_get("referrals_count")?;
        let created_at = row.try_get("created_at")?;
        let last_selected_at = row.try_get("last_selected_at")?;

        Ok(Address {
            quan_address,
            eth_address,
            referral_code,
            referrals_count,
            created_at,
            last_selected_at,
        })
    }
}

// And an unvalidated version that we can deserialize directly from JSON
#[derive(Debug, Deserialize)]
pub struct AddressInput {
    pub quan_address: String,
    pub eth_address: Option<String>,
    pub referral_code: String,
}
