use thiserror::Error;

#[derive(Error, Debug)]
pub enum ProcessingError {
    #[error("missing amount")]
    MissingAmount,
    #[error("invalid amount")]
    InvalidAmount,
    #[error("account locked")]
    AccountLocked,
    #[error("insufficient funds")]
    InsufficientFunds,
    #[error("transaction not found")]
    TransactionNotFound,
    #[error("client mismatch")]
    ClientMismatch,
    #[error("already disputed")]
    AlreadyDisputed,
    #[error("not disputed")]
    NotDisputed,
    #[error("duplicate transaction ID")]
    DuplicateTransaction,
    #[error("actor communication failed")]
    ActorCommunicationError,
}
