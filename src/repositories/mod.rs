use crate::db_persistence::DbError;

pub type DbResult<T> = Result<T, DbError>;

pub mod address;
pub mod admin;
pub mod eth_association;
pub mod opt_in;
pub mod referral;
pub mod task;
pub mod x_association;

pub fn calculate_page_offset(page: u32, page_size: u32) -> u32 {
    (page - 1) * page_size
}
