use std::collections::HashMap;
use std::future::{ready, Ready};
use std::sync::Arc;
use std::time::{Duration, Instant};

use actix_web::{
    body::{EitherBody, MessageBody},
    dev::{forward_ready, Service, ServiceRequest, ServiceResponse, Transform},
    Error,
};
use tokio::sync::Mutex;

use crate::observability::metrics;

#[allow(dead_code)]
static HOURLY_LIMIT: u64 = 120;
#[allow(dead_code)]
static CLEANUP_INTERVAL_SECS: u64 = 3600;

#[allow(dead_code)]
struct RateLimitState {
    count: u64,
    reset_at: Instant,
}

#[allow(dead_code)]
#[derive(Clone)]
pub struct RateLimiter {
    inner: Arc<Mutex<HashMap<String, RateLimitState>>>,
}

#[allow(dead_code)]
impl RateLimiter {
    pub fn new() -> Self {
        let limiter = Self {
            inner: Arc::new(Mutex::new(HashMap::new())),
        };
        limiter.spawn_cleanup();
        limiter
    }

    fn spawn_cleanup(&self) {
        let inner = self.inner.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(CLEANUP_INTERVAL_SECS));
            loop {
                interval.tick().await;
                let mut map = inner.lock().await;
                let now = Instant::now();
                map.retain(|_, entry| entry.reset_at > now);
                tracing::debug!("Rate limiter cleanup: {} entries remaining", map.len());
            }
        });
    }

    async fn check(&self, ip: &str) -> Result<(), ()> {
        let mut map = self.inner.lock().await;
        let now = Instant::now();
        let entry = map.entry(ip.to_string()).or_insert(RateLimitState {
            count: 0,
            reset_at: now + Duration::from_secs(3600),
        });

        if entry.reset_at <= now {
            entry.count = 0;
            entry.reset_at = now + Duration::from_secs(3600);
        }

        if entry.count >= HOURLY_LIMIT {
            return Err(());
        }

        entry.count += 1;
        Ok(())
    }
}

impl Default for RateLimiter {
    fn default() -> Self {
        Self::new()
    }
}

#[allow(dead_code)]
pub struct RateLimiterMiddleware {
    limiter: RateLimiter,
}

#[allow(dead_code)]
impl RateLimiterMiddleware {
    pub fn new() -> Self {
        Self {
            limiter: RateLimiter::new(),
        }
    }
}

impl Default for RateLimiterMiddleware {
    fn default() -> Self {
        Self::new()
    }
}

impl<S, B> Transform<S, ServiceRequest> for RateLimiterMiddleware
where
    S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = Error> + 'static,
    S::Future: 'static,
    B: MessageBody + 'static,
{
    type Response = ServiceResponse<EitherBody<B>>;
    type Error = Error;
    type InitError = ();
    type Transform = RateLimiterService<S>;
    type Future = Ready<Result<Self::Transform, Self::InitError>>;

    fn new_transform(&self, service: S) -> Self::Future {
        ready(Ok(RateLimiterService {
            service,
            limiter: self.limiter.clone(),
        }))
    }
}

#[allow(dead_code)]
pub struct RateLimiterService<S> {
    service: S,
    limiter: RateLimiter,
}

impl<S, B> Service<ServiceRequest> for RateLimiterService<S>
where
    S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = Error>,
    S::Future: 'static,
    B: MessageBody + 'static,
{
    type Response = ServiceResponse<EitherBody<B>>;
    type Error = Error;
    type Future =
        std::pin::Pin<Box<dyn std::future::Future<Output = Result<Self::Response, Self::Error>>>>;

    forward_ready!(service);

    fn call(&self, req: ServiceRequest) -> Self::Future {
        let ip = req
            .connection_info()
            .realip_remote_addr()
            .unwrap_or("unknown")
            .to_string();

        let limiter = self.limiter.clone();
        // Create the inner service future before the async block to avoid lifetime issues
        // with borrowing self.service inside the async block.
        let fut = self.service.call(req);

        Box::pin(async move {
            if limiter.check(&ip).await.is_err() {
                metrics::RATE_LIMIT_HITS_TOTAL.inc();
                tracing::warn!("Rate limit exceeded for IP: {}", ip);
                // Return an error instead of constructing a ServiceResponse with a cloned request.
                // Cloning HttpRequest (which uses Rc internally) would cause Rc::get_mut() to
                // return None and panic when actix-web later calls match_info_mut() on the original.
                return Err(actix_web::error::ErrorTooManyRequests(
                    "Rate limit exceeded. Please try again later.",
                ));
            }

            fut.await.map(|res| res.map_into_left_body())
        })
    }
}
