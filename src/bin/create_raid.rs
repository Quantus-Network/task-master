use clap::Parser;
use std::io::{self, Write};
use task_master::{args::Args, db_persistence::DbPersistence, models::raid_quest::CreateRaidQuest, AppError, Config};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();

    let config = Config::load(&args.config).map_err(AppError::Config)?;

    let db = DbPersistence::new(config.get_database_url()).await?;

    println!("--- Create Raid Quest ---");

    print!("Enter Raid Name: ");
    io::stdout().flush()?;
    let mut name = String::new();
    io::stdin().read_line(&mut name)?;
    let name = name.trim();

    println!("Inserting raid into database...");
    let new_quest = CreateRaidQuest { name: name.to_string() };

    let result = db.raid_quests.create(&new_quest).await;

    match result {
        Ok(id) => {
            println!("✅ Success! Admin created.");
            println!("ID: {}", id);
            println!("Name: {}", name);
        }
        Err(e) => {
            eprintln!("❌ Database Error: {}", e);
        }
    }

    Ok(())
}
