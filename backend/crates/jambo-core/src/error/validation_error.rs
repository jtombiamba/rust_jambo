use thiserror::Error;

#[derive(Debug, Error)]
pub enum ValidationError {
    #[error("Missing required field: {0}")]
    MissingField(String),

    #[error("Card index {0} out of valid range (0-31)")]
    CardIndexOutOfRange(i32),
}

impl ValidationError {
    pub fn source(&self) -> &'static str {
        match self {
            ValidationError::MissingField(_) => "validation:missing_field",
            ValidationError::CardIndexOutOfRange(_) => "validation:card_index_out_of_range",
        }
    }

    pub fn message_key(&self) -> &'static str {
        match self {
            ValidationError::MissingField(_) => "validation.missing_field",
            ValidationError::CardIndexOutOfRange(_) => "validation.card_index_range",
        }
    }

    pub fn params(&self) -> Vec<(&'static str, String)> {
        match self {
            ValidationError::MissingField(field) => vec![("{0}", field.clone())],
            ValidationError::CardIndexOutOfRange(index) => vec![("{0}", index.to_string())],
        }
    }
}
