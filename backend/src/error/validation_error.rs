use thiserror::Error;

#[derive(Debug, Error)]
pub enum ValidationError {
    #[error("Missing required field: {0}")]
    #[allow(dead_code)]
    MissingField(String),

    #[error("Card index {0} out of valid range (0-31)")]
    CardIndexOutOfRange(i32),

    #[error("Invalid player type: {0}")]
    #[allow(dead_code)]
    InvalidPlayerType(String),
}
