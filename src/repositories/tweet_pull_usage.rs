use crate::db_persistence::DbError;
use crate::models::tweet_pull_usage::TweetPullUsage;
use chrono::{Datelike, NaiveDate, Utc};
use sqlx::PgPool;

#[derive(Debug, Clone)]
pub struct TweetPullUsageRepository {
    pool: PgPool,
}

impl TweetPullUsageRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    fn get_current_period(reset_day: u32) -> String {
        Self::calculate_period_for_date(Utc::now(), reset_day)
    }

    /// Calculates the billing period string (YYYY-MM) based on the given date and a reset day.
    ///
    /// The logic handles edge cases where the reset day (e.g., 31st) doesn't exist in the current month.
    /// In such cases, the reset day effectively becomes the last day of that month.
    ///
    /// Note on Month Label Shifting:
    /// If `reset_day` is 31, the "2023-01" cycle starts on Jan 31st and ends on Feb 27th.
    /// The "2023-02" cycle starts on Feb 28th and ends on Mar 30th.
    /// This means most of the usage for the "2023-01" period actually occurs in February.
    /// This is consistent with most billing systems that reset on the last day of the month
    /// for shorter months, but it means the month label refers to the START of the billing cycle.
    fn calculate_period_for_date(date: chrono::DateTime<Utc>, reset_day: u32) -> String {
        let current_year = date.year();
        let current_month = date.month();
        let current_day = date.day();

        // 1. Determine the effective reset day for the CURRENT month.
        //    If `reset_day` exceeds the days in current month, cap it at the last day of the month.
        let days_in_current_month = get_days_in_month(current_year, current_month);
        let effective_reset_day_current_month = std::cmp::min(reset_day, days_in_current_month);

        // 2. Compare current day with the effective reset day.
        if current_day >= effective_reset_day_current_month {
            // We are in the cycle starting this month.
            format!("{}-{:02}", current_year, current_month)
        } else {
            // We are in the cycle that started last month.
            let (prev_year, prev_month) = if current_month == 1 {
                (current_year - 1, 12)
            } else {
                (current_year, current_month - 1)
            };
            format!("{}-{:02}", prev_year, prev_month)
        }
    }

    pub async fn increment_usage(&self, amount: i32, reset_day: u32) -> Result<TweetPullUsage, DbError> {
        let period = Self::get_current_period(reset_day);
        self.increment_usage_for_period(amount, &period).await
    }

    /// Internal helper to increment usage for a specific period string.
    async fn increment_usage_for_period(&self, amount: i32, period: &str) -> Result<TweetPullUsage, DbError> {
        let usage = sqlx::query_as::<_, TweetPullUsage>(
            "INSERT INTO tweet_pull_usage (period, tweet_count) 
             VALUES ($1, $2) 
             ON CONFLICT (period) DO UPDATE 
             SET tweet_count = tweet_pull_usage.tweet_count + EXCLUDED.tweet_count
             RETURNING *",
        )
        .bind(period)
        .bind(amount)
        .fetch_one(&self.pool)
        .await
        .map_err(DbError::Database)?;

        Ok(usage)
    }
}

