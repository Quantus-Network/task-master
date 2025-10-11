use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub server: ServerConfig,
    pub blockchain: BlockchainConfig,
    pub candidates: CandidatesConfig,
    pub task_generation: TaskGenerationConfig,
    pub reverser: ReverserConfig,
    pub data: DataConfig,
    pub logging: LoggingConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerConfig {
    pub host: String,
    pub port: u16,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlockchainConfig {
    pub node_url: String,
    pub wallet_name: String,
    pub wallet_password: String,
    pub reversal_period_hours: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CandidatesConfig {
    pub graphql_url: String,
    pub refresh_interval_minutes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskGenerationConfig {
    pub generation_interval_minutes: u64,
    pub taskees_per_round: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReverserConfig {
    pub early_reversal_minutes: u64,
    pub check_interval_seconds: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DataConfig {
    pub database_url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoggingConfig {
    pub level: String,
}

impl Config {
    /// This is the **PRODUCTION** version, used for `cargo run` and `cargo build`.
    /// It loads `config/default`.
    #[cfg(not(test))]
    pub fn load() -> Result<Self, config::ConfigError> {
        let settings = config::Config::builder()
            .add_source(config::File::with_name("config/default"))
            .add_source(config::Environment::with_prefix("TASKMASTER"))
            .build()?;

        settings.try_deserialize()
    }

    /// This is the **TEST** version, used only for `cargo test`.
    /// It loads `config/test` instead of `config/default`.
    #[cfg(test)]
    pub fn load() -> Result<Self, config::ConfigError> {
        println!("🧪 Loading TEST configuration..."); // For demonstration
        let s = config::Config::builder()
            // Load the test-specific configuration file
            .add_source(config::File::with_name("config/test"))
            // You can still layer environment variables for testing if you need to
            .add_source(config::Environment::with_prefix("TASKMASTER"))
            .build()?;

        s.try_deserialize()
    }

    pub fn get_database_url(&self) -> &str {
        &self.data.database_url
    }

    pub fn get_server_address(&self) -> String {
        format!("{}:{}", self.server.host, self.server.port)
    }

    pub fn get_candidates_refresh_duration(&self) -> tokio::time::Duration {
        tokio::time::Duration::from_secs(self.candidates.refresh_interval_minutes * 60)
    }

    pub fn get_task_generation_duration(&self) -> tokio::time::Duration {
        tokio::time::Duration::from_secs(self.task_generation.generation_interval_minutes * 60)
    }

    pub fn get_reverser_check_duration(&self) -> tokio::time::Duration {
        tokio::time::Duration::from_secs(self.reverser.check_interval_seconds)
    }

    pub fn get_reversal_period_duration(&self) -> chrono::Duration {
        chrono::Duration::hours(self.blockchain.reversal_period_hours as i64)
    }

    pub fn get_early_reversal_duration(&self) -> chrono::Duration {
        chrono::Duration::minutes(self.reverser.early_reversal_minutes as i64)
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            server: ServerConfig {
                host: "127.0.0.1".to_string(),
                port: 3000,
            },
            blockchain: BlockchainConfig {
                node_url: "ws://127.0.0.1:9944".to_string(),
                wallet_name: "task_master_wallet".to_string(),
                wallet_password: "secure_password_change_me".to_string(),
                reversal_period_hours: 12,
            },
            candidates: CandidatesConfig {
                graphql_url: "http://localhost:4000/graphql".to_string(),
                refresh_interval_minutes: 30,
            },
            task_generation: TaskGenerationConfig {
                generation_interval_minutes: 60,
                taskees_per_round: 5,
            },
            reverser: ReverserConfig {
                early_reversal_minutes: 2,
                check_interval_seconds: 30,
            },
            data: DataConfig {
                database_url: "postgres://postgres:postgres@127.0.0.1:5432/task_master".to_string(),
            },
            logging: LoggingConfig {
                level: "info".to_string(),
            },
        }
    }
}
