use sqlx::PgPool;

pub async fn reset_database(pool: &PgPool) {
    sqlx::query("TRUNCATE tasks, referrals, opt_ins, addresses RESTART IDENTITY CASCADE")
        .execute(pool)
        .await
        .expect("Failed to truncate tables for tests");
}


