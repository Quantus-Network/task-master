use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{postgres::PgRow, FromRow, Row};

use crate::{
    models::{address::QuanAddress, ModelError, ModelResult},
    utils::eth_address_validator::is_valid_eth_address,
};

#[derive(Debug, Deserialize, Serialize, Clone, sqlx::Type)]
#[sqlx(transparent)]
pub struct EthAddress(pub String);
impl EthAddress {
    pub fn from(input: &str) -> Result<Self, String> {
        if !is_valid_eth_address(input) {
            return Err(String::from("Invalid ETH address"));
        }

        Ok(EthAddress(input.to_string()))
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct EthAssociation {
    pub quan_address: QuanAddress,
    pub eth_address: EthAddress,
    pub created_at: Option<DateTime<Utc>>,
}

impl EthAssociation {
    pub fn new(input: EthAssociationInput) -> ModelResult<Self> {
        let quan_address = match QuanAddress::from(&input.quan_address) {
            Ok(name) => name,
            Err(e) => {
                tracing::error!(error = %e, "Invalid quan address input");
                return Err(ModelError::InvalidInput);
            }
        };

        let eth_address = match EthAddress::from(&input.eth_address) {
            Ok(eth_address) => eth_address,
            Err(e) => {
                tracing::error!(error = %e, "Invalid ETH address input");
                return Err(ModelError::InvalidInput);
            }
        };

        Ok(EthAssociation {
            quan_address,
            eth_address,
            created_at: None,
        })
    }
}

impl<'r> FromRow<'r, PgRow> for EthAssociation {
    fn from_row(row: &'r PgRow) -> Result<Self, sqlx::Error> {
        let quan_address = row.try_get("quan_address")?;
        let eth_address = row.try_get("eth_address")?;
        let created_at = row.try_get("created_at")?;

        Ok(EthAssociation {
            quan_address,
            eth_address,
            created_at,
        })
    }
}

// An unvalidated version that we can deserialize directly from JSON
#[derive(Debug, Deserialize)]
pub struct EthAssociationInput {
    pub quan_address: String,
    pub eth_address: String,
}

#[derive(Debug, Deserialize)]
pub struct AssociateEthAddressRequest {
    pub eth_address: String,
}

#[derive(Debug, Serialize)]
pub struct AssociateEthAddressResponse {
    pub success: bool,
    pub message: String,
}
