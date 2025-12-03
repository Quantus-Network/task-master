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

    /// Run once and exit (for testing)
    #[arg(long)]
    pub run_once: bool,

    /// Sync transfers from GraphQL and store addresses
    #[arg(long)]
    pub sync_transfers: bool,

    /// Test address selection from database
    #[arg(long)]
    pub test_selection: bool,

    /// Test sending a reversible transaction
    #[arg(long)]
    pub test_transaction: bool,

    /// Destination address for test transaction
    #[arg(long, requires = "test_transaction")]
    pub destination: Option<String>,

    /// Amount for test transaction (in QUAN units)
    #[arg(long, requires = "test_transaction")]
    pub amount: Option<u64>,
}
