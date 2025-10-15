use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tracing::{debug, error, info, warn};

use crate::{
    db_persistence::{DbError, DbPersistence},
    models::address::{Address, AddressInput},
    utils::generate_referral_code::generate_referral_code,
};

#[derive(Debug, thiserror::Error)]
pub enum GraphqlError {
    #[error("HTTP request failed: {0}")]
    RequestError(#[from] reqwest::Error),
    #[error("GraphQL response error: {0}")]
    GraphqlResponseError(String),
    #[error("JSON parsing error: {0}")]
    JsonError(#[from] serde_json::Error),
    #[error("Database error: {0}")]
    DatabaseError(#[from] DbError),
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

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct TransactionsConnection {
    #[serde(rename = "totalCount")]
    pub total_count: u64,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ReversibleTransactionsConnection {
    #[serde(rename = "totalCount")]
    pub total_count: u64,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct MinerStat {
    #[serde(rename = "totalMinedBlocks")]
    pub total_mined_blocks: u64,
    #[serde(rename = "totalRewards")]
    pub total_rewards: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct StatsData {
    pub transactions: TransactionsConnection,
    #[serde(rename = "reversibleTransactions")]
    pub reversible_transactions: ReversibleTransactionsConnection,
    #[serde(rename = "minerStats")]
    pub miner_stats: Vec<MinerStat>,
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

#[derive(Debug, Clone)]
pub struct GraphqlClient {
    client: Client,
    db: DbPersistence,
    graphql_url: String,
}

impl GraphqlClient {
    pub fn new(db: DbPersistence, graphql_url: String) -> Self {
        let client = Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .unwrap_or_else(|_| Client::new());

        Self {
            client,
            db,
            graphql_url,
        }
    }

    /// Execute a GraphQL query
    pub async fn execute_query<T>(&self, payload: GraphqlQuery) -> GraphqlResult<T>
    where
        T: for<'de> Deserialize<'de>,
    {
        debug!("Executing GraphQL query: {}", payload.query);

        let response = self
            .client
            .post(&self.graphql_url)
            .header("Content-Type", "application/json")
            .json(&payload)
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

        let payload = GraphqlQuery {
            query: TRANSFERS_QUERY.to_string(),
            variables: None,
        };

        info!(
            "Fetching transfers from GraphQL endpoint: {}",
            &self.graphql_url
        );

        let transfer_data: TransferData = self.execute_query(payload).await?;

        info!(
            "Successfully fetched {} transfers",
            transfer_data.transfers.len()
        );
        debug!("Transfer data: {:?}", transfer_data.transfers);

        Ok(transfer_data.transfers)
    }

    pub async fn store_addresses_from_transfers(
        &self,
        transfers: &[Transfer],
    ) -> GraphqlResult<u64> {
        let mut unique_addresses = std::collections::HashSet::new();

        for transfer in transfers {
            unique_addresses.insert(transfer.from.id.clone());
            unique_addresses.insert(transfer.to.id.clone());
        }

        info!(
            "Found {} unique addresses in transfers",
            unique_addresses.len()
        );

        let mut tasks = Vec::new();

        for addr in unique_addresses {
            let task = tokio::spawn(async move {
                if let Ok(referral_code) = generate_referral_code(addr.clone()).await {
                    let input = AddressInput {
                        quan_address: addr,
                        eth_address: None,
                        referral_code,
                    };
                    Address::new(input).ok()
                } else {
                    None
                }
            });
            tasks.push(task);
        }

        let mut addresses_to_store = Vec::new();
        for task in tasks {
            match task.await {
                // Task completed successfully
                Ok(Some(address)) => addresses_to_store.push(address),
                // Task completed but returned None (e.g., referral code failed)
                Ok(None) => (),
                // Task failed to complete (e.g., panicked)
                Err(e) => eprintln!("A task failed to execute: {}", e),
            }
        }

        if addresses_to_store.is_empty() {
            warn!("No valid addresses could be processed and stored");
            return Ok(0);
        }

        debug!("Storing addresses in database: {:?}", addresses_to_store);

        match self.db.addresses.create_many(addresses_to_store).await {
            Ok(created_count) => {
                info!(
                    "Successfully stored {} addresses in database",
                    created_count
                );
                Ok(created_count)
            }
            Err(err) => Err(GraphqlError::DatabaseError(err)),
        }
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

        Ok((transfer_count, address_count as usize))
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

    pub async fn get_address_stats(&self, id: String) -> GraphqlResult<AddressStats> {
        const GET_STATS_QUERY: &str = r#"
        query GetStatsById($id: String!) {
            transactions: transfersConnection(
                orderBy: timestamp_DESC
                where: {
                    extrinsicHash_isNull: false
                    AND: { from: { id_eq: $id }, OR: { to: { id_eq: $id } } }
                }
            ) {
                totalCount
            }
            reversibleTransactions: reversibleTransfersConnection(
                orderBy: timestamp_DESC
                where: { from: { id_eq: $id }, OR: { to: { id_eq: $id } } }
            ) {
                totalCount
            }
            minerStats(where: { id_eq: $id} ) {
                totalMinedBlocks
                totalRewards
            }
        }
        "#;

        let mut variables = HashMap::new();
        variables.insert("id".to_string(), serde_json::json!(id));

        let payload = GraphqlQuery {
            query: GET_STATS_QUERY.to_string(),
            variables: Some(variables),
        };

        info!(
            "Fetching transfers from GraphQL endpoint: {}",
            &self.graphql_url
        );

        let stats_data: StatsData = self.execute_query(payload).await?;
        let miner_stats = stats_data
            .miner_stats
            .first()
            .unwrap_or(&MinerStat {
                total_mined_blocks: 0,
                total_rewards: "0".to_string(),
            })
            .to_owned();

        Ok(AddressStats {
            total_reversible_transactions: stats_data.reversible_transactions.total_count,
            total_transactions: stats_data.transactions.total_count,
            total_mined_blocks: miner_stats.total_mined_blocks,
            total_mining_rewards: miner_stats.total_rewards,
        })
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SyncStats {
    pub total_transfers: usize,
    pub unique_addresses: usize,
    pub last_sync_time: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct AddressStats {
    pub total_transactions: u64,
    pub total_reversible_transactions: u64,
    pub total_mining_rewards: String,
    pub total_mined_blocks: u64,
}

#[cfg(test)]
mod tests {

    use crate::{http_server::AppState, utils::test_db::reset_database, Config};

    use super::*;
    use std::{collections::HashSet, sync::Arc};
    use wiremock::{
        matchers::{body_json, method, path},
        Mock, MockServer, ResponseTemplate,
    };

    // Helper functions
    fn sample_transfers() -> Vec<Transfer> {
        vec![
            Transfer {
                id: "0x123".to_string(),
                amount: "1000000000000000000".to_string(),
                from: Account {
                    id: "0xabcdef123456".to_string(),
                },
                to: Account {
                    id: "0x987654321abc".to_string(),
                },
            },
            Transfer {
                id: "0x456".to_string(),
                amount: "2000000000000000000".to_string(),
                from: Account {
                    id: "0xabcdef123456".to_string(), // Same 'from' address
                },
                to: Account {
                    id: "0xfedcba987654".to_string(),
                },
            },
        ]
    }

    // Helper to create a test GraphqlClient with a mock server
    async fn setup_mock_graphql_client(mock_server: &MockServer) -> GraphqlClient {
        let config = Config::load().expect("Failed to load test configuration");
        let db = DbPersistence::new(config.get_database_url()).await.unwrap();

        // Use the mock server URI instead of the real GraphQL endpoint
        let mock_url = mock_server.uri();
        GraphqlClient::new(db, mock_url)
    }

    #[tokio::test]
    async fn test_get_address_stats_success() {
        // Arrange
        let mock_server = MockServer::start().await;
        let address_string = "qzkxaHg7h4zgk5jPNkJ3a7r9xNgbJNGpJ6a5LPEThDnjkfrC6".to_string();

        const GET_STATS_QUERY: &str = r#"
        query GetStatsById($id: String!) {
            transactions: transfersConnection(
                orderBy: timestamp_DESC
                where: {
                    extrinsicHash_isNull: false
                    AND: { from: { id_eq: $id }, OR: { to: { id_eq: $id } } }
                }
            ) {
                totalCount
            }
            reversibleTransactions: reversibleTransfersConnection(
                orderBy: timestamp_DESC
                where: { from: { id_eq: $id }, OR: { to: { id_eq: $id } } }
            ) {
                totalCount
            }
            minerStats(where: { id_eq: $id} ) {
                totalMinedBlocks
                totalRewards
            }
        }
        "#;

        let expected_request = serde_json::json!({
            "query": GET_STATS_QUERY,
            "variables": {
                "id": address_string
            }
        });

        let mock_response = serde_json::json!({
            "data": {
                "transactions": {
                    "totalCount": 42
                },
                "reversibleTransactions": {
                    "totalCount": 5
                },
                "minerStats": [
                    {
                        "totalMinedBlocks": 10,
                        "totalRewards": "1000000000000000000"
                    }
                ]
            }
        });

        Mock::given(method("POST"))
            .and(path("/"))
            .and(body_json(&expected_request))
            .respond_with(ResponseTemplate::new(200).set_body_json(&mock_response))
            .expect(1)
            .mount(&mock_server)
            .await;

        let client = setup_mock_graphql_client(&mock_server).await;

        // Act
        let result = client.get_address_stats(address_string).await;

        // Assert
        assert!(result.is_ok());

        let stats = result.unwrap();
        assert_eq!(stats.total_transactions, 42);
        assert_eq!(stats.total_reversible_transactions, 5);
        assert_eq!(stats.total_mined_blocks, 10);
        assert_eq!(stats.total_mining_rewards, "1000000000000000000");
    }

    // ============================================================================
    // Serialization/Deserialization Tests
    // ============================================================================

    #[test]
    fn test_transfer_deserialization_single() {
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
    fn test_transfer_deserialization_multiple() {
        let json = r#"
        {
            "transfers": [
                {
                    "id": "0x123",
                    "amount": "1000000000000000000",
                    "from": { "id": "0xabc" },
                    "to": { "id": "0xdef" }
                },
                {
                    "id": "0x456",
                    "amount": "2000000000000000000",
                    "from": { "id": "0x111" },
                    "to": { "id": "0x222" }
                }
            ]
        }
        "#;

        let transfer_data: TransferData = serde_json::from_str(json).unwrap();
        assert_eq!(transfer_data.transfers.len(), 2);
        assert_eq!(transfer_data.transfers[0].id, "0x123");
        assert_eq!(transfer_data.transfers[1].id, "0x456");
    }

    #[test]
    fn test_transfer_deserialization_empty() {
        let json = r#"{"transfers": []}"#;
        let transfer_data: TransferData = serde_json::from_str(json).unwrap();
        assert_eq!(transfer_data.transfers.len(), 0);
    }

    #[test]
    fn test_transfer_deserialization_invalid_json() {
        let json = r#"{"transfers": [{"id": "0x123"}]}"#; // Missing required fields
        let result: Result<TransferData, _> = serde_json::from_str(json);
        assert!(result.is_err());
    }

    #[test]
    fn test_account_deserialization() {
        let json = r#"{"id": "0xabcdef"}"#;
        let account: Account = serde_json::from_str(json).unwrap();
        assert_eq!(account.id, "0xabcdef");
    }

    // ============================================================================
    // GraphQL Query Serialization Tests
    // ============================================================================

    #[test]
    fn test_graphql_query_serialization_simple() {
        let query = GraphqlQuery {
            query: "{ transfers { id } }".to_string(),
            variables: None,
        };

        let json = serde_json::to_string(&query).unwrap();
        assert!(json.contains("transfers"));
        assert!(json.contains("query"));
        assert!(!json.contains("variables"));
    }

    #[test]
    fn test_graphql_query_serialization_with_variables() {
        let mut variables = HashMap::new();
        variables.insert("limit".to_string(), serde_json::json!(10));
        variables.insert("offset".to_string(), serde_json::json!(0));

        let query = GraphqlQuery {
            query: "query($limit: Int, $offset: Int) { transfers(limit: $limit, offset: $offset) { id } }".to_string(),
            variables: Some(variables),
        };

        let json = serde_json::to_string(&query).unwrap();
        assert!(json.contains("variables"));
        assert!(json.contains("limit"));
        assert!(json.contains("offset"));
    }

    #[test]
    fn test_graphql_query_roundtrip() {
        let original = GraphqlQuery {
            query: "{ test }".to_string(),
            variables: None,
        };

        let json = serde_json::to_string(&original).unwrap();
        let deserialized: GraphqlQuery = serde_json::from_str(&json).unwrap();

        assert_eq!(original.query, deserialized.query);
        assert_eq!(
            original.variables.is_none(),
            deserialized.variables.is_none()
        );
    }

    // ============================================================================
    // GraphQL Response Deserialization Tests
    // ============================================================================

    #[test]
    fn test_graphql_response_with_data() {
        let json = r#"{
            "data": {
                "transfers": [
                    {
                        "id": "0x123",
                        "amount": "1000",
                        "from": { "id": "0xabc" },
                        "to": { "id": "0xdef" }
                    }
                ]
            }
        }"#;

        let response: GraphqlResponse<TransferData> = serde_json::from_str(json).unwrap();
        assert!(response.data.is_some());
        assert!(response.errors.is_none());

        let data = response.data.unwrap();
        assert_eq!(data.transfers.len(), 1);
    }

    #[test]
    fn test_graphql_response_with_errors() {
        let json = r#"{
            "errors": [
                {
                    "message": "Field 'transfers' not found",
                    "locations": [{"line": 2, "column": 3}],
                    "path": ["transfers"]
                }
            ]
        }"#;

        let response: GraphqlResponse<TransferData> = serde_json::from_str(json).unwrap();
        assert!(response.data.is_none());
        assert!(response.errors.is_some());

        let errors = response.errors.unwrap();
        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].message, "Field 'transfers' not found");
        assert!(errors[0].locations.is_some());
        assert!(errors[0].path.is_some());
    }

    #[test]
    fn test_graphql_response_with_multiple_errors() {
        let json = r#"{
            "errors": [
                {"message": "Error 1"},
                {"message": "Error 2"},
                {"message": "Error 3"}
            ]
        }"#;

        let response: GraphqlResponse<TransferData> = serde_json::from_str(json).unwrap();
        let errors = response.errors.unwrap();
        assert_eq!(errors.len(), 3);
    }

    #[test]
    fn test_graphql_error_detail_with_locations() {
        let json = r#"{
            "message": "Syntax error",
            "locations": [
                {"line": 1, "column": 5},
                {"line": 2, "column": 10}
            ]
        }"#;

        let error: GraphqlErrorDetail = serde_json::from_str(json).unwrap();
        assert_eq!(error.message, "Syntax error");
        assert!(error.locations.is_some());

        let locations = error.locations.unwrap();
        assert_eq!(locations.len(), 2);
        assert_eq!(locations[0].line, 1);
        assert_eq!(locations[0].column, 5);
    }

    // ============================================================================
    // Error Type Tests
    // ============================================================================

    #[test]
    fn test_graphql_error_display_invalid_data() {
        let err = GraphqlError::InvalidData("test error".to_string());
        assert_eq!(err.to_string(), "Invalid data format: test error");
    }

    #[test]
    fn test_graphql_error_display_graphql_response() {
        let err = GraphqlError::GraphqlResponseError("Query failed".to_string());
        assert_eq!(err.to_string(), "GraphQL response error: Query failed");
    }

    #[test]
    fn test_graphql_error_from_db_error() {
        let db_err = DbError::TaskNotFound("task-123".to_string());
        let graphql_err: GraphqlError = db_err.into();

        match graphql_err {
            GraphqlError::DatabaseError(_) => (),
            _ => panic!("Expected DatabaseError conversion"),
        }
    }

    #[test]
    fn test_graphql_error_from_json_error() {
        let json_err = serde_json::from_str::<Transfer>("invalid json").unwrap_err();
        let graphql_err: GraphqlError = json_err.into();

        match graphql_err {
            GraphqlError::JsonError(_) => (),
            _ => panic!("Expected JsonError conversion"),
        }
    }

    // ============================================================================
    // Business Logic Tests (no external dependencies)
    // ============================================================================

    #[test]
    fn test_unique_addresses_extraction() {
        let transfers = sample_transfers();
        let mut unique_addresses = HashSet::new();

        for transfer in &transfers {
            unique_addresses.insert(transfer.from.id.clone());
            unique_addresses.insert(transfer.to.id.clone());
        }

        // We have 2 transfers with 3 unique addresses:
        // 0xabcdef123456 (appears twice as 'from')
        // 0x987654321abc (appears once as 'to')
        // 0xfedcba987654 (appears once as 'to')
        assert_eq!(unique_addresses.len(), 3);
        assert!(unique_addresses.contains("0xabcdef123456"));
        assert!(unique_addresses.contains("0x987654321abc"));
        assert!(unique_addresses.contains("0xfedcba987654"));
    }

    #[test]
    fn test_unique_addresses_with_duplicates() {
        let transfers = vec![
            Transfer {
                id: "0x1".to_string(),
                amount: "100".to_string(),
                from: Account {
                    id: "0xA".to_string(),
                },
                to: Account {
                    id: "0xB".to_string(),
                },
            },
            Transfer {
                id: "0x2".to_string(),
                amount: "200".to_string(),
                from: Account {
                    id: "0xA".to_string(),
                }, // Duplicate
                to: Account {
                    id: "0xB".to_string(),
                }, // Duplicate
            },
        ];

        let mut unique_addresses = HashSet::new();
        for transfer in &transfers {
            unique_addresses.insert(&transfer.from.id);
            unique_addresses.insert(&transfer.to.id);
        }

        assert_eq!(unique_addresses.len(), 2);
    }

    #[test]
    fn test_unique_addresses_same_from_and_to() {
        let transfers = vec![Transfer {
            id: "0x123".to_string(),
            amount: "1000".to_string(),
            from: Account {
                id: "0xsame".to_string(),
            },
            to: Account {
                id: "0xsame".to_string(),
            }, // Same address
        }];

        let mut unique_addresses = HashSet::new();
        for transfer in &transfers {
            unique_addresses.insert(transfer.from.id.clone());
            unique_addresses.insert(transfer.to.id.clone());
        }

        // Should only have 1 unique address
        assert_eq!(unique_addresses.len(), 1);
        assert!(unique_addresses.contains("0xsame"));
    }

    #[test]
    fn test_empty_transfers_no_addresses() {
        let transfers: Vec<Transfer> = vec![];
        let mut unique_addresses = HashSet::new();

        for transfer in &transfers {
            unique_addresses.insert(transfer.from.id.clone());
            unique_addresses.insert(transfer.to.id.clone());
        }

        assert_eq!(unique_addresses.len(), 0);
    }

    // ============================================================================
    // Property-based Tests
    // ============================================================================

    #[test]
    fn test_unique_addresses_count_property() {
        // Property: The number of unique addresses should always be <= (transfers.len() * 2)
        let transfers = sample_transfers();
        let mut unique_addresses = HashSet::new();

        for transfer in &transfers {
            unique_addresses.insert(&transfer.from.id);
            unique_addresses.insert(&transfer.to.id);
        }

        assert!(unique_addresses.len() <= transfers.len() * 2);
    }

    #[test]
    fn test_unique_addresses_always_positive_for_non_empty() {
        // Property: If there are transfers, there must be at least 1 unique address
        let transfers = sample_transfers();
        assert!(!transfers.is_empty());

        let mut unique_addresses = HashSet::new();
        for transfer in &transfers {
            unique_addresses.insert(&transfer.from.id);
            unique_addresses.insert(&transfer.to.id);
        }

        assert!(unique_addresses.len() >= 1);
    }

    // ============================================================================
    // Clone and Debug Tests
    // ============================================================================

    #[test]
    fn test_transfer_clone() {
        let transfer = Transfer {
            id: "0x123".to_string(),
            amount: "1000".to_string(),
            from: Account {
                id: "0xabc".to_string(),
            },
            to: Account {
                id: "0xdef".to_string(),
            },
        };

        let cloned = transfer.clone();
        assert_eq!(transfer.id, cloned.id);
        assert_eq!(transfer.amount, cloned.amount);
        assert_eq!(transfer.from.id, cloned.from.id);
        assert_eq!(transfer.to.id, cloned.to.id);
    }

    #[test]
    fn test_account_clone() {
        let account = Account {
            id: "0xtest".to_string(),
        };
        let cloned = account.clone();
        assert_eq!(account.id, cloned.id);
    }

    #[test]
    fn test_transfer_debug() {
        let transfer = Transfer {
            id: "0x123".to_string(),
            amount: "1000".to_string(),
            from: Account {
                id: "0xabc".to_string(),
            },
            to: Account {
                id: "0xdef".to_string(),
            },
        };

        let debug_str = format!("{:?}", transfer);
        assert!(debug_str.contains("0x123"));
        assert!(debug_str.contains("1000"));
    }

    // ============================================================================
    // SyncStats Tests
    // ============================================================================

    #[test]
    fn test_sync_stats_serialization() {
        let stats = SyncStats {
            total_transfers: 10,
            unique_addresses: 15,
            last_sync_time: chrono::Utc::now(),
        };

        let json = serde_json::to_string(&stats).unwrap();
        assert!(json.contains("total_transfers"));
        assert!(json.contains("unique_addresses"));
        assert!(json.contains("last_sync_time"));
    }

    #[test]
    fn test_sync_stats_deserialization() {
        let json = r#"{
            "total_transfers": 5,
            "unique_addresses": 8,
            "last_sync_time": "2024-01-01T00:00:00Z"
        }"#;

        let stats: SyncStats = serde_json::from_str(json).unwrap();
        assert_eq!(stats.total_transfers, 5);
        assert_eq!(stats.unique_addresses, 8);
    }

    // ============================================================================
    // Edge Cases
    // ============================================================================

    #[test]
    fn test_transfer_with_empty_strings() {
        let transfer = Transfer {
            id: "".to_string(),
            amount: "".to_string(),
            from: Account { id: "".to_string() },
            to: Account { id: "".to_string() },
        };

        assert_eq!(transfer.id, "");
        assert_eq!(transfer.amount, "");
    }

    #[test]
    fn test_transfer_with_very_large_amount() {
        let transfer = Transfer {
            id: "0x1".to_string(),
            amount: "999999999999999999999999999999".to_string(),
            from: Account {
                id: "0xa".to_string(),
            },
            to: Account {
                id: "0xb".to_string(),
            },
        };

        assert_eq!(transfer.amount, "999999999999999999999999999999");
    }

    #[test]
    fn test_graphql_query_with_special_characters() {
        let query = GraphqlQuery {
            query: r#"{ transfers(where: {amount_gt: "100"}) { id } }"#.to_string(),
            variables: None,
        };

        let json = serde_json::to_string(&query).unwrap();
        assert!(json.contains("amount_gt"));
    }
}
