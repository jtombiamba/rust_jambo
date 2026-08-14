use std::sync::Arc;

use sea_orm::{DatabaseTransaction, TransactionTrait};

use crate::room::error::RoomServiceError;

#[async_trait::async_trait]
pub trait TransactionRunner: Send + Sync {
    async fn begin(&self) -> Result<DatabaseTransaction, RoomServiceError>;
    async fn commit(self: Arc<Self>, txn: DatabaseTransaction) -> Result<(), RoomServiceError>;
}

pub struct SeaOrmTransactionRunner {
    db: sea_orm::DatabaseConnection,
}

impl SeaOrmTransactionRunner {
    pub fn new(db: sea_orm::DatabaseConnection) -> Self {
        Self { db }
    }
}

#[async_trait::async_trait]
impl TransactionRunner for SeaOrmTransactionRunner {
    async fn begin(&self) -> Result<DatabaseTransaction, RoomServiceError> {
        self.db.begin().await.map_err(RoomServiceError::Database)
    }

    async fn commit(self: Arc<Self>, txn: DatabaseTransaction) -> Result<(), RoomServiceError> {
        txn.commit().await.map_err(RoomServiceError::Database)
    }
}
