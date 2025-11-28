use sqlx::{PgPool, QueryBuilder};

use crate::{
    models::address::{Address, AddressWithRank},
    repositories::DbResult,
};

#[derive(Clone, Debug)]
pub struct AddressRepository {
    pool: PgPool,
}
impl AddressRepository {
    fn push_leaderboard_base_query<'a>(qb: &mut QueryBuilder<'a, sqlx::Postgres>) {
        qb.push(" FROM addresses WHERE referrals_count > 0");
    }

    fn push_leaderboard_filter_query_if_possible<'a>(
        qb: &mut QueryBuilder<'a, sqlx::Postgres>,
        with_referral_code: Option<String>,
    ) {
        if let Some(code) = with_referral_code {
            qb.push(" AND referral_code ILIKE ").push_bind(format!("{}%", code));
        }
    }

    pub fn new(pool: &PgPool) -> Self {
        Self { pool: pool.clone() }
    }

    pub async fn create(&self, new_address: &Address) -> DbResult<String> {
        let created_id = sqlx::query_scalar::<_, String>(
            "
        INSERT INTO addresses (quan_address, referral_code, referrals_count) 
        VALUES ($1, $2, $3)
        ON CONFLICT (quan_address) 
        DO UPDATE SET quan_address = EXCLUDED.quan_address
        RETURNING quan_address
        ",
        )
        .bind(new_address.quan_address.0.clone())
        .bind(new_address.referral_code.clone())
        .bind(new_address.referrals_count)
        .fetch_optional(&self.pool)
        .await?;

        if let Some(id) = created_id {
            Ok(id)
        } else {
            let existing_address = self.find_by_id(&new_address.quan_address.0).await?.unwrap();
            Ok(existing_address.quan_address.0)
        }
    }

    pub async fn create_many(&self, addresses: Vec<Address>) -> DbResult<u64> {
        if addresses.is_empty() {
            return Ok(0);
        }

        // Manually deconstruct the Vec<Address> into four separate vectors.
        // This is the fix for the `unzip` limitation.
        let mut quan_addresses = Vec::with_capacity(addresses.len());
        let mut referral_codes = Vec::with_capacity(addresses.len());
        let mut referrals_counts = Vec::with_capacity(addresses.len());

        for address in addresses {
            quan_addresses.push(address.quan_address.0);
            referral_codes.push(address.referral_code);
            referrals_counts.push(address.referrals_count);
        }

        let result = sqlx::query(
            r#"
        INSERT INTO addresses (quan_address, referral_code, referrals_count)
        SELECT * FROM UNNEST($1, $2, $3)
        ON CONFLICT (quan_address) DO NOTHING
        "#,
        )
        .bind(&quan_addresses)
        .bind(&referral_codes)
        .bind(&referrals_counts)
        .execute(&self.pool)
        .await?;

        Ok(result.rows_affected())
    }

    pub async fn find_by_id(&self, id: &str) -> DbResult<Option<Address>> {
        let address = sqlx::query_as::<_, Address>("SELECT * FROM addresses WHERE quan_address = $1")
            .bind(id)
            .fetch_optional(&self.pool)
            .await?;

        Ok(address)
    }

    pub async fn find_by_referral_code(&self, referral_code: &str) -> DbResult<Option<Address>> {
        let address = sqlx::query_as::<_, Address>("SELECT * FROM addresses WHERE referral_code = $1")
            .bind(referral_code)
            .fetch_optional(&self.pool)
            .await?;

        Ok(address)
    }

    pub async fn get_total_items(&self, with_referral_code: Option<String>) -> DbResult<i64> {
        let mut qb = QueryBuilder::new("SELECT COUNT(*) ");
        AddressRepository::push_leaderboard_base_query(&mut qb);
        AddressRepository::push_leaderboard_filter_query_if_possible(&mut qb, with_referral_code);

        let total_items = qb.build_query_scalar().fetch_one(&self.pool).await?;

        Ok(total_items)
    }

    pub async fn find_all(&self) -> DbResult<Vec<Address>> {
        let addresses = sqlx::query_as::<_, Address>("SELECT * FROM addresses")
            .fetch_all(&self.pool)
            .await?;

        Ok(addresses)
    }

    pub async fn get_leaderboard_entries(
        &self,
        page_size: u32,
        offset: u32,
        with_referral_code: Option<String>,
    ) -> DbResult<Vec<AddressWithRank>> {
        let mut qb = QueryBuilder::new(
            "WITH ranked_addresses AS (
            SELECT 
                *,
                ROW_NUMBER() OVER (ORDER BY referrals_count DESC) as rank
        ",
        );

        AddressRepository::push_leaderboard_base_query(&mut qb);
        qb.push(") SELECT * FROM ranked_addresses WHERE 1=1");

        AddressRepository::push_leaderboard_filter_query_if_possible(&mut qb, with_referral_code);
        qb.push(" ORDER BY rank LIMIT ")
            .push_bind(page_size as i64)
            .push(" OFFSET ")
            .push_bind(offset as i64);

        let query = qb.build_query_as::<AddressWithRank>();

        let addresses = query.fetch_all(&self.pool).await?;

        Ok(addresses)
    }

    pub async fn increment_referrals_count(&self, quan_address: &str) -> DbResult<i32> {
        let new_count = sqlx::query_scalar::<_, i32>(
            r#"
        UPDATE addresses
        SET referrals_count = referrals_count + 1
        WHERE quan_address = $1
        RETURNING referrals_count
        "#,
        )
        .bind(quan_address)
        .fetch_one(&self.pool)
        .await?;

        Ok(new_count)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use crate::models::address::{Address, AddressInput};
    use crate::utils::test_db::reset_database;
    use sqlx::PgPool;

    // Helper function to set up a test repository using the app's config loader.
    // Note: This requires a `config/test.toml` file or equivalent environment
    // variables (e.g., `TASKMASTER_DATA_DATABASE_URL`) for the tests to run.
    async fn setup_test_repository() -> AddressRepository {
        let config = Config::load_test_env().expect("Failed to load configuration for tests");
        let pool = PgPool::connect(config.get_database_url())
            .await
            .expect("Failed to create pool.");

        // Clean database before each test
        reset_database(&pool).await;

        AddressRepository::new(&pool)
    }

    fn create_mock_address(id: &str, code: &str) -> Address {
        let input = AddressInput {
            quan_address: format!("qz_test_address_{}", id),
            referral_code: code.to_string(),
        };
        Address::new(input).unwrap()
    }

    fn create_mock_address_with_referrals_count(id: &str, code: &str, referrals_count: i32) -> Address {
        let input = AddressInput {
            quan_address: format!("qz_test_address_{}", id),
            referral_code: code.to_string(),
        };

        let mut address = Address::new(input).unwrap();
        address.referrals_count = referrals_count;

        address
    }

    #[tokio::test]
    async fn test_create_and_find_by_id() {
        let repo = setup_test_repository().await;
        let address = create_mock_address("001", "REF001");

        let created_id = repo.create(&address).await.unwrap();
        assert_eq!(created_id, address.quan_address.0);

        let found = repo.find_by_id(&created_id).await.unwrap().unwrap();
        assert_eq!(found.quan_address.0, address.quan_address.0);
        assert_eq!(found.referral_code, "ref001");
    }

    #[tokio::test]
    async fn test_create_many_and_get_total_items() {
        let repo = setup_test_repository().await;
        let address = create_mock_address("001", "REF001");
        let address2 = create_mock_address_with_referrals_count("002", "REF002", 9);
        let address3 = create_mock_address_with_referrals_count("003", "REF003", 10);
        let address4 = create_mock_address_with_referrals_count("004", "REF004", 11);

        repo.create_many([address, address2, address3, address4].to_vec())
            .await
            .unwrap();

        let total_items = repo.get_total_items(None).await.unwrap();
        assert_eq!(total_items, 3);
    }

    #[tokio::test]
    async fn test_get_total_items_with_referral_code_filter() {
        let repo = setup_test_repository().await;
        let address = create_mock_address("001", "REF001");
        let address2 = create_mock_address_with_referrals_count("002", "REF002", 9);
        let address3 = create_mock_address_with_referrals_count("003", "REF003", 10);
        let address4 = create_mock_address_with_referrals_count("004", "REF004", 11);

        repo.create_many([address, address2, address3, address4].to_vec())
            .await
            .unwrap();

        let total_items = repo.get_total_items(Some(String::from("REF003"))).await.unwrap();
        assert_eq!(total_items, 1);
    }

    #[tokio::test]
    async fn test_get_leaderboard() {
        let repo = setup_test_repository().await;
        let address1 = create_mock_address_with_referrals_count("001", "REF001", 0);
        let address2 = create_mock_address_with_referrals_count("002", "REF002", 10);
        let address3 = create_mock_address_with_referrals_count("003", "REF003", 5);
        let address4 = create_mock_address_with_referrals_count("004", "REF004", 8);

        repo.create_many(vec![address1, address2.clone(), address3, address4])
            .await
            .unwrap();

        let addresses = repo.get_leaderboard_entries(1, 0, None).await.unwrap();
        assert_eq!(addresses.len(), 1);

        let first_index_address = addresses.first().unwrap();
        assert_eq!(first_index_address.address.quan_address.0, address2.quan_address.0);
    }

    #[tokio::test]
    async fn test_get_leaderboard_omit_zero_referral() {
        let repo = setup_test_repository().await;
        let address1 = create_mock_address_with_referrals_count("001", "REF001", 0);
        let address2 = create_mock_address_with_referrals_count("002", "REF002", 10);
        let address3 = create_mock_address_with_referrals_count("003", "REF003", 5);
        let address4 = create_mock_address_with_referrals_count("004", "REF004", 8);

        repo.create_many(vec![address1, address2.clone(), address3, address4])
            .await
            .unwrap();

        let addresses = repo.get_leaderboard_entries(4, 0, None).await.unwrap();
        assert_eq!(addresses.len(), 3);

        let first_index_address = addresses.first().unwrap();
        assert_eq!(first_index_address.address.quan_address.0, address2.quan_address.0);
    }

    #[tokio::test]
    async fn test_get_leaderboard_with_referral_code_filter() {
        let repo = setup_test_repository().await;
        let address1 = create_mock_address_with_referrals_count("001", "REF001", 0);
        let address2 = create_mock_address_with_referrals_count("002", "REF002", 10);
        let address3 = create_mock_address_with_referrals_count("003", "REF003", 5);
        let address4 = create_mock_address_with_referrals_count("004", "REF004", 8);
        let address5 = create_mock_address_with_referrals_count("005", "REF005", 7);

        repo.create_many(vec![address1, address2, address3.clone(), address4, address5.clone()])
            .await
            .unwrap();

        let addresses = repo
            .get_leaderboard_entries(4, 0, Some(String::from("REF005")))
            .await
            .unwrap();
        assert_eq!(addresses.len(), 1);

        let first_index_address = addresses.first().unwrap();
        assert_eq!(first_index_address.address.quan_address.0, address5.quan_address.0);

        assert_eq!(first_index_address.rank, 3);
    }

    #[tokio::test]
    async fn test_create_and_find_by_referral_code() {
        let repo = setup_test_repository().await;
        let address = create_mock_address("001", "REF001");

        let created_id = repo.create(&address).await.unwrap();
        assert_eq!(created_id, address.quan_address.0);

        let found = repo
            .find_by_referral_code(&address.referral_code)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(found.quan_address.0, address.quan_address.0);
        assert_eq!(found.referral_code, "ref001");
    }

    #[tokio::test]
    async fn test_create_conflict() {
        let repo = setup_test_repository().await;
        let address = create_mock_address("002", "REF002");

        // Create the first time
        let created_id1 = repo.create(&address).await.unwrap();
        assert_eq!(created_id1, address.quan_address.0);

        // Attempt to create again
        let created_id2 = repo.create(&address).await.unwrap();
        assert_eq!(created_id2, address.quan_address.0);

        let all_addresses = repo.find_all().await.unwrap();
        assert_eq!(all_addresses.len(), 1);
    }

    #[tokio::test]
    async fn test_find_by_id_not_found() {
        let repo = setup_test_repository().await;
        let found = repo.find_by_id("non_existent_id").await.unwrap();
        assert!(found.is_none());
    }

    #[tokio::test]
    async fn test_find_all() {
        let repo = setup_test_repository().await;

        // Initially empty
        let addresses = repo.find_all().await.unwrap();
        assert!(addresses.is_empty());

        // After creation
        repo.create(&create_mock_address("101", "REF101")).await.unwrap();
        repo.create(&create_mock_address("102", "REF102")).await.unwrap();
        let addresses = repo.find_all().await.unwrap();
        assert_eq!(addresses.len(), 2);
    }

    #[tokio::test]
    async fn test_create_many() {
        let repo = setup_test_repository().await;
        let addresses = vec![
            create_mock_address("201", "REF201"),
            create_mock_address("202", "REF202"),
        ];

        let rows_affected = repo.create_many(addresses).await.unwrap();
        assert_eq!(rows_affected, 2);

        let all = repo.find_all().await.unwrap();
        assert_eq!(all.len(), 2);
    }

    #[tokio::test]
    async fn test_create_many_with_conflicts() {
        let repo = setup_test_repository().await;
        repo.create(&create_mock_address("301", "REF301")).await.unwrap();

        let addresses = vec![
            create_mock_address("301", "REF301"), // Conflict
            create_mock_address("302", "REF302"), // New
        ];

        // ON CONFLICT DO NOTHING means only 1 new row should be affected
        let rows_affected = repo.create_many(addresses).await.unwrap();
        assert_eq!(rows_affected, 1);

        let all = repo.find_all().await.unwrap();
        assert_eq!(all.len(), 2);
    }

    #[tokio::test]
    async fn test_increment_referrals_count() {
        let repo = setup_test_repository().await;
        let address = create_mock_address("501", "REF501");
        repo.create(&address).await.unwrap();

        let new_count = repo.increment_referrals_count(&address.quan_address.0).await.unwrap();
        assert_eq!(new_count, 1);

        let updated = repo.find_by_id(&address.quan_address.0).await.unwrap().unwrap();
        assert_eq!(updated.referrals_count, 1);

        let new_count_2 = repo.increment_referrals_count(&address.quan_address.0).await.unwrap();
        assert_eq!(new_count_2, 2);
    }
}
