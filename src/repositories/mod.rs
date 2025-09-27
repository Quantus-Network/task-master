use crate::errors::DbError;

pub type DbResult<T> = Result<T, DbError>;

pub mod address;
pub mod task;
pub mod referral;