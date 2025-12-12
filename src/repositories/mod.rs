use sqlx::{Postgres, QueryBuilder};

use crate::db_persistence::DbError;

pub type DbResult<T> = Result<T, DbError>;

pub mod address;
pub mod admin;
pub mod eth_association;
pub mod opt_in;
pub mod raid_quest;
pub mod referral;
pub mod relevant_tweet;
pub mod task;
pub mod tweet_author;
pub mod x_association;

pub trait QueryBuilderExt {
    fn push_condition(&mut self, sql: &str, where_started: &mut bool);
}

impl<'a> QueryBuilderExt for QueryBuilder<'a, Postgres> {
    fn push_condition(&mut self, sql: &str, where_started: &mut bool) {
        if *where_started {
            self.push(" AND ");
        } else {
            self.push(" WHERE ");
            *where_started = true;
        }
        self.push(sql);
    }
}

pub fn calculate_page_offset(page: u32, page_size: u32) -> u32 {
    (page - 1) * page_size
}
