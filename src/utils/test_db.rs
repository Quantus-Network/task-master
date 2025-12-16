use chrono::Utc;
use sqlx::PgPool;
use uuid::Uuid;

use crate::{
    models::{
        address::{Address, AddressInput},
        admin::Admin,
        eth_association::{EthAssociation, EthAssociationInput},
        x_association::{XAssociation, XAssociationInput},
    },
    repositories::{
        address::AddressRepository, eth_association::EthAssociationRepository, x_association::XAssociationRepository,
    },
};

pub async fn reset_database(pool: &PgPool) {
    sqlx::query("TRUNCATE tasks, referrals, opt_ins, addresses, admins, eth_associations, x_associations, relevant_tweets, tweet_authors, raid_quests, raid_submissions RESTART IDENTITY CASCADE")
        .execute(pool)
        .await
        .expect("Failed to truncate tables for tests");

    // Refresh the materialized view to clear the stale snapshot
    // Since the source tables are now empty, this will result in an empty view.
    sqlx::query("REFRESH MATERIALIZED VIEW raid_leaderboards")
        .execute(pool)
        .await
        .expect("Failed to refresh materialized view for tests");
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

pub fn create_mock_admin() -> Admin {
    Admin {
        id: Uuid::new_v4(),
        username: "admin_tester".to_string(),
        password: "hash".to_string(),
        updated_at: Utc::now(),
        created_at: Utc::now(),
    }
}
