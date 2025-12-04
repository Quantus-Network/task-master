use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{postgres::PgRow, FromRow, Row};

use crate::models::{ModelError, ModelResult};

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

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Address {
    pub quan_address: QuanAddress,
    pub referral_code: String,
    pub referrals_count: i32,
    pub updated_at: Option<DateTime<Utc>>,
    pub created_at: Option<DateTime<Utc>>,
}
impl Address {
    pub fn new(input: AddressInput) -> ModelResult<Self> {
        let quan_address = match QuanAddress::from(&input.quan_address) {
            Ok(name) => name,
            Err(e) => {
                tracing::error!(error = %e, "Invalid quan address input");
                return Err(ModelError::InvalidInput);
            }
        };

        Ok(Address {
            quan_address,
            referral_code: input.referral_code.to_lowercase(),
            referrals_count: 0,
            created_at: None,
            updated_at: None,
        })
    }
}
impl<'r> FromRow<'r, PgRow> for Address {
    fn from_row(row: &'r PgRow) -> Result<Self, sqlx::Error> {
        let quan_address = row.try_get("quan_address")?;
        let referral_code = row.try_get("referral_code")?;
        let referrals_count = row.try_get("referrals_count")?;
        let updated_at = row.try_get("updated_at")?;
        let created_at = row.try_get("created_at")?;

        Ok(Address {
            quan_address,
            referral_code,
            referrals_count,
            updated_at,
            created_at,
        })
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AddressSortColumn {
    CreatedAt,
    ReferralsCount,
    ReferralCode,
    Address,
    OptInNumber,
    UpdatedAt,
    EthAddress,
    XUsername,
    IsOptedIn,
}

impl AddressSortColumn {
    // Helper to map enum to actual SQL column names
    pub fn to_sql_column(&self) -> &'static str {
        match self {
            AddressSortColumn::CreatedAt => "a.created_at",
            AddressSortColumn::ReferralsCount => "a.referrals_count",
            AddressSortColumn::OptInNumber => "o.opt_in_number",
            AddressSortColumn::UpdatedAt => "a.updated_at",
            AddressSortColumn::EthAddress => "e.eth_address",
            AddressSortColumn::XUsername => "x.username",
            AddressSortColumn::IsOptedIn => "o.quan_address",
            AddressSortColumn::ReferralCode => "a.referral_code",
            AddressSortColumn::Address => "a.quan_address",
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct AddressFilter {
    pub is_opted_in: Option<bool>,
    pub min_referrals: Option<i32>,
    pub has_eth_address: Option<bool>,
    pub has_x_account: Option<bool>,
}

// An unvalidated version that we can deserialize directly from JSON
#[derive(Debug, Deserialize)]
pub struct AddressInput {
    pub quan_address: String,
    pub referral_code: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct NewAddressPayload {
    pub quan_address: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SyncTransfersResponse {
    pub success: bool,
    pub message: String,
    pub transfers_processed: Option<usize>,
    pub addresses_stored: Option<usize>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RewardProgramStatusPayload {
    pub new_status: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AddressStatsResponse {
    pub referrals: u64,
    pub referral_events: u64,
    pub immediate_txs: u64,
    pub reversible_txs: u64,
    pub mining_events: u64,
    pub mining_rewards: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AggregateStatsQueryParams {
    #[serde(default)]
    pub addresses: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct OptedInPositionResponse {
    pub quan_address: String,
    pub position: i64,
    pub is_opted_in: bool,
}

#[derive(sqlx::FromRow, Debug, Clone, Serialize)]
pub struct AddressWithRank {
    #[sqlx(flatten)]
    pub address: Address,
    pub rank: i64,
}

#[derive(Debug, Serialize)]
pub struct AssociatedAccountsResponse {
    pub eth_address: Option<String>,
    pub x_username: Option<String>,
}

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct AddressWithOptInAndAssociations {
    #[sqlx(flatten)]
    pub address: Address,
    pub is_opted_in: bool,
    pub opt_in_number: Option<i32>,
    pub eth_address: Option<String>,
    pub x_username: Option<String>,
}
