use std::collections::HashMap;
use std::future::{ready, Ready};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use actix_web::{
    body::{EitherBody, MessageBody},
    dev::{forward_ready, Service, ServiceRequest, ServiceResponse, Transform},
    Error, HttpMessage, HttpResponse,
};
use redis::RedisError;

use crate::api::dto::responses::ApiErrorResponse;
use crate::i18n::{extract_lang, Translator};
use crate::messaging::RedisClient;
use crate::observability::metrics;

#[derive(Clone, Debug)]
pub struct RateLimitConfig {
    pub max_requests: u64,
    pub window_seconds: u64,
    pub key_prefix: &'static str,
}

#[derive(Clone)]
pub struct RateLimitConfigs {
    #[allow(dead_code)]
    pub default: RateLimitConfig,
    pub contact: RateLimitConfig,
    pub register: RateLimitConfig,
    pub login: RateLimitConfig,
    pub forgot_password: RateLimitConfig,
    pub reset_password: RateLimitConfig,
}

impl RateLimitConfigs {
    pub fn from_config(config: &crate::config::Config) -> Self {
        Self {
            default: RateLimitConfig {
                max_requests: config.rate_limit_default_max_requests,
                window_seconds: config.rate_limit_default_window_seconds,
                key_prefix: "default",
            },
            contact: RateLimitConfig {
                max_requests: config.rate_limit_contact_max_requests,
                window_seconds: config.rate_limit_contact_window_seconds,
                key_prefix: "contact",
            },
            register: RateLimitConfig {
                max_requests: config.rate_limit_register_max_requests,
                window_seconds: config.rate_limit_register_window_seconds,
                key_prefix: "register",
            },
            login: RateLimitConfig {
                max_requests: config.rate_limit_login_max_requests,
                window_seconds: config.rate_limit_login_window_seconds,
                key_prefix: "login",
            },
            forgot_password: RateLimitConfig {
                max_requests: config.rate_limit_forgot_password_max_requests,
                window_seconds: config.rate_limit_forgot_password_window_seconds,
                key_prefix: "forgot_password",
            },
            reset_password: RateLimitConfig {
                max_requests: config.rate_limit_reset_password_max_requests,
                window_seconds: config.rate_limit_reset_password_window_seconds,
                key_prefix: "reset_password",
            },
        }
    }
}

#[derive(Clone, Debug)]
pub struct RateLimitCheckResult {
    pub allowed: bool,
    pub retry_after_secs: u64,
}

impl RateLimitCheckResult {
    fn blocked(retry_after_secs: u64) -> Self {
        Self {
            allowed: false,
            retry_after_secs,
        }
    }

    fn allowed() -> Self {
        Self {
            allowed: true,
            retry_after_secs: 0,
        }
    }

    fn fail_closed() -> Self {
        Self {
            allowed: false,
            retry_after_secs: 60,
        }
    }
}

#[derive(Default)]
struct InMemoryRateLimiter {
    records: Mutex<HashMap<String, Vec<Instant>>>,
}

impl InMemoryRateLimiter {
    fn check(&self, ip: &str, config: &RateLimitConfig) -> RateLimitCheckResult {
        let key = format!("{}:{}", config.key_prefix, ip);
        let mut records = match self.records.lock() {
            Ok(r) => r,
            Err(_) => {
                tracing::error!("Rate limiter in-memory store mutex poisoned");
                return RateLimitCheckResult::fail_closed();
            }
        };

        let now = Instant::now();
        let window = Duration::from_secs(config.window_seconds);
        let timestamps = records.entry(key).or_default();

        timestamps.retain(|t| now.duration_since(*t) < window);

        if timestamps.len() >= config.max_requests as usize {
            let retry_after = timestamps
                .first()
                .map(|oldest| {
                    window
                        .saturating_sub(now.duration_since(*oldest))
                        .as_secs()
                        .max(1)
                })
                .unwrap_or(config.window_seconds);
            return RateLimitCheckResult::blocked(retry_after);
        }

        timestamps.push(now);
        RateLimitCheckResult::allowed()
    }
}

