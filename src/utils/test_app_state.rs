use crate::{db_persistence::DbPersistence, http_server::AppState, metrics::Metrics, Config, GraphqlClient};
use rusx::TwitterAuth;
use std::sync::{Arc, Mutex};

pub async fn create_test_app_state() -> AppState {
    let config = Config::load_test_env().expect("Failed to load test configuration");
    let db = DbPersistence::new_unmigrated(config.get_database_url()).await.unwrap();
    let x_oauth = TwitterAuth::new(config.x_oauth.clone()).unwrap();
    let graphql_client = GraphqlClient::new(db.clone(), config.candidates.graphql_url.clone());

    return AppState {
        db: Arc::new(db),
        metrics: Arc::new(Metrics::new()),
        graphql_client: Arc::new(graphql_client),
        config: Arc::new(config),
        x_oauth: Arc::new(x_oauth),
        oauth_sessions: Arc::new(Mutex::new(std::collections::HashMap::new())),
        challenges: Arc::new(tokio::sync::RwLock::new(std::collections::HashMap::new())),
    };
}
