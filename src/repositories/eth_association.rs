use sqlx::PgPool;

use crate::{
    models::{address::QuanAddress, eth_association::EthAssociation},
    repositories::DbResult,
};

#[derive(Clone, Debug)]
pub struct EthAssociationRepository {
    pool: PgPool,
}

impl EthAssociationRepository {
    pub fn new(pool: &PgPool) -> Self {
        Self { pool: pool.clone() }
    }

    pub async fn create(&self, new_association: &EthAssociation) -> DbResult<EthAssociation> {
        let association = sqlx::query_as::<_, EthAssociation>(
            r#"
            INSERT INTO eth_associations (quan_address, eth_address) 
            VALUES ($1, $2)
            RETURNING quan_address, eth_address, created_at
            "#,
        )
        .bind(&new_association.quan_address.0)
        .bind(&new_association.eth_address.0)
        .fetch_one(&self.pool)
        .await?;

        Ok(association)
    }

    pub async fn find_by_quan_address(&self, quan_address: &QuanAddress) -> DbResult<Option<EthAssociation>> {
        let association = sqlx::query_as::<_, EthAssociation>("SELECT * FROM eth_associations WHERE quan_address = $1")
            .bind(&quan_address.0)
            .fetch_optional(&self.pool)
            .await?;

        Ok(association)
    }

    pub async fn update_eth_address(&self, new_association: &EthAssociation) -> DbResult<EthAssociation> {
        let association = sqlx::query_as::<_, EthAssociation>(
            r#"
            UPDATE eth_associations 
            SET eth_address = $2 
            WHERE quan_address = $1 
            RETURNING *
            "#,
        )
        .bind(&new_association.quan_address.0)
        .bind(&new_association.eth_address.0)
        .fetch_one(&self.pool)
        .await?;

        Ok(association)
    }

    pub async fn delete(&self, quan_address: &QuanAddress) -> DbResult<()> {
        sqlx::query("DELETE FROM eth_associations WHERE quan_address = $1")
            .bind(&quan_address.0)
            .execute(&self.pool)
            .await?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        config::Config,
        models::eth_association::EthAssociationInput,
        repositories::address::AddressRepository,
        utils::test_db::{create_persisted_address, reset_database},
    };
    use sqlx::PgPool;

    // Helper to set up test repositories.
    async fn setup_test_repositories() -> (AddressRepository, EthAssociationRepository) {
        let config = Config::load_test_env().expect("Failed to load configuration for tests");
        let pool = PgPool::connect(config.get_database_url())
            .await
            .expect("Failed to create pool.");

        reset_database(&pool).await;

        (AddressRepository::new(&pool), EthAssociationRepository::new(&pool))
    }

    #[tokio::test]
    async fn test_create_and_find_association() {
        let (address_repo, eth_repo) = setup_test_repositories().await;

        // Create Parent Address
        let address = create_persisted_address(&address_repo, "user_01").await;

        // Create ETH Association
        let input = EthAssociationInput {
            quan_address: address.quan_address.0.clone(),
            eth_address: "0x00000000219ab540356cBB839Cbe05303d7705Fa".to_string(),
        };
        let new_association = EthAssociation::new(input).unwrap();

        let created = eth_repo.create(&new_association).await.unwrap();

        // Check returned value
        assert_eq!(created.eth_address.0, "0x00000000219ab540356cBB839Cbe05303d7705Fa");
        assert!(created.created_at.is_some());

        // Verify by finding by Quan Address
        let found = eth_repo
            .find_by_quan_address(&address.quan_address)
            .await
            .unwrap()
            .unwrap();

        assert_eq!(found.quan_address.0, address.quan_address.0);
        assert_eq!(found.eth_address.0, "0x00000000219ab540356cBB839Cbe05303d7705Fa");
    }

    #[tokio::test]
    async fn test_update_eth_address() {
        let (address_repo, eth_repo) = setup_test_repositories().await;

        let address = create_persisted_address(&address_repo, "user_03").await;

        // Initial Create
        let initial_input = EthAssociationInput {
            quan_address: address.quan_address.0.clone(),
            eth_address: "0x00000000219ab540356cBB839Cbe05303d7705Fa".to_string(),
        };
        let initial_association = EthAssociation::new(initial_input).unwrap();
        eth_repo.create(&initial_association).await.unwrap();

        // Update
        let new_input = EthAssociationInput {
            quan_address: address.quan_address.0.clone(),
            eth_address: "0xC02aaA39b223FE8D0A0e5C4F27eAD9083C756Cc2".to_string(),
        };
        let new_association = EthAssociation::new(new_input).unwrap();
        let updated = eth_repo.update_eth_address(&new_association).await.unwrap();

        assert_eq!(updated.eth_address.0, new_association.eth_address.0);

        // Verify in DB
        let found = eth_repo
            .find_by_quan_address(&address.quan_address)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(found.eth_address.0, new_association.eth_address.0);
    }

    #[tokio::test]
    async fn test_delete_association() {
        let (address_repo, eth_repo) = setup_test_repositories().await;

        let address = create_persisted_address(&address_repo, "user_04").await;

        let input = EthAssociationInput {
            quan_address: address.quan_address.0.clone(),
            eth_address: "0x00000000219ab540356cBB839Cbe05303d7705Fa".to_string(),
        };
        let new_association = EthAssociation::new(input).unwrap();
        eth_repo.create(&new_association).await.unwrap();

        // Verify it exists
        assert!(eth_repo
            .find_by_quan_address(&address.quan_address)
            .await
            .unwrap()
            .is_some());

        // Delete
        eth_repo.delete(&address.quan_address).await.unwrap();

        // Verify it is gone
        let found = eth_repo.find_by_quan_address(&address.quan_address).await.unwrap();
        assert!(found.is_none());
    }
}
