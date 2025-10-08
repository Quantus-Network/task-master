use sqlx::PgPool;

use crate::{models::referrals::Referral, repositories::DbResult};

#[derive(Clone, Debug)]
pub struct ReferralRepository {
    pool: PgPool,
}
impl ReferralRepository {
    pub fn new(pool: &PgPool) -> Self {
        Self { pool: pool.clone() }
    }

    pub async fn create(&self, new_referral: &Referral) -> DbResult<i32> {
        let created_id = sqlx::query_scalar::<_, i32>(
            "
        INSERT INTO referrals (referrer_address, referee_address) 
        VALUES ($1, $2)
        RETURNING id
        ",
        )
        .bind(new_referral.referrer_address.0.clone())
        .bind(new_referral.referee_address.0.clone())
        .fetch_one(&self.pool)
        .await?;

        Ok(created_id)
    }

    pub async fn find_all_by_referrer(&self, quan_address: String) -> DbResult<Vec<Referral>> {
        let referrals =
            sqlx::query_as::<_, Referral>("SELECT * FROM referrals WHERE referrer_address = $1")
                .bind(quan_address.clone())
                .fetch_all(&self.pool)
                .await?;

        Ok(referrals)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        config::Config,
        models::{
            address::{Address, AddressInput},
            referrals::{Referral, ReferralInput},
        },
        repositories::address::AddressRepository,
    };
    use sqlx::PgPool;

    // Helper function to set up test repositories.
    // Cleans both tables to ensure a fresh state and handle foreign keys.
    async fn setup_test_repositories() -> (AddressRepository, ReferralRepository) {
        let config = Config::load().expect("Failed to load configuration for tests");
        let pool = PgPool::connect(config.get_database_url())
            .await
            .expect("Failed to create pool.");

        sqlx::query("TRUNCATE addresses, referrals RESTART IDENTITY CASCADE")
            .execute(&pool)
            .await
            .expect("Failed to truncate tables.");

        (
            AddressRepository::new(&pool),
            ReferralRepository::new(&pool),
        )
    }

    // Helper to create a persisted address for tests.
    async fn create_persisted_address(repo: &AddressRepository, id: &str) -> Address {
        let input = AddressInput {
            quan_address: format!("qz_test_address_{}", id),
            eth_address: None,
            referral_code: format!("REF{}", id),
        };
        let address = Address::new(input).unwrap();
        repo.create(&address).await.unwrap();
        address
    }

    #[tokio::test]
    async fn test_create_and_find_referral() {
        let (address_repo, referral_repo) = setup_test_repositories().await;

        // Referrals require existing addresses, so we create them first.
        let referrer = create_persisted_address(&address_repo, "referrer_01").await;
        let referee = create_persisted_address(&address_repo, "referee_01").await;

        let referral_input = ReferralInput {
            referrer_address: referrer.quan_address.0.clone(),
            referee_address: referee.quan_address.0.clone(),
        };
        let new_referral = Referral::new(referral_input).unwrap();

        let created_id = referral_repo.create(&new_referral).await.unwrap();
        assert!(created_id > 0);

        // Verify by finding it
        let referrals = referral_repo
            .find_all_by_referrer(referrer.quan_address.0)
            .await
            .unwrap();

        assert_eq!(referrals.len(), 1);
        assert_eq!(
            referrals[0].referee_address.0,
            referee.quan_address.0
        );
    }

    #[tokio::test]
    async fn test_find_all_by_referrer() {
        let (address_repo, referral_repo) = setup_test_repositories().await;

        let referrer = create_persisted_address(&address_repo, "referrer_02").await;
        let referee1 = create_persisted_address(&address_repo, "referee_02a").await;
        let referee2 = create_persisted_address(&address_repo, "referee_02b").await;
        // This one should not be found in the results
        let other_referrer = create_persisted_address(&address_repo, "other_referrer").await;

        // Create two referrals from the same referrer
        referral_repo
            .create(
                &Referral::new(ReferralInput {
                    referrer_address: referrer.quan_address.0.clone(),
                    referee_address: referee1.quan_address.0.clone(),
                })
                .unwrap(),
            )
            .await
            .unwrap();
        referral_repo
            .create(
                &Referral::new(ReferralInput {
                    referrer_address: referrer.quan_address.0.clone(),
                    referee_address: referee2.quan_address.0.clone(),
                })
                .unwrap(),
            )
            .await
            .unwrap();
        // Create an unrelated referral
        referral_repo
            .create(
                &Referral::new(ReferralInput {
                    referrer_address: other_referrer.quan_address.0.clone(),
                    referee_address: referee1.quan_address.0.clone(),
                })
                .unwrap(),
            )
            .await
            .unwrap();

        let results = referral_repo
            .find_all_by_referrer(referrer.quan_address.0)
            .await
            .unwrap();
        assert_eq!(results.len(), 2);
    }

    #[tokio::test]
    async fn test_find_all_by_referrer_no_results() {
        let (_address_repo, referral_repo) = setup_test_repositories().await;
        
        let results = referral_repo
            .find_all_by_referrer("qz_non_existent_address".to_string())
            .await
            .unwrap();
            
        assert!(results.is_empty());
    }
}