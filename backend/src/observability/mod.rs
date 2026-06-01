pub mod metrics;
pub mod middleware;
pub mod ws;

use uuid::Uuid;

tokio::task_local! {
    pub static CORRELATION_ID: Uuid;
}

/// Newtype for safe storage in actix_web::dev::Extensions.
#[derive(Debug, Clone, Copy)]
pub struct CorrelationId(pub Uuid);

impl Default for CorrelationId {
    fn default() -> Self {
        Self::new()
    }
}

impl CorrelationId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl std::fmt::Display for CorrelationId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<Uuid> for CorrelationId {
    fn from(uuid: Uuid) -> Self {
        Self(uuid)
    }
}
