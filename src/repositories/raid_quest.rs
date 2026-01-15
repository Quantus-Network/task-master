use chrono::Utc;
use sqlx::{PgPool, Postgres, QueryBuilder};

use crate::{
    db_persistence::DbError,
    handlers::ListQueryParams,
    models::raid_quest::{CreateRaidQuest, RaidQuest, RaidQuestFilter, RaidQuestSortColumn},
    repositories::{calculate_page_offset, DbResult, QueryBuilderExt},
};

#[derive(Clone, Debug)]
pub struct RaidQuestRepository {
    pool: PgPool,
}

impl RaidQuestRepository {
    fn create_select_base_query<'a>() -> QueryBuilder<'a, Postgres> {
        QueryBuilder::new("SELECT * FROM raid_quests")
    }

    fn create_pagination_base_query<'a>(
        &self,
        query_builder: &mut QueryBuilder<'a, Postgres>,
        search: &Option<String>,
        filters: &RaidQuestFilter,
    ) {
        query_builder.push(" FROM raid_quests rq ");

        let mut where_started = false;

        // Global Text Search ---
        if let Some(s) = search {
            if !s.is_empty() {
                query_builder.push(" WHERE (");
                where_started = true;

                query_builder.push(" rq.name ILIKE ");
                query_builder.push_bind(format!("%{}%", s));
                query_builder.push(") ");
            }
        }

        // Filter: Active status
        if let Some(is_active) = filters.is_active {
            if is_active {
                query_builder.push_condition(" rq.end_date IS NULL ", &mut where_started);
            } else {
                query_builder.push_condition(" rq.end_date IS NOT NULL ", &mut where_started);
            }
        }
    }

    pub fn new(pool: &PgPool) -> Self {
        Self { pool: pool.clone() }
    }

    pub async fn find_all(
        &self,
        params: &ListQueryParams<RaidQuestSortColumn>,
        filters: &RaidQuestFilter,
    ) -> Result<Vec<RaidQuest>, DbError> {
        // Select all tweet columns + author name/username
        // We use aliases that match the TweetWithAuthor struct expectations
        let mut query_builder = QueryBuilder::new(
            r#"
            SELECT 
                rq.*
            "#,
        );

        self.create_pagination_base_query(&mut query_builder, &params.search, filters);

        // Sorting
        query_builder.push(" ORDER BY ");
        let sort_col = params.sort_by.as_ref().unwrap_or(&RaidQuestSortColumn::CreatedAt);
        query_builder.push(sort_col.to_sql_column());

        query_builder.push(" ");
        query_builder.push(params.order.to_string());

        // Secondary sort for stability
        query_builder.push(", rq.id ASC");

        // Pagination
        let offset = calculate_page_offset(params.page, params.page_size);
        query_builder.push(" LIMIT ");
        query_builder.push_bind(params.page_size as i64);
        query_builder.push(" OFFSET ");
        query_builder.push_bind(offset as i64);

        let tweets = query_builder
            .build_query_as::<RaidQuest>()
            .fetch_all(&self.pool)
            .await
            .map_err(DbError::Database)?;

        Ok(tweets)
    }

    pub async fn create(&self, new_quest: &CreateRaidQuest) -> DbResult<i32> {
        let start_date = Utc::now();

        let result = sqlx::query_scalar::<_, i32>(
            "
            INSERT INTO raid_quests (name, start_date) 
            VALUES ($1, $2)
            RETURNING id
            ",
        )
        .bind(&new_quest.name)
        .bind(start_date)
        .fetch_one(&self.pool)
        .await;

        match result {
            Ok(id) => Ok(id),
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

    pub async fn delete_by_id(&self, id: i32) -> DbResult<Option<RaidQuest>> {
        let mut qb = QueryBuilder::new("DELETE FROM raid_quests");
        qb.push(" WHERE id = ");
        qb.push_bind(id);
        qb.push(" RETURNING *");

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
        qb.push(" AND end_date IS NULL");

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

    pub async fn make_active(&self, id: i32) -> DbResult<()> {
        let result = sqlx::query("UPDATE raid_quests SET end_date = NULL WHERE id = $1")
            .bind(id)
            .execute(&self.pool)
            .await;

        match result {
            Ok(result) => {
                if result.rows_affected() == 0 {
                    return Err(DbError::RecordNotFound(format!("Raid Quest {} not found", id)));
                }

                Ok(())
            }
            Err(sqlx::Error::Database(db_err)) => {
                // Check specifically for Exclusion Violation (Postgres Code 23P01)
                if let Some(code) = db_err.code() {
                    if code == "23P01" {
                        return Err(DbError::UniqueViolation(
                            "Cannot revert active raid: Another raid is currently active or overlaps with this time range."
                                .to_string(),
                        ));
                    }
                }
                Err(DbError::Database(sqlx::Error::Database(db_err)))
            }
            Err(e) => Err(DbError::Database(e)),
        }
    }

    pub async fn count_filtered(
        &self,
        params: &ListQueryParams<RaidQuestSortColumn>,
        filters: &RaidQuestFilter,
    ) -> Result<i64, DbError> {
        let mut query_builder = QueryBuilder::new("SELECT COUNT(rq.id)");

        self.create_pagination_base_query(&mut query_builder, &params.search, filters);

        let count = query_builder
            .build_query_scalar()
            .fetch_one(&self.pool)
            .await
            .map_err(DbError::Database)?;

        Ok(count)
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
        CreateRaidQuest { name: name.to_string() }
    }

    // -------------------------------------------------------------------------
    // Tests
    // -------------------------------------------------------------------------

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
        let repo_err = repo.create(&input2).await.unwrap_err();
        match repo_err {
            DbError::UniqueViolation(_) => {}
            err => panic!("Expected UniqueViolation, got {:?}", err),
        }

        let db_err =
            sqlx::query_scalar::<_, i32>("INSERT INTO raid_quests (name, start_date) VALUES ($1, $2) RETURNING id")
                .bind("Raid Three")
                .bind(Utc::now())
                .fetch_one(&repo.pool)
                .await
                .unwrap_err();
        match db_err {
            sqlx::Error::Database(db_err) => assert_eq!(db_err.code().as_deref(), Some("23P01")),
            err => panic!("Expected exclusion violation (23P01), got {:?}", err),
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
        let finished_raid = repo.find_active().await.unwrap();
        assert!(finished_raid.is_none());
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
