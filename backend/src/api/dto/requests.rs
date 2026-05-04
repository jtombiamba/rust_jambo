use serde::Deserialize;
use uuid::Uuid;

use crate::error::ValidationError;

#[derive(Debug, Deserialize)]
pub struct PlayCardRequest {
    pub player_id: Uuid,
    pub card_index: i32,
}

impl PlayCardRequest {
    pub fn validate(&self) -> Result<(), ValidationError> {
        if !(0..32).contains(&self.card_index) {
            return Err(ValidationError::CardIndexOutOfRange(self.card_index));
        }
        Ok(())
    }
}
