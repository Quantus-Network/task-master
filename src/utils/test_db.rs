use sqlx::PgPool;

use crate::{
    models::address::{Address, AddressInput},
    repositories::address::AddressRepository,
};

pub async fn reset_database(pool: &PgPool) {
    sqlx::query("TRUNCATE tasks, referrals, opt_ins, addresses RESTART IDENTITY CASCADE")
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
