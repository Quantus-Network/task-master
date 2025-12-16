use sqlx::{PgPool, Postgres, QueryBuilder};

use crate::{
    db_persistence::DbError,
    models::raid_submission::{CreateRaidSubmission, RaidSubmission, UpdateRaidSubmissionStats},
    repositories::DbResult,
};

#[derive(Clone, Debug)]
pub struct RaidSubmissionRepository {
    pool: PgPool,
}

impl RaidSubmissionRepository {
    fn create_select_base_query<'a>() -> QueryBuilder<'a, Postgres> {
        QueryBuilder::new("SELECT * FROM raid_submissions")
    }

    pub fn new(pool: &PgPool) -> Self {
        Self { pool: pool.clone() }
    }

    pub async fn create(&self, submission: &CreateRaidSubmission) -> DbResult<String> {
        let created_id = sqlx::query_scalar::<_, String>(
            "
            INSERT INTO raid_submissions (
                id, raid_id, target_id, raider_id
            ) 
            VALUES ($1, $2, $3, $4)
            RETURNING id
            ",
        )
        .bind(&submission.id)
        .bind(submission.raid_id)
        .bind(&submission.target_id)
        .bind(&submission.raider_id)
        .fetch_optional(&self.pool)
        .await?;

        if let Some(id) = created_id {
            Ok(id)
        } else {
            Err(DbError::RecordNotFound("Failed to retrieve generated ID".to_string()))
        }
    }

    pub async fn delete(&self, submission_id: &str) -> DbResult<()> {
        sqlx::query("DELETE FROM raid_submissions WHERE id = $1")
            .bind(submission_id)
            .execute(&self.pool)
            .await?;

        Ok(())
    }

    pub async fn find_by_id(&self, id: &str) -> DbResult<Option<RaidSubmission>> {
        let mut qb = Self::create_select_base_query();
        qb.push(" WHERE id = ");
        qb.push_bind(id);

        let submission = qb.build_query_as().fetch_optional(&self.pool).await?;

        Ok(submission)
    }

    pub async fn find_by_raid(&self, raid_id: i32) -> DbResult<Vec<RaidSubmission>> {
        let mut qb = Self::create_select_base_query();
        qb.push(" WHERE raid_id = ");
        qb.push_bind(raid_id);
        qb.push(" ORDER BY created_at DESC");

        let submissions = qb.build_query_as().fetch_all(&self.pool).await?;

        Ok(submissions)
    }