/// Helper to get the number of days in a given month/year.
fn get_days_in_month(year: i32, month: u32) -> u32 {
    // If month is December (12), next month is Jan (1) of next year.
    // Otherwise, just next month of same year.
    let (next_year, next_month) = if month == 12 { (year + 1, 1) } else { (year, month + 1) };

    // The '0th' day of the next month is the last day of the current month.
    // We get the date of the 1st of the next month, subtract 1 day.
    NaiveDate::from_ymd_opt(next_year, next_month, 1)
        .unwrap()
        .pred_opt()
        .unwrap()
        .day()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::utils::test_app_state::create_test_app_state;
    use crate::utils::test_db::reset_database;
    use chrono::{TimeZone, Utc};
    use sqlx::{Pool, Postgres};

    async fn get_current_usage(pool: &Pool<Postgres>, reset_day: u32) -> Result<TweetPullUsage, DbError> {
        let period = TweetPullUsageRepository::get_current_period(reset_day);
        get_usage_for_period(pool, &period).await
    }

    /// Internal helper to get usage for a specific period string.
    async fn get_usage_for_period(pool: &Pool<Postgres>, period: &str) -> Result<TweetPullUsage, DbError> {
        let usage = sqlx::query_as::<_, TweetPullUsage>(
            "INSERT INTO tweet_pull_usage (period, tweet_count) 
             VALUES ($1, 0) 
             ON CONFLICT (period) DO UPDATE SET period = EXCLUDED.period
             RETURNING *",
        )
        .bind(period)
        .fetch_one(pool)
        .await
        .map_err(DbError::Database)?;

        Ok(usage)
    }

    #[tokio::test]
    async fn test_get_current_usage_integration() {
        let state = create_test_app_state().await;
        reset_database(&state.db.pool).await;

        let reset_day = 1;

        // 1. Initial call should create a record with 0
        let usage = get_current_usage(&state.db.pool, reset_day).await.unwrap();
        assert_eq!(usage.tweet_count, 0);

        // 2. Subsequent call should return the same record
        let usage2 = get_current_usage(&state.db.pool, reset_day).await.unwrap();
        assert_eq!(usage2.tweet_count, 0);
        assert_eq!(usage.period, usage2.period);
    }

    #[tokio::test]
    async fn test_increment_usage_integration() {
        let state = create_test_app_state().await;
        reset_database(&state.db.pool).await;

        let repo = &state.db.tweet_pull_usage;
        let reset_day = 1;

        // 1. Increment from zero
        let usage = repo.increment_usage(10, reset_day).await.unwrap();
        assert_eq!(usage.tweet_count, 10);

        // 2. Increment again
        let usage2 = repo.increment_usage(5, reset_day).await.unwrap();
        assert_eq!(usage2.tweet_count, 15);
    }

    #[tokio::test]
    async fn test_transition_between_months_integration() {
        let state = create_test_app_state().await;
        reset_database(&state.db.pool).await;

        let repo = &state.db.tweet_pull_usage;

        // 1. Increment for Month A
        let period_a = "2023-01";
        repo.increment_usage_for_period(100, period_a).await.unwrap();

        // 2. Increment for Month B
        let period_b = "2023-02";
        repo.increment_usage_for_period(50, period_b).await.unwrap();

        // 3. Verify they are separate
        let usage_a = get_usage_for_period(&state.db.pool, period_a).await.unwrap();
        let usage_b = get_usage_for_period(&state.db.pool, period_b).await.unwrap();

        assert_eq!(usage_a.tweet_count, 100);
        assert_eq!(usage_b.tweet_count, 50);
        assert_ne!(usage_a.period, usage_b.period);
    }

    #[test]
    fn test_period_standard_reset() {
        // Reset on 7th. Current is Jan 5th. Should be Dec cycle.
        let date = Utc.with_ymd_and_hms(2023, 1, 5, 0, 0, 0).unwrap();
        assert_eq!(TweetPullUsageRepository::calculate_period_for_date(date, 7), "2022-12");

        // Reset on 7th. Current is Jan 7th. Should be Jan cycle.
        let date = Utc.with_ymd_and_hms(2023, 1, 7, 0, 0, 0).unwrap();
        assert_eq!(TweetPullUsageRepository::calculate_period_for_date(date, 7), "2023-01");
    }

    #[test]
    fn test_period_end_of_month_reset_short_feb() {
        // Reset on 30th.
        // Feb 2023 has 28 days.

        // Date: Feb 27th.
        // Effective reset for Feb is 28th (min(30, 28)).
        // 27 < 28 -> Previous cycle (Jan).
        let date = Utc.with_ymd_and_hms(2023, 2, 27, 0, 0, 0).unwrap();
        assert_eq!(TweetPullUsageRepository::calculate_period_for_date(date, 30), "2023-01");

        // Date: Feb 28th.
        // 28 >= 28 -> Current cycle (Feb).
        let date = Utc.with_ymd_and_hms(2023, 2, 28, 0, 0, 0).unwrap();
        assert_eq!(TweetPullUsageRepository::calculate_period_for_date(date, 30), "2023-02");
    }

    #[test]
    fn test_period_leap_year() {
        // Reset on 30th.
        // Feb 2024 has 29 days.

        // Date: Feb 28th.
        // Effective reset for Feb is 29th (min(30, 29)).
        // 28 < 29 -> Previous cycle (Jan).
        let date = Utc.with_ymd_and_hms(2024, 2, 28, 0, 0, 0).unwrap();
        assert_eq!(TweetPullUsageRepository::calculate_period_for_date(date, 30), "2024-01");

        // Date: Feb 29th.
        // 29 >= 29 -> Current cycle (Feb).
        let date = Utc.with_ymd_and_hms(2024, 2, 29, 0, 0, 0).unwrap();
        assert_eq!(TweetPullUsageRepository::calculate_period_for_date(date, 30), "2024-02");
    }

    #[test]
    fn test_period_new_year() {
        // Reset on 31st. Current is Jan 1st 2024.
        // Effective reset for Jan is 31st.
        // 1 < 31 -> Previous cycle (Dec 2023).
        let date = Utc.with_ymd_and_hms(2024, 1, 1, 0, 0, 0).unwrap();
        assert_eq!(TweetPullUsageRepository::calculate_period_for_date(date, 31), "2023-12");
    }

    #[test]
    fn test_period_reset_day_31_feb_edge_cases() {
        // Reset on 31st.

        // Jan 31st -> Starts Jan cycle.
        let date = Utc.with_ymd_and_hms(2023, 1, 31, 0, 0, 0).unwrap();
        assert_eq!(TweetPullUsageRepository::calculate_period_for_date(date, 31), "2023-01");

        // Feb 27th -> Still in Jan cycle (started Jan 31).
        let date = Utc.with_ymd_and_hms(2023, 2, 27, 0, 0, 0).unwrap();
        assert_eq!(TweetPullUsageRepository::calculate_period_for_date(date, 31), "2023-01");

        // Feb 28th -> Starts Feb cycle because Feb only has 28 days.
        // The reset is "capped" at the last day of the month.
        let date = Utc.with_ymd_and_hms(2023, 2, 28, 0, 0, 0).unwrap();
        assert_eq!(TweetPullUsageRepository::calculate_period_for_date(date, 31), "2023-02");

        // Mar 1st -> Still in Feb cycle (started Feb 28).
        let date = Utc.with_ymd_and_hms(2023, 3, 1, 0, 0, 0).unwrap();
        assert_eq!(TweetPullUsageRepository::calculate_period_for_date(date, 31), "2023-02");

        // Mar 30th -> Still in Feb cycle.
        let date = Utc.with_ymd_and_hms(2023, 3, 30, 0, 0, 0).unwrap();
        assert_eq!(TweetPullUsageRepository::calculate_period_for_date(date, 31), "2023-02");

        // Mar 31st -> Starts Mar cycle.
        let date = Utc.with_ymd_and_hms(2023, 3, 31, 0, 0, 0).unwrap();
        assert_eq!(TweetPullUsageRepository::calculate_period_for_date(date, 31), "2023-03");
    }
}
