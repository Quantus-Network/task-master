use sqlx::PgPool;

use crate::{models::opt_in::OptIn, repositories::DbResult};

#[derive(Clone, Debug)]
pub struct OptInRepository {
    pool: PgPool,
}

impl OptInRepository {
    pub fn new(pool: &PgPool) -> Self {
        Self { pool: pool.clone() }
    }

    pub async fn create(&self, quan_address: &str) -> DbResult<OptIn> {
        let opt_in = sqlx::query_as::<_, OptIn>(
            r#"
            INSERT INTO opt_ins (quan_address)
            VALUES ($1)
            ON CONFLICT (quan_address) DO UPDATE
            SET quan_address = EXCLUDED.quan_address
            RETURNING *
            "#,
        )
        .bind(quan_address)
        .fetch_one(&self.pool)
        .await?;

        Ok(opt_in)
    }

    pub async fn delete(&self, quan_address: &str) -> DbResult<()> {
        sqlx::query("DELETE FROM opt_ins WHERE quan_address = $1")
            .bind(quan_address)
            .execute(&self.pool)
            .await?;

        Ok(())
    }

    pub async fn find_by_address(&self, quan_address: &str) -> DbResult<Option<OptIn>> {
        let opt_in = sqlx::query_as::<_, OptIn>("SELECT * FROM opt_ins WHERE quan_address = $1")
            .bind(quan_address)
            .fetch_optional(&self.pool)
            .await?;

        Ok(opt_in)
    }

    pub async fn get_all_ordered(&self, limit: i64) -> DbResult<Vec<OptIn>> {
        let opt_ins = sqlx::query_as::<_, OptIn>("SELECT * FROM opt_ins ORDER BY created_at ASC LIMIT $1")
            .bind(limit)
            .fetch_all(&self.pool)
            .await?;

        Ok(opt_ins)
    }

    pub async fn count(&self) -> DbResult<i64> {
        let count = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM opt_ins")
            .fetch_one(&self.pool)
            .await?;

        Ok(count)
    }
}

#[cfg(test)]
mod tests {
    // NOTE: These tests must run sequentially (--test-threads=1) to avoid database conflicts
    // Run with: cargo test --lib opt_in -- --test-threads=1
    use super::*;
    use crate::config::Config;
    use crate::models::address::{Address, AddressInput};
    use crate::repositories::address::AddressRepository;
    use crate::utils::test_db::reset_database;
    use sqlx::{postgres::PgPoolOptions, PgPool};
    use std::time::Duration;
    use tokio::time::sleep;

    async fn setup_test_repository() -> (OptInRepository, AddressRepository, PgPool) {
        let config = Config::load_test_env().expect("Failed to load configuration for tests");
        let pool = PgPoolOptions::new()
            .max_connections(5)
            .connect(config.get_database_url())
            .await
            .expect("Failed to create pool.");

        reset_database(&pool).await;

        let opt_in_repo = OptInRepository::new(&pool);
        let address_repo = AddressRepository::new(&pool);

        (opt_in_repo, address_repo, pool)
    }

    fn create_test_address(id: &str) -> Address {
        let input = AddressInput {
            quan_address: format!("qz_test_{}", id),
            eth_address: None,
            referral_code: format!("ref_{}", id),
        };
        Address::new(input).unwrap()
    }

    #[tokio::test]
    async fn test_create_and_find_by_address() {
        let (opt_in_repo, address_repo, _pool) = setup_test_repository().await;
        let address = create_test_address("test_create_001");

        address_repo.create(&address).await.unwrap();
        let count = opt_in_repo.count().await.unwrap();
        let opt_in = opt_in_repo.create(&address.quan_address.0).await.unwrap();

        assert_eq!(opt_in.quan_address.0, address.quan_address.0);
        assert_eq!(opt_in.opt_in_number, (count + 1) as i32);

        let found = opt_in_repo
            .find_by_address(&address.quan_address.0)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(found.quan_address.0, address.quan_address.0);
        assert_eq!(found.opt_in_number, (count + 1) as i32);
    }