    pub async fn update_stats_many(&self, updates: &[UpdateRaidSubmissionStats]) -> DbResult<u64> {
        if updates.is_empty() {
            return Ok(0);
        }

        let mut ids = Vec::with_capacity(updates.len());
        let mut impressions = Vec::with_capacity(updates.len());
        let mut replies = Vec::with_capacity(updates.len());
        let mut retweets = Vec::with_capacity(updates.len());
        let mut likes = Vec::with_capacity(updates.len());

        for u in updates {
            ids.push(u.id.clone());
            impressions.push(u.impression_count);
            replies.push(u.reply_count);
            retweets.push(u.retweet_count);
            likes.push(u.like_count);
        }

        let query = "
            UPDATE raid_submissions AS rs
            SET 
                impression_count = data.impression_count,
                reply_count = data.reply_count,
                retweet_count = data.retweet_count,
                like_count = data.like_count,
                updated_at = NOW()
            FROM (
                SELECT * FROM UNNEST(
                    $1::varchar[], 
                    $2::int[], 
                    $3::int[], 
                    $4::int[], 
                    $5::int[]
                ) AS t(id, impression_count, reply_count, retweet_count, like_count)
            ) AS data
            WHERE rs.id = data.id
        ";

        let result = sqlx::query(query)
            .bind(&ids)
            .bind(&impressions)
            .bind(&replies)
            .bind(&retweets)
            .bind(&likes)
            .execute(&self.pool)
            .await?;

        Ok(result.rows_affected())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use crate::utils::test_db::reset_database;
    use chrono::Utc;
    use sqlx::PgPool;
    use uuid::Uuid;

    // -------------------------------------------------------------------------
    // Setup & Helpers
    // -------------------------------------------------------------------------

    async fn setup_test_repository() -> RaidSubmissionRepository {
        let config = Config::load_test_env().expect("Failed to load configuration for tests");
        let pool = PgPool::connect(config.get_database_url())
            .await
            .expect("Failed to create pool.");

        // Clean database before each test to ensure empty state
        reset_database(&pool).await;

        RaidSubmissionRepository::new(&pool)
    }

    struct SeedData {
        raid_id: i32,
        raider_id: String,
        target_id: String,
    }

    // Helper to satisfy the strict Foreign Key chain:
    // 1. Address (Raider)
    // 2. Raid Quest
    // 3. Tweet Author (Required by Relevant Tweet)
    // 4. Relevant Tweet (Target)
    async fn seed_dependencies(pool: &PgPool) -> SeedData {
        // 1. Seed Address (Raider)
        let raider_id = "qz_test_raider_01".to_string();
        sqlx::query("INSERT INTO addresses (quan_address, referral_code) VALUES ($1, 'REF01')")
            .bind(&raider_id)
            .execute(pool)
            .await
            .expect("Failed to seed address");

        // 2. Seed Raid Quest
        // Note: RaidQuests has 'enforce_one_active_raid' constraint, but since we reset DB, this is safe.
        let raid_id: i32 =
            sqlx::query_scalar("INSERT INTO raid_quests (name, start_date) VALUES ('Test Raid', NOW()) RETURNING id")
                .fetch_one(pool)
                .await
                .expect("Failed to seed raid quest");

        // 3. Seed Tweet Author (REQUIRED because relevant_tweets references tweet_authors)
        let author_id = "author_123".to_string();
        sqlx::query("INSERT INTO tweet_authors (id, name, username) VALUES ($1, 'Test Author', 'test_author')")
            .bind(&author_id)
            .execute(pool)
            .await
            .expect("Failed to seed tweet author");

        // 4. Seed Relevant Tweet (Target)
        // Note: created_at is NOT NULL in migration 005
        let target_id = "tweet_target_1".to_string();
        sqlx::query(
            "INSERT INTO relevant_tweets (id, author_id, text, created_at) VALUES ($1, $2, 'Target Tweet', $3)",
        )
        .bind(&target_id)
        .bind(&author_id)
        .bind(Utc::now())
        .execute(pool)
        .await
        .expect("Failed to seed relevant tweet");

        SeedData {
            raid_id,
            raider_id,
            target_id,
        }
    }

    fn create_mock_submission_input(seed: &SeedData) -> CreateRaidSubmission {
        CreateRaidSubmission {
            id: Uuid::new_v4().to_string(),
            raid_id: seed.raid_id,
            target_id: seed.target_id.clone(),
            raider_id: seed.raider_id.clone(),
        }
    }

    // -------------------------------------------------------------------------
    // Tests
    // -------------------------------------------------------------------------

    #[tokio::test]
    async fn test_create_and_find_by_id() {
        let repo = setup_test_repository().await;
        let seed = seed_dependencies(&repo.pool).await;

        let input = create_mock_submission_input(&seed);

        // 1. Create
        let created_id = repo.create(&input).await.expect("Failed to create submission");
        assert_eq!(created_id, input.id);

        // 2. Find
        let found = repo
            .find_by_id(&created_id)
            .await
            .expect("Failed to query")
            .expect("Submission not found");

        assert_eq!(found.id, input.id);
        assert_eq!(found.raid_id, seed.raid_id);
        assert_eq!(found.raider_id, seed.raider_id);
        assert_eq!(found.impression_count, 0);
        assert_eq!(found.like_count, 0);
    }

    #[tokio::test]
    async fn test_create_and_delete_by_id() {
        let repo = setup_test_repository().await;
        let seed = seed_dependencies(&repo.pool).await;

        let input = create_mock_submission_input(&seed);

        // 1. Create
        let created_id = repo.create(&input).await.expect("Failed to create submission");
        assert_eq!(created_id, input.id);

        // 2. Delete
        repo.delete(&created_id).await.unwrap();

        // 3. Find
        let found = repo.find_by_id(&created_id).await.expect("Failed to query");

        assert!(found.is_none());
    }

    #[tokio::test]
    async fn test_find_by_raid_sorting() {
        let repo = setup_test_repository().await;
        let seed = seed_dependencies(&repo.pool).await;

        // Create 3 submissions with slight delays to ensure distinct created_at timestamps
        let sub1 = create_mock_submission_input(&seed);
        repo.create(&sub1).await.unwrap();
        tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;

        let sub2 = create_mock_submission_input(&seed);
        repo.create(&sub2).await.unwrap();
        tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;

        let sub3 = create_mock_submission_input(&seed);
        repo.create(&sub3).await.unwrap();

        // Query by Raid ID
        let results = repo.find_by_raid(seed.raid_id).await.unwrap();

        assert_eq!(results.len(), 3);

        // Verify Sorting: Query uses "ORDER BY created_at DESC"
        // So sub3 (newest) should be first
        assert_eq!(results[0].id, sub3.id, "Newest submission should be first");
        assert_eq!(results[1].id, sub2.id);
        assert_eq!(results[2].id, sub1.id, "Oldest submission should be last");
    }

    #[tokio::test]
    async fn test_update_stats_success() {
        let repo = setup_test_repository().await;
        let seed = seed_dependencies(&repo.pool).await;
        let input = create_mock_submission_input(&seed);

        repo.create(&input).await.unwrap();

        // 1. Prepare the Update Payload
        let updates = vec![UpdateRaidSubmissionStats {
            id: input.id.clone(),
            impression_count: 100,
            reply_count: 5,
            retweet_count: 10,
            like_count: 50,
        }];

        // 2. Execute Bulk Update
        let rows_affected = repo.update_stats_many(&updates).await.expect("Failed to update stats");

        assert_eq!(rows_affected, 1, "Should have updated exactly 1 record");

        // 3. Verify Update in DB
        let updated = repo.find_by_id(&input.id).await.unwrap().unwrap();

        assert_eq!(updated.impression_count, 100);
        assert_eq!(updated.reply_count, 5);
        assert_eq!(updated.retweet_count, 10);
        assert_eq!(updated.like_count, 50);

        // 4. Verify `updated_at` trigger worked
        assert!(
            updated.updated_at > updated.created_at,
            "updated_at timestamp was not refreshed"
        );
    }

    #[tokio::test]
    async fn test_update_stats_not_found() {
        let repo = setup_test_repository().await;

        // Try to update a random ID that doesn't exist
        let updates = vec![UpdateRaidSubmissionStats {
            id: "non-existent-id".to_string(),
            impression_count: 10,
            reply_count: 0,
            retweet_count: 0,
            like_count: 0,
        }];

        // In bulk operations, a missing ID usually results in 0 rows affected,
        // rather than an Error, because it's a set-based operation.
        let result = repo.update_stats_many(&updates).await;

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 0, "Should return 0 affected rows for non-existent ID");
    }

    #[tokio::test]
    async fn test_create_fails_without_dependencies() {
        let repo = setup_test_repository().await;
        // We do NOT call seed_dependencies here

        let input = CreateRaidSubmission {
            id: Uuid::new_v4().to_string(),
            raid_id: 9999, // Non-existent Raid
            target_id: "fake_tweet".to_string(),
            raider_id: "fake_user".to_string(),
        };

        let result = repo.create(&input).await;

        // This should fail with a Foreign Key violation
        assert!(result.is_err());

        // Verify it is a Database error (SQL State 23503 is foreign_key_violation)
        if let DbError::Database(sqlx::Error::Database(e)) = result.unwrap_err() {
            assert_eq!(e.code().unwrap(), "23503");
        } else {
            // It might be wrapped differently depending on your DbError impl, but valid failure is enough
        }
    }
}
