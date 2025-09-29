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
