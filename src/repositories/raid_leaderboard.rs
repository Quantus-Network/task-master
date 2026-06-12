use crate::repositories::DbResult;
use sqlx::PgPool;

#[derive(Clone, Debug)]
pub struct RaidLeaderboardRepository {
    pool: PgPool,
}

impl RaidLeaderboardRepository {
    pub fn new(pool: &PgPool) -> Self {
        Self { pool: pool.clone() }
    }

    pub async fn refresh(&self) -> DbResult<()> {
        sqlx::query("REFRESH MATERIALIZED VIEW CONCURRENTLY raid_leaderboards")
            .execute(&self.pool)
            .await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{config::Config, utils::test_db::reset_database};
    use sqlx::PgPool;

    async fn setup_test_repository() -> RaidLeaderboardRepository {
        let config = Config::load_test_env().expect("Failed to load configuration for tests");
        let pool = PgPool::connect(config.get_database_url())
            .await
            .expect("Failed to create pool.");

        reset_database(&pool).await;

        RaidLeaderboardRepository::new(&pool)
    }

    #[tokio::test]
    async fn test_refresh_materialized_view() {
        let repo = setup_test_repository().await;

        sqlx::query("INSERT INTO raid_quests (name, start_date) VALUES ('Refresh Raid', NOW())")
            .execute(&repo.pool)
            .await
            .unwrap();

        repo.refresh().await.expect("Failed to refresh view");
    }
}
