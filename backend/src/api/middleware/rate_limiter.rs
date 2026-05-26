use std::future::{ready, Ready};
use std::sync::Arc;

use actix_web::{
    body::{EitherBody, MessageBody},
    dev::{forward_ready, Service, ServiceRequest, ServiceResponse, Transform},
    Error,
};

use crate::i18n::{extract_lang, Translator};
use crate::messaging::RedisClient;
use crate::observability::metrics;

static HOURLY_LIMIT: u64 = 120;

#[derive(Clone, Default)]
pub struct RateLimiter {
    redis_client: Option<RedisClient>,
}

impl RateLimiter {
    pub fn new(redis_client: Option<RedisClient>) -> Self {
        Self { redis_client }
    }

    async fn check(&self, ip: &str) -> Result<(), ()> {
        let mut redis = match self.redis_client.clone() {
            Some(r) => r,
            None => return Ok(()),
        };

        let key = format!("ratelimit:hourly:{ip}");
        let count: u64 = redis.incr(&key).await.unwrap_or_else(|e| {
            tracing::error!("Rate limiter Redis error: {}", e);
            0
        });

        if count == 1 {
            let _ = redis.expire(&key, 3600).await;
        }

        if count > HOURLY_LIMIT {
            return Err(());
        }

        Ok(())
    }
}

#[derive(Clone)]
pub struct RateLimiterMiddleware {
    limiter: RateLimiter,
    translator: Arc<Translator>,
}

impl RateLimiterMiddleware {
    pub fn new(redis_client: Option<RedisClient>, translator: Arc<Translator>) -> Self {
        Self {
            limiter: RateLimiter::new(redis_client),
            translator,
        }
    }
}

impl Default for RateLimiterMiddleware {
    fn default() -> Self {
        Self::new(None, Arc::new(Translator::new()))
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
            translator: self.translator.clone(),
        }))
    }
}

pub struct RateLimiterService<S> {
    service: S,
    limiter: RateLimiter,
    translator: Arc<Translator>,
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

        let lang = extract_lang(&req);
        let translator = self.translator.clone();
        let limiter = self.limiter.clone();
        let fut = self.service.call(req);

        Box::pin(async move {
            if limiter.check(&ip).await.is_err() {
                metrics::RATE_LIMIT_HITS_TOTAL.inc();
                tracing::warn!("Rate limit exceeded for IP: {}", ip);
                return Err(actix_web::error::ErrorTooManyRequests(
                    translator.t("rate_limit.exceeded", lang),
                ));
            }

            fut.await.map(|res| res.map_into_left_body())
        })
    }
}
