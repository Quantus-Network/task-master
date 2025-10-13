use sqlx::PgPool;

pub async fn reset_database(pool: &PgPool) {
    // Truncate in FK-safe order with cascade to ensure full cleanup.
    sqlx::query("TRUNCATE tasks, referrals, addresses RESTART IDENTITY CASCADE")
        .execute(pool)
        .await
        .expect("Failed to truncate tables for tests");
}


