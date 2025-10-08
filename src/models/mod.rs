#[derive(Debug, thiserror::Error)]
pub enum ModelError {
    #[error("Invalid data input")]
    InvalidInput,
    #[error("Failed generating checkphrase")]
    FailedGenerateCheckphrase,
}

pub type ModelResult<T> = Result<T, ModelError>;

pub mod address;
pub mod referrals;
pub mod task;
