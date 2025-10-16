use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EthAddressAssociation {
    pub quan_address: String,
    pub eth_address: String,
}
