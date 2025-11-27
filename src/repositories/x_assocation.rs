use sqlx::PgPool;

use crate::{
    models::{address::QuanAddress, x_association::XAssociation},
    repositories::DbResult,
};

#[derive(Clone, Debug)]
pub struct XAssociationRepository {
    pool: PgPool,
}

impl XAssociationRepository {
    pub fn new(pool: &PgPool) -> Self {
        Self { pool: pool.clone() }
    }

    pub async fn create(&self, new_association: &XAssociation) -> DbResult<XAssociation> {
        let association = sqlx::query_as::<_, XAssociation>(
            r#"
            INSERT INTO x_associations (quan_address, username) 
            VALUES ($1, $2)
            RETURNING quan_address, username, created_at
            "#,
        )
        .bind(&new_association.quan_address.0)
        .bind(&new_association.username)
        .fetch_one(&self.pool)
        .await?;

        Ok(association)
    }

    pub async fn find_by_address(&self, quan_address: &QuanAddress) -> DbResult<Option<XAssociation>> {
        let association = sqlx::query_as::<_, XAssociation>("SELECT * FROM x_associations WHERE quan_address = $1")
            .bind(&quan_address.0)
            .fetch_optional(&self.pool)
            .await?;

        Ok(association)
    }

    pub async fn find_by_username(&self, username: &str) -> DbResult<Option<XAssociation>> {
        let association = sqlx::query_as::<_, XAssociation>("SELECT * FROM x_associations WHERE username = $1")
            .bind(username)
            .fetch_optional(&self.pool)
            .await?;

        Ok(association)
    }

    pub async fn update_username(&self, quan_address: &QuanAddress, new_username: &str) -> DbResult<XAssociation> {
        let association = sqlx::query_as::<_, XAssociation>(
            r#"
            UPDATE x_associations 
            SET username = $2 
            WHERE quan_address = $1 
            RETURNING *
            "#,
        )
        .bind(&quan_address.0)
        .bind(new_username)
        .fetch_one(&self.pool)
        .await?;

        Ok(association)
    }

    pub async fn delete(&self, quan_address: &QuanAddress) -> DbResult<()> {
        sqlx::query("DELETE FROM x_associations WHERE quan_address = $1")
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
        models::x_association::{XAssociation, XAssociationInput},
        repositories::address::AddressRepository,
        utils::test_db::{create_persisted_address, reset_database},
    };
    use sqlx::PgPool;

    // Helper to set up test repositories.
    async fn setup_test_repositories() -> (AddressRepository, XAssociationRepository) {
        let config = Config::load_test_env().expect("Failed to load configuration for tests");
        let pool = PgPool::connect(config.get_database_url())
            .await
            .expect("Failed to create pool.");

        reset_database(&pool).await;

        (AddressRepository::new(&pool), XAssociationRepository::new(&pool))
    }

    #[tokio::test]
    async fn test_create_and_find_association() {
        let (address_repo, x_repo) = setup_test_repositories().await;

        // Create Parent Address
        let address = create_persisted_address(&address_repo, "user_01").await;

        // Create X Association
        let input = XAssociationInput {
            quan_address: address.quan_address.0.clone(),
            username: "x_user_01".to_string(),
        };
        let new_association = XAssociation::new(input).unwrap();

        let created = x_repo.create(&new_association).await.unwrap();

        // Check returned value
        assert_eq!(created.username, "x_user_01");
        assert!(created.created_at.is_some());

        // Verify by finding by Quan Address
        let found = x_repo.find_by_address(&address.quan_address).await.unwrap().unwrap();

        assert_eq!(found.quan_address.0, address.quan_address.0);
        assert_eq!(found.username, "x_user_01");
    }

    #[tokio::test]
    async fn test_find_by_username() {
        let (address_repo, x_repo) = setup_test_repositories().await;

        let address = create_persisted_address(&address_repo, "user_02").await;

        let input = XAssociationInput {
            quan_address: address.quan_address.0.clone(),
            username: "unique_handler_123".to_string(),
        };
        let new_association = XAssociation::new(input).unwrap();
        x_repo.create(&new_association).await.unwrap();

        // Find by Username
        let found = x_repo.find_by_username("unique_handler_123").await.unwrap();

        assert!(found.is_some());
        let found = found.unwrap();
        assert_eq!(found.quan_address.0, address.quan_address.0);
    }

    #[tokio::test]
    async fn test_update_username() {
        let (address_repo, x_repo) = setup_test_repositories().await;

        let address = create_persisted_address(&address_repo, "user_03").await;

        // Initial Create
        let input = XAssociationInput {
            quan_address: address.quan_address.0.clone(),
            username: "old_username".to_string(),
        };
        let new_association = XAssociation::new(input).unwrap();
        x_repo.create(&new_association).await.unwrap();

        // Update
        let updated = x_repo
            .update_username(&address.quan_address, "new_cool_username")
            .await
            .unwrap();

        assert_eq!(updated.username, "new_cool_username");

        // Verify in DB
        let found = x_repo.find_by_address(&address.quan_address).await.unwrap().unwrap();
        assert_eq!(found.username, "new_cool_username");
    }

    #[tokio::test]
    async fn test_delete_association() {
        let (address_repo, x_repo) = setup_test_repositories().await;

        let address = create_persisted_address(&address_repo, "user_04").await;

        let input = XAssociationInput {
            quan_address: address.quan_address.0.clone(),
            username: "to_be_deleted".to_string(),
        };
        let new_association = XAssociation::new(input).unwrap();
        x_repo.create(&new_association).await.unwrap();

        // Verify it exists
        assert!(x_repo.find_by_address(&address.quan_address).await.unwrap().is_some());

        // Delete
        x_repo.delete(&address.quan_address).await.unwrap();

        // Verify it is gone
        let found = x_repo.find_by_address(&address.quan_address).await.unwrap();
        assert!(found.is_none());
    }

    #[tokio::test]
    async fn test_find_non_existent() {
        let (_address_repo, x_repo) = setup_test_repositories().await;

        let result = x_repo.find_by_username("ghost_user").await.unwrap();
        assert!(result.is_none());
    }
}
