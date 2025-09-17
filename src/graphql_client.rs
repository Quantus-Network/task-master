use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tracing::{debug, error, info, warn};

use crate::db_persistence::DbPersistence;

const GRAPHQL_ENDPOINT: &str = "https://gql.res.fm/graphql";

#[derive(Debug, thiserror::Error)]
pub enum GraphqlError {
    #[error("HTTP request failed: {0}")]
    RequestError(#[from] reqwest::Error),
    #[error("GraphQL response error: {0}")]
    GraphqlResponseError(String),
    #[error("JSON parsing error: {0}")]
    JsonError(#[from] serde_json::Error),
    #[error("Database error: {0}")]
    DatabaseError(#[from] crate::db_persistence::DbError),
    #[error("Invalid data format: {0}")]
    InvalidData(String),
}

pub type GraphqlResult<T> = Result<T, GraphqlError>;

#[derive(Debug, Serialize, Deserialize)]
pub struct GraphqlQuery {
    query: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    variables: Option<HashMap<String, serde_json::Value>>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct GraphqlResponse<T> {
    #[serde(skip_serializing_if = "Option::is_none")]
    data: Option<T>,
    #[serde(skip_serializing_if = "Option::is_none")]
    errors: Option<Vec<GraphqlErrorDetail>>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct GraphqlErrorDetail {
    message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    locations: Option<Vec<GraphqlErrorLocation>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    path: Option<Vec<serde_json::Value>>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct GraphqlErrorLocation {
    line: u32,
    column: u32,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct TransferData {
    transfers: Vec<Transfer>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Transfer {
    pub id: String,
    pub amount: String,
    pub from: Account,
    pub to: Account,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Account {
    pub id: String,
}

pub struct GraphqlClient {
    client: Client,
    db: DbPersistence,
}

impl GraphqlClient {
    pub fn new(db: DbPersistence) -> Self {
        let client = Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .unwrap_or_else(|_| Client::new());

        Self { client, db }
    }

    /// Execute a GraphQL query
    pub async fn execute_query<T>(&self, query: &str) -> GraphqlResult<T>
    where
        T: for<'de> Deserialize<'de>,
    {
        let graphql_query = GraphqlQuery {
            query: query.to_string(),
            variables: None,
        };

        debug!("Executing GraphQL query: {}", query);

        let response = self
            .client
            .post(GRAPHQL_ENDPOINT)
            .header("Content-Type", "application/json")
            .json(&graphql_query)
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            return Err(GraphqlError::GraphqlResponseError(format!(
                "HTTP {} - {}",
                status, text
            )));
        }

        let graphql_response: GraphqlResponse<T> = response.json().await?;

        if let Some(errors) = graphql_response.errors {
            let error_messages: Vec<String> = errors.into_iter().map(|e| e.message).collect();
            return Err(GraphqlError::GraphqlResponseError(
                error_messages.join(", "),
            ));
        }

        graphql_response
            .data
            .ok_or_else(|| GraphqlError::InvalidData("No data in GraphQL response".to_string()))
    }

    /// Fetch transfers from the GraphQL endpoint
    pub async fn fetch_transfers(&self) -> GraphqlResult<Vec<Transfer>> {
        const TRANSFERS_QUERY: &str = r#"
        {
            transfers {
                id
                amount
                from { id }
                to { id }
            }
        }
        "#;

        info!(
            "Fetching transfers from GraphQL endpoint: {}",
            GRAPHQL_ENDPOINT
        );

        let transfer_data: TransferData = self.execute_query(TRANSFERS_QUERY).await?;

        info!(
            "Successfully fetched {} transfers",
            transfer_data.transfers.len()
        );
        debug!("Transfer data: {:?}", transfer_data.transfers);

        Ok(transfer_data.transfers)
    }

    /// Store addresses from transfers in the database
    pub async fn store_addresses_from_transfers(
        &self,
        transfers: &[Transfer],
    ) -> GraphqlResult<usize> {
        let mut unique_addresses = std::collections::HashSet::new();

        // Collect unique addresses from both 'from' and 'to' fields
        for transfer in transfers {
            unique_addresses.insert(&transfer.from.id);
            unique_addresses.insert(&transfer.to.id);
        }

        info!(
            "Found {} unique addresses in transfers",
            unique_addresses.len()
        );

        // Convert to the format expected by add_addresses: Vec<(String, Option<String>)>
        // quan_address, eth_address (None since these are quan addresses from transfers)
        let addresses_to_store: Vec<(String, Option<String>)> = unique_addresses
            .into_iter()
            .map(|addr| (addr.to_string(), None))
            .collect();

        if addresses_to_store.is_empty() {
            warn!("No addresses to store");
            return Ok(0);
        }

        debug!("Storing addresses in database: {:?}", addresses_to_store);

        // Store addresses in the database
        self.db.add_addresses(addresses_to_store.clone()).await?;

        let stored_count = addresses_to_store.len();
        info!("Successfully stored {} addresses in database", stored_count);

        Ok(stored_count)
    }

    /// Fetch transfers and store their addresses in one operation
    pub async fn sync_transfers_and_addresses(&self) -> GraphqlResult<(usize, usize)> {
        info!("Starting transfer sync process");

        let transfers = self.fetch_transfers().await?;
        let transfer_count = transfers.len();

        let address_count = self.store_addresses_from_transfers(&transfers).await?;

        info!(
            "Sync completed: {} transfers processed, {} addresses stored",
            transfer_count, address_count
        );

        Ok((transfer_count, address_count))
    }

    /// Get statistics about stored transfers and addresses
    pub async fn get_sync_stats(&self) -> GraphqlResult<SyncStats> {
        // Note: This would require additional database queries to get counts
        // For now, we'll return basic stats from the current sync
        let transfers = self.fetch_transfers().await?;
        let unique_addresses: std::collections::HashSet<&String> = transfers
            .iter()
            .flat_map(|t| [&t.from.id, &t.to.id])
            .collect();

        Ok(SyncStats {
            total_transfers: transfers.len(),
            unique_addresses: unique_addresses.len(),
            last_sync_time: chrono::Utc::now(),
        })
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SyncStats {
    pub total_transfers: usize,
    pub unique_addresses: usize,
    pub last_sync_time: chrono::DateTime<chrono::Utc>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_transfer_deserialization() {
        let json = r#"
        {
            "transfers": [
                {
                    "id": "0x123",
                    "amount": "1000000000000000000",
                    "from": { "id": "0xabcdef123456" },
                    "to": { "id": "0x987654321abc" }
                }
            ]
        }
        "#;

        let transfer_data: TransferData = serde_json::from_str(json).unwrap();
        assert_eq!(transfer_data.transfers.len(), 1);
        assert_eq!(transfer_data.transfers[0].id, "0x123");
        assert_eq!(transfer_data.transfers[0].amount, "1000000000000000000");
        assert_eq!(transfer_data.transfers[0].from.id, "0xabcdef123456");
        assert_eq!(transfer_data.transfers[0].to.id, "0x987654321abc");
    }

    #[test]
    fn test_graphql_query_serialization() {
        let query = GraphqlQuery {
            query: "{ transfers { id } }".to_string(),
            variables: None,
        };

        let json = serde_json::to_string(&query).unwrap();
        assert!(json.contains("transfers"));
    }
}
