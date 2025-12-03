use argon2::{
    password_hash::{rand_core::OsRng, PasswordHasher, SaltString},
    Argon2,
};

use clap::Parser;
use sqlx::Row;
use std::io::{self, Write};
use task_master::{args::Args, db_persistence::DbPersistence, AppError, Config};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();

    let config = Config::load(&args.config).map_err(AppError::Config)?;

    let db = DbPersistence::new(config.get_database_url()).await?;

    println!("--- Create Admin Account ---");

    print!("Enter Username: ");
    io::stdout().flush()?;
    let mut username = String::new();
    io::stdin().read_line(&mut username)?;
    let username = username.trim();

    print!("Enter Password: ");
    io::stdout().flush()?;
    let mut password = String::new();
    io::stdin().read_line(&mut password)?;
    let password = password.trim();

    if password.is_empty() {
        eprintln!("Error: Password cannot be empty.");
        return Ok(());
    }

    println!("Hashing password...");
    let salt = SaltString::generate(&mut OsRng);
    // Uses default Argon2id. Must match your login handler's config!
    let argon2 = Argon2::default();
    let password_hash = argon2.hash_password(password.as_bytes(), &salt)?.to_string();

    println!("Inserting admin into database...");

    let result = sqlx::query(
        r#"
        INSERT INTO admins (username, password)
        VALUES ($1, $2)
        RETURNING id
        "#,
    )
    .bind(username)
    .bind(password_hash)
    .fetch_one(&db.pool)
    .await;

    match result {
        Ok(row) => {
            // Assuming your ID is a UUID. If it's an INT, change to: row.try_get::<i32, _>("id")?
            let id: uuid::Uuid = row.try_get("id")?;
            println!("✅ Success! Admin created.");
            println!("ID: {}", id);
            println!("Username: {}", username);
        }
        Err(e) => {
            if e.to_string().contains("duplicate key") || e.to_string().contains("unique constraint") {
                eprintln!("❌ Error: Username '{}' already exists.", username);
            } else {
                eprintln!("❌ Database Error: {}", e);
            }
        }
    }

    Ok(())
}
