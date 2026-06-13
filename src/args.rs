use clap::Parser;

#[derive(Parser, Debug)]
#[command(name = "task-master")]
#[command(about = "Task management server")]
pub struct Args {
    /// Configuration file path
    #[arg(short, long, default_value = "config/default.toml")]
    pub config: String,

    /// Sync transfers from GraphQL and store addresses
    #[arg(long)]
    pub sync_transfers: bool,
}