    #[tokio::test]
    async fn test_delete() {
        let (opt_in_repo, address_repo, _pool) = setup_test_repository().await;
        let address = create_test_address("test_delete_001");

        address_repo.create(&address).await.unwrap();
        let count_before = opt_in_repo.count().await.unwrap();
        opt_in_repo.create(&address.quan_address.0).await.unwrap();

        assert!(opt_in_repo
            .find_by_address(&address.quan_address.0)
            .await
            .unwrap()
            .is_some());

        opt_in_repo.delete(&address.quan_address.0).await.unwrap();

        assert!(opt_in_repo
            .find_by_address(&address.quan_address.0)
            .await
            .unwrap()
            .is_none());
    }

    #[tokio::test]
    async fn test_get_all_ordered() {
        let (opt_in_repo, address_repo, _pool) = setup_test_repository().await;

        let addr1 = create_test_address("test_ordered_001");
        let addr2 = create_test_address("test_ordered_002");
        let addr3 = create_test_address("test_ordered_003");

        address_repo.create(&addr1).await.unwrap();
        address_repo.create(&addr2).await.unwrap();
        address_repo.create(&addr3).await.unwrap();

        let count = opt_in_repo.count().await.unwrap();
        opt_in_repo.create(&addr1.quan_address.0).await.unwrap();
        sleep(Duration::from_millis(10)).await;

        let count = opt_in_repo.count().await.unwrap();
        opt_in_repo.create(&addr2.quan_address.0).await.unwrap();
        sleep(Duration::from_millis(10)).await;

        let count = opt_in_repo.count().await.unwrap();
        opt_in_repo.create(&addr3.quan_address.0).await.unwrap();

        let all = opt_in_repo.get_all_ordered(100).await.unwrap();
        assert_eq!(all.len(), 3);
        assert_eq!(all[0].quan_address.0, addr1.quan_address.0);
        assert_eq!(all[1].quan_address.0, addr2.quan_address.0);
        assert_eq!(all[2].quan_address.0, addr3.quan_address.0);
    }

    #[tokio::test]
    async fn test_get_all_ordered_with_limit() {
        let (opt_in_repo, address_repo, _pool) = setup_test_repository().await;

        let addr1 = create_test_address("test_limit_001");
        let addr2 = create_test_address("test_limit_002");
        let addr3 = create_test_address("test_limit_003");

        address_repo.create(&addr1).await.unwrap();
        address_repo.create(&addr2).await.unwrap();
        address_repo.create(&addr3).await.unwrap();

        let count = opt_in_repo.count().await.unwrap();
        opt_in_repo.create(&addr1.quan_address.0).await.unwrap();
        sleep(Duration::from_millis(10)).await;

        let count = opt_in_repo.count().await.unwrap();
        opt_in_repo.create(&addr2.quan_address.0).await.unwrap();
        sleep(Duration::from_millis(10)).await;

        let count = opt_in_repo.count().await.unwrap();
        opt_in_repo.create(&addr3.quan_address.0).await.unwrap();

        let limited = opt_in_repo.get_all_ordered(2).await.unwrap();
        assert_eq!(limited.len(), 2);
        assert_eq!(limited[0].quan_address.0, addr1.quan_address.0);
        assert_eq!(limited[1].quan_address.0, addr2.quan_address.0);
    }

    #[tokio::test]
    async fn test_count() {
        let (opt_in_repo, address_repo, _pool) = setup_test_repository().await;

        assert_eq!(opt_in_repo.count().await.unwrap(), 0);

        let addr1 = create_test_address("test_count_001");
        let addr2 = create_test_address("test_count_002");
        let addr3 = create_test_address("test_count_003");

        address_repo.create(&addr1).await.unwrap();
        address_repo.create(&addr2).await.unwrap();
        address_repo.create(&addr3).await.unwrap();

        let count = opt_in_repo.count().await.unwrap();
        opt_in_repo.create(&addr1.quan_address.0).await.unwrap();
        assert_eq!(opt_in_repo.count().await.unwrap(), 1);

        let count = opt_in_repo.count().await.unwrap();
        opt_in_repo.create(&addr2.quan_address.0).await.unwrap();
        assert_eq!(opt_in_repo.count().await.unwrap(), 2);

        let count = opt_in_repo.count().await.unwrap();
        opt_in_repo.create(&addr3.quan_address.0).await.unwrap();
        assert_eq!(opt_in_repo.count().await.unwrap(), 3);

        opt_in_repo.delete(&addr2.quan_address.0).await.unwrap();
        assert_eq!(opt_in_repo.count().await.unwrap(), 2);
    }

