use std::future::{ready, Ready};

use actix_web::{
    body::MessageBody,
    dev::{forward_ready, Service, ServiceRequest, ServiceResponse, Transform},
    Error, HttpMessage,
};
use tracing::Instrument;
use uuid::Uuid;

use super::CorrelationId;

pub struct CorrelationIdMiddleware;

impl<S, B> Transform<S, ServiceRequest> for CorrelationIdMiddleware
where
    S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = Error>,
    S::Future: 'static,
    B: MessageBody + 'static,
{
    type Response = ServiceResponse<B>;
    type Error = Error;
    type InitError = ();
    type Transform = CorrelationIdMiddlewareService<S>;
    type Future = Ready<Result<Self::Transform, Self::InitError>>;

    fn new_transform(&self, service: S) -> Self::Future {
        ready(Ok(CorrelationIdMiddlewareService { service }))
    }
}

pub struct CorrelationIdMiddlewareService<S> {
    service: S,
}

impl<S, B> Service<ServiceRequest> for CorrelationIdMiddlewareService<S>
where
    S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = Error>,
    S::Future: 'static,
    B: MessageBody + 'static,
{
    type Response = ServiceResponse<B>;
    type Error = Error;
    type Future =
        std::pin::Pin<Box<dyn std::future::Future<Output = Result<Self::Response, Self::Error>>>>;

    forward_ready!(service);

    fn call(&self, req: ServiceRequest) -> Self::Future {
        let correlation_id = req
            .headers()
            .get("X-Request-Id")
            .or_else(|| req.headers().get("X-Correlation-Id"))
            .and_then(|v| v.to_str().ok())
            .and_then(|s| Uuid::parse_str(s).ok())
            .map(CorrelationId::from)
            .unwrap_or_default();

        let method = req.method().clone();
        let path = req.path().to_string();
        let start = std::time::Instant::now();

        req.extensions_mut().insert(correlation_id);

        let span = tracing::info_span!(
            "http_request",
            correlation_id = %correlation_id,
            http.method = %method,
            http.path = %path,
            http.status_code = tracing::field::Empty,
            http.duration_ms = tracing::field::Empty,
        );

        let fut = self.service.call(req);
        let fut = fut.instrument(span.clone());

        Box::pin(async move {
            let result = fut.await;
            let duration = start.elapsed();
            span.record("http.duration_ms", duration.as_millis() as u64);

            match result {
                Ok(response) => {
                    span.record("http.status_code", response.status().as_u16());
                    let correlation_id_str = correlation_id.to_string();
                    let mut response = response;
                    let headers = response.headers_mut();
                    headers.insert(
                        actix_web::http::header::HeaderName::from_static("x-request-id"),
                        actix_web::http::header::HeaderValue::from_str(&correlation_id_str)
                            .unwrap_or(actix_web::http::header::HeaderValue::from_static(
                                "invalid",
                            )),
                    );
                    Ok(response)
                }
                Err(e) => {
                    span.record("http.status_code", 500u16);
                    Err(e)
                }
            }
        })
    }
}
