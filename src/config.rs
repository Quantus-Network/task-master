use std::path::Path;

use axum::http::HeaderValue;
use rusx::config::OauthConfig;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub server: ServerConfig,
    pub candidates: CandidatesConfig,
    pub data: DataConfig,
    pub logging: LoggingConfig,
    pub jwt: JwtConfig,
    pub x_oauth: OauthConfig,
    pub remote_configs: RemoteConfigsConfig,
    pub risk_checker: RiskCheckerConfig,
    pub exchange_rate: ExchangeRateConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoteConfigsConfig {
    pub wallet_configs_file: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerConfig {
    pub host: String,
    pub port: u16,
    pub cors_allowed_origins: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CandidatesConfig {
    pub graphql_url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DataConfig {
    pub database_url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoggingConfig {
    pub level: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JwtConfig {
    pub secret: String,
    pub admin_secret: String,
    pub exp_in_hours: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RiskCheckerConfig {
    pub etherscan_api_key: String,
    pub etherscan_base_url: String,
    pub infura_api_key: String,
    pub infura_base_url: String,
    pub etherscan_calls_per_sec: u32,
    pub max_concurrent_requests: usize,
}

/// Exchange rate API (e.g. [ExchangeRate-API v6](https://www.exchangerate-api.com/)).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExchangeRateConfig {
    pub api_key: String,
}

impl Config {
    pub fn load(config_path: &str) -> Result<Self, config::ConfigError> {
        let settings = config::Config::builder()
            .add_source(config::File::new(config_path, config::FileFormat::Toml))
            .add_source(config::Environment::with_prefix("TASKMASTER"))
            .build()?;

        let mut config: Self = settings.try_deserialize()?;
        config.resolve_relative_paths(config_path);
        Ok(config)
    }

    #[cfg(test)]
    pub fn load_test_env() -> Result<Self, config::ConfigError> {
        let test_config_path = "config/test.toml";
        let settings = config::Config::builder()
            // Load the test-specific configuration file
            .add_source(config::File::new(test_config_path, config::FileFormat::Toml))
            // You can still layer environment variables for testing if you need to
            .add_source(config::Environment::with_prefix("TASKMASTER"))
            .build()?;

        let mut config: Self = settings.try_deserialize()?;
        config.resolve_relative_paths(test_config_path);
        Ok(config)
    }

    pub fn get_database_url(&self) -> &str {
        &self.data.database_url
    }

    pub fn get_server_address(&self) -> String {
        format!("{}:{}", self.server.host, self.server.port)
    }

    pub fn get_jwt_expiration(&self) -> chrono::Duration {
        chrono::Duration::hours(self.jwt.exp_in_hours)
    }

    pub fn get_cors_allowed_origins(&self) -> Vec<HeaderValue> {
        self.server
            .cors_allowed_origins
            .iter()
            .filter_map(|o| match o.parse() {
                Ok(v) => Some(v),
                Err(e) => {
                    tracing::warn!("Skipping invalid CORS origin {:?}: {}", o, e);
                    None
                }
            })
            .collect()
    }

    fn resolve_relative_paths(&mut self, config_path: &str) {
        let wallet_configs_path = Path::new(&self.remote_configs.wallet_configs_file);
        if wallet_configs_path.is_absolute() {
            return;
        }
        let base_dir = Path::new(config_path).parent().expect("Failed to get base directory");
        self.remote_configs.wallet_configs_file = base_dir.join(wallet_configs_path).to_string_lossy().to_string();
    }
}
