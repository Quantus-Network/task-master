use sqlx::PgPool;

use crate::{
    models::{
        address::{Address, AddressInput},
        eth_association::{EthAssociation, EthAssociationInput},
        x_association::{XAssociation, XAssociationInput},
    },
    repositories::{
        address::AddressRepository, eth_association::EthAssociationRepository, x_association::XAssociationRepository,
    },
};

pub async fn reset_database(pool: &PgPool) {
    sqlx::query("TRUNCATE tasks, referrals, opt_ins, addresses, admins, eth_associations, x_associations, raid_quests, raid_submissions RESTART IDENTITY CASCADE")
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

pub async fn create_persisted_x_association(
    repo: &XAssociationRepository,
    address: &str,
    username: &str,
) -> XAssociation {
    let input = XAssociationInput {
        quan_address: address.to_string(),
        username: username.to_string(),
    };
    let new_association = XAssociation::new(input).unwrap();

    repo.create(&new_association).await.unwrap();

    new_association
}

pub async fn create_persisted_eth_association(
    repo: &EthAssociationRepository,
    quan_address: &str,
    eth_address: &str,
) -> EthAssociation {
    let input = EthAssociationInput {
        quan_address: quan_address.to_string(),
        eth_address: eth_address.to_string(),
    };
    let new_association = EthAssociation::new(input).unwrap();

    repo.create(&new_association).await.unwrap();

    new_association
}
