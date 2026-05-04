pub mod middleware;
pub mod ws;

use uuid::Uuid;

/// Newtype for safe storage in actix_web::dev::Extensions.
#[derive(Debug, Clone, Copy)]
pub struct CorrelationId(pub Uuid);

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

/// Extract the correlation ID from the current tracing span, if available.
/// Returns `None` if no correlation_id field is set on any active span.
pub fn current_correlation_id() -> Option<CorrelationId> {
    let span = tracing::Span::current();
    if span.is_none() {
        return None;
    }
    // tracing doesn't have a direct "get field value" API at runtime.
    // We rely on Propagation through request extensions or explicit parameters.
    None
}
