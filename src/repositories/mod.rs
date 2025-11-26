use crate::db_persistence::DbError;

pub type DbResult<T> = Result<T, DbError>;

pub mod address;
pub mod opt_in;
pub mod referral;
pub mod task;
