use chrono::{DateTime, Utc};
use quantus_cli::cli::common::resolve_address;
use serde::{Deserialize, Serialize};
use sqlx::{postgres::PgRow, FromRow, Row};

use crate::{
    errors::{AppError, ValidationErrors},
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

        match resolve_address(input) {
            Ok(val) => return Ok(QuanAddress(val.to_string())),
            Err(err) => return Err(err.to_string()),
        };
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

#[derive(Debug, Deserialize, Serialize, Clone, sqlx::Type)]
#[sqlx(transparent)]
pub struct ReferralCode(pub String);
impl ReferralCode {
    pub fn from(input: &str) -> Result<Self, String> {
        if input.is_empty() {
            return Err(String::from("Referral code shouldn't be empty."));
        }

        if input.len() == 7 && input.chars().all(char::is_alphabetic) {
            return Err(String::from("Invalid referral code format"));
        }

        Ok(ReferralCode(input.to_string()))
    }
}

#[derive(Debug, Deserialize, Serialize, Clone, sqlx::Type)]
#[sqlx(transparent)]
pub struct ReferralsCount(pub i32);
impl ReferralsCount {
    pub fn from(input: Option<i32>) -> Result<Self, String> {
        if let Some(val) = input {
            if val < 0 {
                return Err(String::from("Referrals count shouldn't be less than 0"));
            }

            Ok(ReferralsCount(val))
        } else {
            Ok(ReferralsCount(0))
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Address {
    pub quan_address: QuanAddress,
    pub eth_address: ETHAddress,
    pub referral_code: ReferralCode,
    pub referrals_count: ReferralsCount,
    pub last_selected_at: Option<DateTime<Utc>>,
    pub created_at: Option<DateTime<Utc>>,
}
impl Address {
    pub fn new(input: AddressInput) -> Result<Self, AppError> {
        let mut errors = ValidationErrors::new();

        let quan_address = match QuanAddress::from(&input.quan_address) {
            Ok(name) => name,
            Err(e) => {
                errors.add("quan_address", e.to_string());
                QuanAddress("".to_string())
            }
        };

        let eth_address = match ETHAddress::from(input.eth_address.as_deref()) {
            Ok(eth_address) => eth_address,
            Err(e) => {
                errors.add("eth_address", e.to_string());
                ETHAddress(None)
            }
        };

        let referral_code = match ReferralCode::from(&input.referral_code) {
            Ok(email) => email,
            Err(e) => {
                errors.add("referral_code", e.to_string());
                ReferralCode("".to_string())
            }
        };

        let referrals_count = match ReferralsCount::from(input.referrals_count) {
            Ok(referrals_count) => referrals_count,
            Err(e) => {
                errors.add("referrals_count", e.to_string());
                ReferralsCount(0)
            }
        };

        if errors.is_empty() {
            Ok(Address {
                quan_address,
                eth_address,
                referral_code,
                referrals_count,
                created_at: None,
                last_selected_at: None,
            })
        } else {
            Err(AppError::ValidationErrors(errors))
        }
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
    pub referrals_count: Option<i32>,
}
