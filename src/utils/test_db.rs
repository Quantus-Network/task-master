use chrono::Utc;
use sqlx::PgPool;
use uuid::Uuid;

use crate::{
    models::{
        address::{Address, AddressInput},
        admin::Admin,
    },
    repositories::address::AddressRepository,
};

pub async fn reset_database(pool: &PgPool) {
    sqlx::query("TRUNCATE referrals, opt_ins, addresses, admins, eth_associations, x_associations, relevant_tweets, tweet_authors, raid_quests, raid_submissions, tweet_pull_usage RESTART IDENTITY CASCADE")
        .execute(pool)
        .await
        .expect("Failed to truncate tables for tests");
}

pub async fn create_persisted_address(repo: &AddressRepository, id: &str) -> Address {
    let input = AddressInput {
        quan_address: format!("qz_test_address_{}", id),
        referral_code: format!("REF{}", id),
    };
    let address = Address::new(input).unwrap();
    repo.create(&address).await.unwrap();
    address
}

pub async fn create_persisted_opt_in(pool: &PgPool, quan_address: &str) {
    sqlx::query("INSERT INTO opt_ins (quan_address) VALUES ($1)")
        .bind(quan_address)
        .execute(pool)
        .await
        .expect("Failed to create opt-in");
}

pub async fn create_persisted_x_association(pool: &PgPool, quan_address: &str, username: &str) {
    sqlx::query("INSERT INTO x_associations (quan_address, username) VALUES ($1, $2)")
        .bind(quan_address)
        .bind(username)
        .execute(pool)
        .await
        .expect("Failed to create x association");
}

pub async fn create_persisted_eth_association(pool: &PgPool, quan_address: &str, eth_address: &str) {
    sqlx::query("INSERT INTO eth_associations (quan_address, eth_address) VALUES ($1, $2)")
        .bind(quan_address)
        .bind(eth_address)
        .execute(pool)
        .await
        .expect("Failed to create eth association");
}

pub fn create_mock_admin() -> Admin {
    Admin {
        id: Uuid::new_v4(),
        username: "admin_tester".to_string(),
        password: "hash".to_string(),
        updated_at: Utc::now(),
        created_at: Utc::now(),
    }
}
