use chrono::Utc;
use sqlx::{PgPool, Postgres, QueryBuilder};

use crate::{
    db_persistence::DbError,
    models::raid_quest::{CreateRaidQuest, RaidQuest},
    repositories::DbResult,
};

#[derive(Clone, Debug)]
pub struct RaidQuestRepository {
    pool: PgPool,
}

impl RaidQuestRepository {
    fn create_select_base_query<'a>() -> QueryBuilder<'a, Postgres> {
        QueryBuilder::new("SELECT * FROM raid_quests")
    }

    pub fn new(pool: &PgPool) -> Self {
        Self { pool: pool.clone() }
    }

    pub async fn create(&self, new_quest: &CreateRaidQuest) -> DbResult<i32> {
        let start_date = new_quest.start_date.unwrap_or_else(|| Utc::now());

        let result = sqlx::query_scalar::<_, i32>(
            "
            INSERT INTO raid_quests (name, start_date, end_date) 
            VALUES ($1, $2, $3)
            RETURNING id
            ",
        )
        .bind(&new_quest.name)
        .bind(start_date)
        .bind(new_quest.end_date)
        .fetch_optional(&self.pool)
        .await;

        match result {
            Ok(Some(id)) => Ok(id),
            Ok(None) => Err(DbError::RecordNotFound("Failed to retrieve generated ID".to_string())),
            Err(sqlx::Error::Database(db_err)) => {
                // Check specifically for Exclusion Violation (Postgres Code 23P01)
                if let Some(code) = db_err.code() {
                    if code == "23P01" {
                        return Err(DbError::UniqueViolation(
                            "Cannot create raid: Another raid is currently active or overlaps with this time range."
                                .to_string(),
                        ));
                    }
                }
                Err(DbError::Database(sqlx::Error::Database(db_err)))
            }
            Err(e) => Err(DbError::Database(e)),
        }
    }

    pub async fn find_by_id(&self, id: i32) -> DbResult<Option<RaidQuest>> {
        let mut qb = Self::create_select_base_query();
        qb.push(" WHERE id = ");
        qb.push_bind(id);

        let quest = qb.build_query_as().fetch_optional(&self.pool).await?;

        Ok(quest)
    }

    /// Finds the single currently active raid quest.
    /// Active = start_date <= NOW and (end_date IS NULL or end_date > NOW)
    pub async fn find_active(&self) -> DbResult<Option<RaidQuest>> {
        let mut qb = Self::create_select_base_query();
        let now = Utc::now();

        qb.push(" WHERE start_date <= ");
        qb.push_bind(now);

        // Check that it hasn't ended yet
        qb.push(" AND (end_date IS NULL OR end_date > ");
        qb.push_bind(now);
        qb.push(")");

        // Order by most recently started just to be safe
        qb.push(" ORDER BY start_date DESC LIMIT 1");

        let quest = qb.build_query_as().fetch_optional(&self.pool).await?;

        Ok(quest)
    }

    pub async fn finish(&self, id: i32) -> DbResult<()> {
        let result = sqlx::query("UPDATE raid_quests SET end_date = $1 WHERE id = $2")
            .bind(Utc::now())
            .bind(id)
            .execute(&self.pool)
            .await?;

        if result.rows_affected() == 0 {
            return Err(DbError::RecordNotFound(format!("Raid Quest {} not found", id)));
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use crate::utils::test_db::reset_database;
    use sqlx::PgPool;

    // -------------------------------------------------------------------------
    // Setup & Helpers
    // -------------------------------------------------------------------------

    async fn setup_test_repository() -> RaidQuestRepository {
        let config = Config::load_test_env().expect("Failed to load configuration for tests");
        let pool = PgPool::connect(config.get_database_url())
            .await
            .expect("Failed to create pool.");

        // Clean database before each test
        reset_database(&pool).await;

        RaidQuestRepository::new(&pool)
    }

    fn create_mock_quest_input(name: &str) -> CreateRaidQuest {
        CreateRaidQuest {
            name: name.to_string(),
            start_date: None, // Will default to NOW()
            end_date: None,
        }
    }

    // -------------------------------------------------------------------------
    // Tests
    // -------------------------------------------------------------------------

    #[tokio::test]
    async fn test_create_and_find_by_id() {
        let repo = setup_test_repository().await;
        let input = create_mock_quest_input("Raid Alpha");

        let id = repo.create(&input).await.expect("Failed to create raid");

        let found = repo
            .find_by_id(id)
            .await
            .expect("Failed to find raid")
            .expect("Raid not found");

        assert_eq!(found.name, "Raid Alpha");
        assert!(found.end_date.is_none());
    }

    #[tokio::test]
    async fn test_find_active_none() {
        let repo = setup_test_repository().await;

        // No raids created yet
        let active = repo.find_active().await.expect("Failed to query active");
        assert!(active.is_none());
    }

    #[tokio::test]
    async fn test_create_active_constraint_violation() {
        let repo = setup_test_repository().await;

        // 1. Create first active raid (Start: NOW, End: NULL)
        let input1 = create_mock_quest_input("Raid One");
        repo.create(&input1).await.expect("First raid should succeed");

        // 2. Attempt to create second active raid immediately
        let input2 = create_mock_quest_input("Raid Two");
        let result = repo.create(&input2).await;

        // 3. Assert failure
        assert!(result.is_err());

        match result.unwrap_err() {
            DbError::UniqueViolation(_) => {
                // Success: We caught the specific overlap violation
            }
            err => panic!("Expected UniqueViolation, got {:?}", err),
        }
    }

    #[tokio::test]
    async fn test_finish_raid() {
        let repo = setup_test_repository().await;

        // 1. Create and verify active
        let id = repo.create(&create_mock_quest_input("Raid Active")).await.unwrap();
        let active = repo.find_active().await.unwrap();
        assert!(active.is_some());

        // 2. Finish the raid
        repo.finish(id).await.expect("Failed to finish raid");

        // 3. Verify it is no longer active
        let active_after = repo.find_active().await.unwrap();
        assert!(active_after.is_none());

        // 4. Verify end_date is set in DB
        let finished_raid = repo.find_by_id(id).await.unwrap().unwrap();
        assert!(finished_raid.end_date.is_some());
    }

    #[tokio::test]
    async fn test_cycle_create_finish_create() {
        let repo = setup_test_repository().await;

        // 1. Create Raid A
        let id_a = repo.create(&create_mock_quest_input("Raid A")).await.unwrap();

        // 2. Finish Raid A
        repo.finish(id_a).await.unwrap();

        // 3. Create Raid B (Should succeed because A is finished)
        let input_b = create_mock_quest_input("Raid B");
        let result = repo.create(&input_b).await;

        assert!(
            result.is_ok(),
            "Should be able to create new raid after finishing previous one"
        );

        let id_b = result.unwrap();
        assert_ne!(id_a, id_b);

        // 4. Verify B is the current active one
        let active = repo.find_active().await.unwrap().unwrap();
        assert_eq!(active.id, id_b);
        assert_eq!(active.name, "Raid B");
    }

    #[tokio::test]
    async fn test_cannot_finish_non_existent_raid() {
        let repo = setup_test_repository().await;

        let result = repo.finish(9999).await;

        assert!(result.is_err());
        match result.unwrap_err() {
            DbError::RecordNotFound(_) => {} // Expected
            err => panic!("Expected RecordNotFound, got {:?}", err),
        }
    }
}
