use std::future::{ready, Ready};

use actix_web::{
    body::MessageBody,
    dev::{forward_ready, Service, ServiceRequest, ServiceResponse, Transform},
    Error, HttpMessage,
};

use crate::auth::config::AuthConfig;
use crate::auth::extractors::ClientIp;

pub struct ForwardedIpMiddleware;

impl<S, B> Transform<S, ServiceRequest> for ForwardedIpMiddleware
where
    S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = Error>,
    S::Future: 'static,
    B: MessageBody + 'static,
{
    type Response = ServiceResponse<B>;
    type Error = Error;
    type InitError = ();
    type Transform = ForwardedIpMiddlewareService<S>;
    type Future = Ready<Result<Self::Transform, Self::InitError>>;

    fn new_transform(&self, service: S) -> Self::Future {
        ready(Ok(ForwardedIpMiddlewareService { service }))
    }
}

pub struct ForwardedIpMiddlewareService<S> {
    service: S,
}

impl<S, B> Service<ServiceRequest> for ForwardedIpMiddlewareService<S>
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
        let auth_config = req.app_data::<actix_web::web::Data<AuthConfig>>().cloned();

        let pepper = auth_config
            .map(|c| c.ip_hash_pepper.clone())
            .unwrap_or_default();

        let raw_ip = extract_request_ip(&req);
        let client_ip = ClientIp::from_raw_ip(&raw_ip, &pepper);
        req.extensions_mut().insert(client_ip);

        let fut = self.service.call(req);
        Box::pin(fut)
    }
}

fn extract_request_ip(req: &ServiceRequest) -> String {
    req.headers()
        .get("X-Forwarded-For")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.split(',').next().map(|ip| ip.trim().to_string()))
        .or_else(|| {
            req.headers()
                .get("X-Real-IP")
                .and_then(|v| v.to_str().ok())
                .map(|s| s.to_string())
        })
        .unwrap_or_else(|| {
            req.peer_addr()
                .map(|addr| addr.ip().to_string())
                .unwrap_or_else(|| "unknown".to_string())
        })
}
