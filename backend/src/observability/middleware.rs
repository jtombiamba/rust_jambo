use std::future::{ready, Ready};

use actix_web::{
    body::MessageBody,
    dev::{forward_ready, Service, ServiceRequest, ServiceResponse, Transform},
    Error, HttpMessage,
};
use uuid::Uuid;

use super::metrics;
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

        Box::pin(async move {
            let correlation_id_uuid = correlation_id.0;

            super::CORRELATION_ID
                .scope(correlation_id_uuid, async move {
                    // Enter the span manually so that record() calls happen while the span is active.
                    // Using .instrument() on the future would exit the span before we can record fields.
                    let _guard = span.enter();
                    let result = fut.await;
                    let duration = start.elapsed();

                    let status_code = match &result {
                        Ok(resp) => resp.status().as_u16(),
                        Err(_) => 500u16,
                    };

                    span.record("http.duration_ms", duration.as_millis() as u64);
                    span.record("http.status_code", status_code);

                    tracing::info!("request completed");

                    let method_str = method.as_str();
                    let path_segments: Vec<&str> = path.split('/').collect();
                    let normalized_path = if path_segments.len() >= 4 && path_segments[1] == "api" {
                        let parts: Vec<&str> = path_segments
                            .iter()
                            .map(|seg| {
                                if Uuid::parse_str(seg).is_ok() {
                                    "{id}"
                                } else {
                                    seg
                                }
                            })
                            .collect();
                        parts.join("/").to_string()
                    } else {
                        path.clone()
                    };

                    metrics::HTTP_REQUESTS_TOTAL
                        .with_label_values(&[
                            method_str,
                            &normalized_path,
                            &status_code.to_string(),
                        ])
                        .inc();
                    metrics::HTTP_REQUEST_DURATION_SECONDS
                        .with_label_values(&[method_str, &normalized_path])
                        .observe(duration.as_secs_f64());

                    match result {
                        Ok(response) => {
                            let correlation_id_str = correlation_id_uuid.to_string();
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
                        Err(e) => Err(e),
                    }
                })
                .await
        })
    }
}
