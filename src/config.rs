use rusx::config::OauthConfig;
use serde::{Deserialize, Serialize};
use tokio::time;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub server: ServerConfig,
    pub blockchain: BlockchainConfig,
    pub candidates: CandidatesConfig,
    pub data: DataConfig,
    pub logging: LoggingConfig,
    pub jwt: JwtConfig,
    pub x_oauth: OauthConfig,
    pub tweet_sync: TweetSyncConfig,
    pub tg_bot: TelegramBotConfig,
    pub raid_leaderboard: RaidLeaderboardConfig,
    pub alert: AlertConfig,
    pub x_association: XAssociationConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerConfig {
    pub host: String,
    pub port: u16,
    pub base_api_url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlockchainConfig {
    pub website_url: String,
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
pub struct TweetSyncConfig {
    pub interval_in_hours: u64,
    pub keywords: String,
    pub api_key: String,
    pub monthly_limit: u32,
    pub alert_threshold: u32,
    pub reset_day: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TelegramBotConfig {
    pub base_url: String,
    pub token: String,
    pub chat_id: String,
    pub message_thread_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RaidLeaderboardConfig {
    pub sync_interval_in_hours: u64,
    pub tweets_req_interval_in_secs: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlertConfig {
    pub webhook_url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct XAssociationConfig {
    pub keywords: String,
}

impl Config {
    pub fn load(config_path: &str) -> Result<Self, config::ConfigError> {
        let settings = config::Config::builder()
            .add_source(config::File::new(config_path, config::FileFormat::Toml))
            .add_source(config::Environment::with_prefix("TASKMASTER"))
            .build()?;

        settings.try_deserialize()
    }

    #[cfg(test)]
    pub fn load_test_env() -> Result<Self, config::ConfigError> {
        println!("Loading TEST configuration..."); // For demonstration
        let settings = config::Config::builder()
            // Load the test-specific configuration file
            .add_source(config::File::with_name("config/test"))
            // You can still layer environment variables for testing if you need to
            .add_source(config::Environment::with_prefix("TASKMASTER"))
            .build()?;

        settings.try_deserialize()
    }

    pub fn get_database_url(&self) -> &str {
        &self.data.database_url
    }

    pub fn get_server_address(&self) -> String {
        format!("{}:{}", self.server.host, self.server.port)
    }

    pub fn get_base_api_url(&self) -> &str {
        &self.server.base_api_url
    }

    pub fn get_jwt_expiration(&self) -> chrono::Duration {
        chrono::Duration::hours(self.jwt.exp_in_hours)
    }

    pub fn get_tweet_sync_interval(&self) -> time::Duration {
        time::Duration::from_secs(self.tweet_sync.interval_in_hours * 3600)
    }

    pub fn get_raid_leaderboard_sync_interval(&self) -> time::Duration {
        time::Duration::from_secs(self.raid_leaderboard.sync_interval_in_hours * 3600)
    }

    pub fn get_raid_leaderboard_tweets_req_interval(&self) -> time::Duration {
        time::Duration::from_secs(self.raid_leaderboard.tweets_req_interval_in_secs)
    }

    pub fn get_x_association_keywords(&self) -> &str {
        &self.x_association.keywords
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            server: ServerConfig {
                host: "127.0.0.1".to_string(),
                port: 3000,
                base_api_url: "http://127.0.0.1:3000/api".to_string(),
            },
            blockchain: BlockchainConfig {
                website_url: "https://www.quantus.com".to_string(),
                node_url: "ws://127.0.0.1:9944".to_string(),
                wallet_name: "task_master_wallet".to_string(),
                wallet_password: "secure_password_change_me".to_string(),
                reversal_period_hours: 12,
            },
            candidates: CandidatesConfig {
                graphql_url: "http://localhost:4000/graphql".to_string(),
                refresh_interval_minutes: 30,
            },
            data: DataConfig {
                database_url: "postgres://postgres:postgres@127.0.0.1:5432/task_master".to_string(),
            },
            logging: LoggingConfig {
                level: "info".to_string(),
            },
            jwt: JwtConfig {
                admin_secret: "Also-change-in-production".to_string(),
                secret: "Change-in-production".to_string(),
                exp_in_hours: 24,
            },
            x_oauth: OauthConfig {
                callback_url: "example".to_string(),
                client_id: "example".to_string(),
                client_secret: "example".to_string(),
            },
            tweet_sync: TweetSyncConfig {
                interval_in_hours: 24,
                keywords: "hello".to_string(),
                api_key: "key".to_string(),
                monthly_limit: 15000,
                alert_threshold: 13000,
                reset_day: 1,
            },
            tg_bot: TelegramBotConfig {
                base_url: "https://api.telegram.org".to_string(),
                chat_id: "-0".to_string(),
                message_thread_id: Some("-0".to_string()),
                token: "token".to_string(),
            },
            raid_leaderboard: RaidLeaderboardConfig {
                sync_interval_in_hours: 24,
                tweets_req_interval_in_secs: 60,
            },
            alert: AlertConfig {
                webhook_url: "https://your-webhook-url.com".to_string(),
            },
            x_association: XAssociationConfig {
                keywords: "quantus".to_string(),
            },
        }
    }
}
