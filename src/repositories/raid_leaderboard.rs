use crate::{models::raid_leaderboard::RaidLeaderboard, repositories::DbResult};
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

    /// Retrieves the top raiders with their Rank and Address info.
    pub async fn get_entries(&self, raid_id: i32, limit: i64, offset: i64) -> DbResult<Vec<RaidLeaderboard>> {
        let query = "
            SELECT 
                l.raid_id,
                l.total_submissions,
                l.total_impressions,
                l.total_replies,
                l.total_retweets,
                l.total_likes,
                l.last_activity,
                RANK() OVER (ORDER BY l.total_impressions DESC) as rank,
                a.quan_address,
                a.referral_code,
                a.referrals_count
            FROM raid_leaderboards l
            JOIN addresses a ON l.raider_id = a.quan_address
            WHERE l.raid_id = $1
            ORDER BY l.total_impressions DESC
            LIMIT $2 OFFSET $3
        ";

        let leaderboard = sqlx::query_as::<_, RaidLeaderboard>(query)
            .bind(raid_id)
            .bind(limit)
            .bind(offset)
            .fetch_all(&self.pool)
            .await?;

        Ok(leaderboard)
    }

    /// Get a specific user's stats, rank, and populated address info.
    pub async fn get_raider_entry(&self, raid_id: i32, raider_id: &str) -> DbResult<Option<RaidLeaderboard>> {
        // CTE calculates rank first, then we join addresses on the result
        let query = "
            WITH ranked_entries AS (
                SELECT 
                    *,
                    RANK() OVER (ORDER BY total_impressions DESC) as rank
                FROM raid_leaderboards
                WHERE raid_id = $1
            )
            SELECT 
                r.raid_id,
                r.total_submissions,
                r.total_impressions,
                r.total_replies,
                r.total_retweets,
                r.total_likes,
                r.last_activity,
                r.rank,
                -- Joined Address Data
                a.quan_address,
                a.referral_code,
                a.referrals_count
            FROM ranked_entries r
            JOIN addresses a ON r.raider_id = a.quan_address
            WHERE r.raider_id = $2
        ";

        let entry = sqlx::query_as::<_, RaidLeaderboard>(query)
            .bind(raid_id)
            .bind(raider_id)
            .fetch_optional(&self.pool)
            .await?;

        Ok(entry)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use crate::utils::test_db::reset_database;
    use sqlx::PgPool;
    use uuid::Uuid;

    // -------------------------------------------------------------------------
    // Setup & Helpers
    // -------------------------------------------------------------------------

    async fn setup_test_repository() -> RaidLeaderboardRepository {
        let config = Config::load_test_env().expect("Failed to load configuration for tests");
        let pool = PgPool::connect(config.get_database_url())
            .await
            .expect("Failed to create pool.");

        // Clean database to ensure the view starts empty
        reset_database(&pool).await;

        RaidLeaderboardRepository::new(&pool)
    }

    struct ScenarioData {
        raid_id: i32,
        user_high: String, // Rank 1
        user_mid: String,  // Rank 2
        user_low: String,  // Rank 3
    }

    /// Seeds a complete scenario:
    /// 1. One active Raid
    /// 2. Three Users (High, Mid, Low stats) with Address profiles
    /// 3. Submissions for each user with defined impression counts to force ranking
    async fn seed_leaderboard_scenario(pool: &PgPool) -> ScenarioData {
        // 1. Create Raid
        let raid_id: i32 = sqlx::query_scalar(
            "INSERT INTO raid_quests (name, start_date) VALUES ('Leaderboard Raid', NOW()) RETURNING id",
        )
        .fetch_one(pool)
        .await
        .unwrap();

        // 2. Create Tweet Author (Required for FK)
        sqlx::query("INSERT INTO tweet_authors (id, name, username) VALUES ('auth_1', 'Auth', 'auth')")
            .execute(pool)
            .await
            .unwrap();

        // 3. Create 3 Users and their Submissions
        let users = vec![
            ("user_high", "REF_HIGH", 1000), // 1000 Impressions
            ("user_mid", "REF_MID", 500),    // 500 Impressions
            ("user_low", "REF_LOW", 100),    // 100 Impressions
        ];

        for (uid, ref_code, impressions) in &users {
            // A. Create Address
            sqlx::query("INSERT INTO addresses (quan_address, referral_code) VALUES ($1, $2)")
                .bind(uid)
                .bind(ref_code)
                .execute(pool)
                .await
                .unwrap();

            // B. Create Target Tweet (Unique per user to avoid unique constraint issues if any)
            let tweet_id = format!("tweet_{}", uid);
            sqlx::query(
                "INSERT INTO relevant_tweets (id, author_id, text, created_at) VALUES ($1, 'auth_1', 'text', NOW())",
            )
            .bind(&tweet_id)
            .execute(pool)
            .await
            .unwrap();

            // C. Create Submission
            sqlx::query(
                "INSERT INTO raid_submissions (id, raid_id, target_id, raider_id, text, impression_count) 
                 VALUES ($1, $2, $3, $4, 'content', $5)",
            )
            .bind(Uuid::new_v4().to_string())
            .bind(raid_id)
            .bind(tweet_id)
            .bind(uid)
            .bind(impressions)
            .execute(pool)
            .await
            .unwrap();
        }

        ScenarioData {
            raid_id,
            user_high: "user_high".to_string(),
            user_mid: "user_mid".to_string(),
            user_low: "user_low".to_string(),
        }
    }

    // -------------------------------------------------------------------------
    // Tests
    // -------------------------------------------------------------------------

    #[tokio::test]
    async fn test_refresh_and_get_entries_ranking() {
        let repo = setup_test_repository().await;
        let data = seed_leaderboard_scenario(&repo.pool).await;

        // CRITICAL: We must refresh the view, or it will be empty
        repo.refresh().await.expect("Failed to refresh view");

        // 1. Get Top 3
        let entries = repo.get_entries(data.raid_id, 10, 0).await.unwrap();

        assert_eq!(entries.len(), 3);

        // 2. Verify Ranking (High -> Mid -> Low)
        let first = &entries[0];
        assert_eq!(first.raider.address, data.user_high);
        assert_eq!(first.rank, Some(1));
        assert_eq!(first.total_impressions, Some(1000));
        assert_eq!(first.raider.referral_code, Some("REF_HIGH".to_string()));

        let second = &entries[1];
        assert_eq!(second.raider.address, data.user_mid);
        assert_eq!(second.rank, Some(2));

        let third = &entries[2];
        assert_eq!(third.raider.address, data.user_low);
        assert_eq!(third.rank, Some(3));
    }

    #[tokio::test]
    async fn test_get_raider_entry_specific_rank() {
        let repo = setup_test_repository().await;
        let data = seed_leaderboard_scenario(&repo.pool).await;
        repo.refresh().await.unwrap();

        // 1. Fetch the Middle User (should be Rank 2)
        let entry = repo.get_raider_entry(data.raid_id, &data.user_mid).await.unwrap();

        assert!(entry.is_some());
        let stats = entry.unwrap();

        assert_eq!(stats.raider.address, data.user_mid);
        assert_eq!(stats.rank, Some(2)); // Confirms CTE rank calculation works for single row
        assert_eq!(stats.total_impressions, Some(500));
    }

    #[tokio::test]
    async fn test_pagination() {
        let repo = setup_test_repository().await;
        let data = seed_leaderboard_scenario(&repo.pool).await;
        repo.refresh().await.unwrap();

        // 1. Limit 1, Offset 1 (Should skip Rank 1, return Rank 2)
        let page = repo.get_entries(data.raid_id, 1, 1).await.unwrap();

        assert_eq!(page.len(), 1);
        assert_eq!(page[0].raider.address, data.user_mid);
        assert_eq!(page[0].rank, Some(2));
    }

    #[tokio::test]
    async fn test_view_is_isolated_by_raid_id() {
        let repo = setup_test_repository().await;
        // 1. This creates the "Active" raid starting NOW()
        let data = seed_leaderboard_scenario(&repo.pool).await;

        // 2. Create a separate, "Old" raid that definitively ended in the past.
        //    Using "NOW() - INTERVAL '1 HOUR'" for the end_date ensures it does not
        //    overlap with the raid created in step 1.
        let other_raid_id: i32 = sqlx::query_scalar(
            "INSERT INTO raid_quests (name, start_date, end_date) 
             VALUES ('Other Raid', NOW() - INTERVAL '2 DAYS', NOW() - INTERVAL '1 HOUR') 
             RETURNING id",
        )
        .fetch_one(&repo.pool)
        .await
        .unwrap();

        // 3. Refresh to include this new raid in the view
        repo.refresh().await.unwrap();

        // Query the empty/old raid
        let entries = repo.get_entries(other_raid_id, 10, 0).await.unwrap();
        assert!(entries.is_empty(), "Should not return entries from a different raid");

        // Query the populated raid
        let entries_main = repo.get_entries(data.raid_id, 10, 0).await.unwrap();
        assert_eq!(entries_main.len(), 3);
    }
}