#[derive(Clone)]
pub struct RateLimiter {
    redis_client: Option<RedisClient>,
    in_memory: Arc<InMemoryRateLimiter>,
    config: RateLimitConfig,
    fallback_warned: Arc<AtomicBool>,
}

impl RateLimiter {
    pub fn new(redis_client: Option<RedisClient>, config: RateLimitConfig) -> Self {
        Self {
            redis_client,
            in_memory: Arc::new(InMemoryRateLimiter::default()),
            config,
            fallback_warned: Arc::new(AtomicBool::new(false)),
        }
    }

    pub fn config(&self) -> &RateLimitConfig {
        &self.config
    }

    async fn check(&self, ip: &str) -> RateLimitCheckResult {
        let key = format!("ratelimit:{}:{}", self.config.key_prefix, ip);

        if let Some(mut redis) = self.redis_client.clone() {
            match check_redis(&mut redis, &key, &self.config).await {
                Ok(result) => return result,
                Err(e) => {
                    tracing::warn!(
                        "Rate limiter Redis error for prefix '{}', falling back to in-memory: {}",
                        self.config.key_prefix,
                        e
                    );
                }
            }
        }

        if !self.fallback_warned.swap(true, Ordering::Relaxed) {
            tracing::warn!(
                "Rate limiter using in-memory store for prefix '{}'",
                self.config.key_prefix
            );
        }
        tracing::debug!(
            "Rate limiter in-memory check for prefix '{}', IP: {}",
            self.config.key_prefix,
            ip
        );

        self.in_memory.check(ip, &self.config)
    }
}

async fn check_redis(
    redis: &mut RedisClient,
    key: &str,
    config: &RateLimitConfig,
) -> Result<RateLimitCheckResult, RedisError> {
    let count = redis.incr_with_expire(key, config.window_seconds).await?;

    if count <= config.max_requests {
        Ok(RateLimitCheckResult::allowed())
    } else {
        Ok(RateLimitCheckResult::blocked(config.window_seconds))
    }
}

#[derive(Clone)]
pub struct RateLimiterMiddleware {
    limiter: RateLimiter,
    translator: Arc<Translator>,
}

impl RateLimiterMiddleware {
    pub fn new(
        redis_client: Option<RedisClient>,
        config: RateLimitConfig,
        translator: Arc<Translator>,
    ) -> Self {
        Self {
            limiter: RateLimiter::new(redis_client, config),
            translator,
        }
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
            .extensions()
            .get::<crate::auth::extractors::ClientIp>()
            .map(|c| c.hash.clone())
            .unwrap_or_else(|| {
                req.peer_addr()
                    .map(|addr| addr.ip().to_string())
                    .unwrap_or_else(|| "unknown".to_string())
            });

        let lang = extract_lang(&req);
        let translator = self.translator.clone();
        let limiter = self.limiter.clone();
        let fut = self.service.call(req);

        Box::pin(async move {
            let result = limiter.check(&ip).await;

            if !result.allowed {
                metrics::RATE_LIMIT_HITS_TOTAL.inc();
                tracing::warn!(
                    "Rate limit exceeded for prefix '{}', IP: {}",
                    limiter.config().key_prefix,
                    ip
                );

                let response = HttpResponse::TooManyRequests()
                    .insert_header(("Retry-After", result.retry_after_secs.to_string()))
                    .json(ApiErrorResponse {
                        success: false,
                        error: translator.t("rate_limit.exceeded", lang),
                        field: None,
                        source: "rate_limiter".to_string(),
                        request_id: crate::observability::CORRELATION_ID
                            .try_with(|id| id.to_string())
                            .ok(),
                    });

                return Err(actix_web::error::InternalError::from_response(
                    "rate limit exceeded",
                    response,
                )
                .into());
            }

            fut.await.map(|res| res.map_into_left_body())
        })
    }
}
#[cfg(test)]
#[path = "rate_limiter_tests.rs"]
mod tests;
