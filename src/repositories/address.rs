use sqlx::PgPool;

use crate::{models::address::Address, repositories::DbResult};

#[derive(Clone, Debug)]
pub struct AddressRepository {
    pool: PgPool,
}
impl AddressRepository {
    pub fn new(pool: &PgPool) -> Self {
        Self { pool: pool.clone() }
    }

    pub async fn create(&self, new_address: &Address) -> DbResult<String> {
        let created_id = sqlx::query_scalar::<_, String>(
             "
        INSERT INTO addresses (quan_address, eth_address, referral_code, referrals_count) 
        VALUES ($1, $2, $3, $4)
        ON CONFLICT (quan_address) 
        DO UPDATE SET quan_address = EXCLUDED.quan_address
        RETURNING quan_address
        ",
        )
        .bind(new_address.quan_address.0.clone())
        .bind(new_address.eth_address.0.clone())
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
        let mut eth_addresses = Vec::with_capacity(addresses.len());
        let mut referral_codes = Vec::with_capacity(addresses.len());
        let mut referrals_counts = Vec::with_capacity(addresses.len());

        for address in addresses {
            quan_addresses.push(address.quan_address.0);
            eth_addresses.push(address.eth_address.0);
            referral_codes.push(address.referral_code);
            referrals_counts.push(address.referrals_count);
        }

        let result = sqlx::query(
            r#"
        INSERT INTO addresses (quan_address, eth_address, referral_code, referrals_count)
        SELECT * FROM UNNEST($1, $2, $3, $4)
        ON CONFLICT (quan_address) DO NOTHING
        "#,
        )
        .bind(&quan_addresses)
        .bind(&eth_addresses)
        .bind(&referral_codes)
        .bind(&referrals_counts)
        .execute(&self.pool)
        .await?;

        Ok(result.rows_affected())
    }

    pub async fn find_by_id(&self, id: &str) -> DbResult<Option<Address>> {
        let address =
            sqlx::query_as::<_, Address>("SELECT * FROM addresses WHERE quan_address = $1")
                .bind(id)
                .fetch_optional(&self.pool)
                .await?;

        Ok(address)
    }

    pub async fn find_all(&self) -> DbResult<Vec<Address>> {
        let addresses = sqlx::query_as::<_, Address>("SELECT * FROM addresses")
            .fetch_all(&self.pool)
            .await?;

        Ok(addresses)
    }

    pub async fn update_address_last_selected(&self, quan_address: &str) -> DbResult<()> {
        sqlx::query(
            "UPDATE addresses SET last_selected_at = CURRENT_TIMESTAMP WHERE quan_address = $1",
        )
        .bind(quan_address)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    pub async fn update_address_eth(&self, quan_address: &str, eth_address: &str) -> DbResult<()> {
        sqlx::query("UPDATE addresses SET eth_address = $1 WHERE quan_address = $2")
            .bind(eth_address)
            .bind(quan_address)
            .execute(&self.pool)
            .await?;

        Ok(())
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
