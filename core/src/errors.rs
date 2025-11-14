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

    #[error("Initialization error: {0}")]
    InitError(String),

    #[error("Network error: {0}")]
    NetworkError(String),

    #[error("Serialization error: {0}")]
    SerializationError(String),

    #[error("Timeout: {0}")]
    Timeout(String),

    #[error("Invalid assignment: {0}")]
    InvalidAssignment(String),
}

pub type Result<T> = std::result::Result<T, MevError>;
