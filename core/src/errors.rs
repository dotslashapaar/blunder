use thiserror::Error;

#[derive(Error, Debug)]
pub enum MevError {
    #[error("Bundle execution failed: {0}")]
    BundleExecutionFailed(String),

    #[error("Lock acquisition failed")]
    LockAcquisitionFailed,

    #[error("Scheduler error: {0}")]
    SchedulerError(String),

    #[error("Validation error: {0}")]
    ValidationError(String),

    #[error("Account not found: {0}")]
    AccountNotFound(String),

    #[error("Invalid transaction: {0}")]
    InvalidTransaction(String),
}

pub type Result<T> = std::result::Result<T, MevError>;