    #[tokio::test]
    async fn test_create_duplicate_updates_existing() {
        let (opt_in_repo, address_repo, _pool) = setup_test_repository().await;
        let address = create_test_address("test_duplicate_001");

        address_repo.create(&address).await.unwrap();

        let count = opt_in_repo.count().await.unwrap();
        let opt_in1 = opt_in_repo.create(&address.quan_address.0).await.unwrap();
        let first_created_at = opt_in1.created_at;

        sleep(Duration::from_millis(10)).await;

        let count = opt_in_repo.count().await.unwrap();
        let opt_in2 = opt_in_repo.create(&address.quan_address.0).await.unwrap();

        assert_eq!(opt_in2.quan_address.0, address.quan_address.0);
        assert_eq!(opt_in2.opt_in_number, opt_in1.opt_in_number);
        assert_eq!(opt_in2.created_at, first_created_at);
    }

    #[tokio::test]
    async fn test_find_by_address_not_found() {
        let (opt_in_repo, _address_repo, _pool) = setup_test_repository().await;

        let result = opt_in_repo.find_by_address("qz_nonexistent").await.unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_delete_nonexistent_no_error() {
        let (opt_in_repo, _address_repo, _pool) = setup_test_repository().await;

        let result = opt_in_repo.delete("qz_nonexistent").await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_sequential_opt_in_numbering() {
        let (opt_in_repo, address_repo, _pool) = setup_test_repository().await;

        let addr1 = create_test_address("test_seq_001");
        let addr2 = create_test_address("test_seq_002");
        let addr3 = create_test_address("test_seq_003");

        address_repo.create(&addr1).await.unwrap();
        address_repo.create(&addr2).await.unwrap();
        address_repo.create(&addr3).await.unwrap();

        assert_eq!(opt_in_repo.count().await.unwrap(), 0);

        let count = opt_in_repo.count().await.unwrap();
        let opt_in1 = opt_in_repo.create(&addr1.quan_address.0).await.unwrap();
        assert_eq!(opt_in1.opt_in_number, 1);
        assert_eq!(opt_in_repo.count().await.unwrap(), 1);

        let count = opt_in_repo.count().await.unwrap();
        let opt_in2 = opt_in_repo.create(&addr2.quan_address.0).await.unwrap();
        assert_eq!(opt_in2.opt_in_number, 2);
        assert_eq!(opt_in_repo.count().await.unwrap(), 2);

        let count = opt_in_repo.count().await.unwrap();
        let opt_in3 = opt_in_repo.create(&addr3.quan_address.0).await.unwrap();
        assert_eq!(opt_in3.opt_in_number, 3);
        assert_eq!(opt_in_repo.count().await.unwrap(), 3);

        let all = opt_in_repo.get_all_ordered(100).await.unwrap();
        assert_eq!(all[0].opt_in_number, 1);
        assert_eq!(all[1].opt_in_number, 2);
        assert_eq!(all[2].opt_in_number, 3);
    }

    #[tokio::test]
    async fn test_opt_in_timestamps_are_set() {
        let (opt_in_repo, address_repo, _pool) = setup_test_repository().await;
        let address = create_test_address("test_timestamp_001");

        address_repo.create(&address).await.unwrap();
        let count = opt_in_repo.count().await.unwrap();
        let opt_in = opt_in_repo.create(&address.quan_address.0).await.unwrap();

        assert!(!opt_in.created_at.to_rfc3339().is_empty());
    }
}
