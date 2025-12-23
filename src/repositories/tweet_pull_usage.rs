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
    /// The logic handles edge cases where the reset day (e.g., 31st) doesn't exist in the current or previous month.
    /// In such cases, the reset day effectively becomes the last day of that month.
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

    pub async fn get_current_usage(&self, reset_day: u32) -> Result<TweetPullUsage, DbError> {
        let period = Self::get_current_period(reset_day);

        let usage = sqlx::query_as::<_, TweetPullUsage>(
            "INSERT INTO tweet_pull_usage (period, tweet_count) 
             VALUES ($1, 0) 
             ON CONFLICT (period) DO UPDATE SET period = EXCLUDED.period
             RETURNING *",
        )
        .bind(&period)
        .fetch_one(&self.pool)
        .await
        .map_err(DbError::Database)?;

        Ok(usage)
    }

    pub async fn increment_usage(&self, amount: i32, reset_day: u32) -> Result<TweetPullUsage, DbError> {
        let period = Self::get_current_period(reset_day);

        let usage = sqlx::query_as::<_, TweetPullUsage>(
            "INSERT INTO tweet_pull_usage (period, tweet_count) 
             VALUES ($1, $2) 
             ON CONFLICT (period) DO UPDATE 
             SET tweet_count = tweet_pull_usage.tweet_count + EXCLUDED.tweet_count
             RETURNING *",
        )
        .bind(&period)
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
    use chrono::{TimeZone, Utc};

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
}
