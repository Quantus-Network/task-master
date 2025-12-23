use crate::{
    db_persistence::DbPersistence, http_server::AppState, metrics::Metrics, models::auth::TokenClaims,
    services::alert_service::AlertService, Config, GraphqlClient,
};
use jsonwebtoken::{encode, EncodingKey, Header};
use rusx::RusxGateway;
use std::sync::{Arc, Mutex};

pub async fn create_test_app_state() -> AppState {
    let config = Config::load_test_env().expect("Failed to load test configuration");
    let db = DbPersistence::new(config.get_database_url()).await.unwrap();
    let twitter_gateway = RusxGateway::new(config.x_oauth.clone(), None).unwrap();
    let graphql_client = GraphqlClient::new(db.clone(), config.candidates.graphql_url.clone());

    return AppState {
        db: Arc::new(db),
        metrics: Arc::new(Metrics::new()),
        graphql_client: Arc::new(graphql_client),
        alert_client: Arc::new(AlertService::new(config.alert.webhook_url.clone())),
        config: Arc::new(config),
        twitter_gateway: Arc::new(twitter_gateway),
        oauth_sessions: Arc::new(Mutex::new(std::collections::HashMap::new())),
        twitter_oauth_tokens: Arc::new(tokio::sync::RwLock::new(std::collections::HashMap::new())),
        challenges: Arc::new(tokio::sync::RwLock::new(std::collections::HashMap::new())),
    };
}

pub fn generate_test_token(secret: &str, user_id: &str) -> String {
    let claims = TokenClaims {
        sub: user_id.to_string(),
        iat: 1,          // Just a valid past timestamp
        exp: 9999999999, // Far future timestamp,
    };

    encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(secret.as_bytes()),
    )
    .expect("Failed to sign token")
}
