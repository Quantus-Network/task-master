use clap::Parser;

#[derive(Parser, Debug)]
#[command(name = "task-master")]
#[command(about = "Task management server with reversible blockchain transactions")]
pub struct Args {
    /// Configuration file path
    #[arg(short, long, default_value = "config/default.toml")]
    pub config: String,

    /// Wallet name override
    #[arg(long)]
    pub wallet_name: Option<String>,

    /// Wallet password override
    #[arg(long)]
    pub wallet_password: Option<String>,

    /// Node URL override
    #[arg(long)]
    pub node_url: Option<String>,

    /// Sync transfers from GraphQL and store addresses
    #[arg(long)]
    pub sync_transfers: bool,
}
