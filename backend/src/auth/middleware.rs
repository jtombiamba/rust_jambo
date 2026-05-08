use std::future::{ready, Ready};

use actix_web::{
    body::MessageBody,
    dev::{forward_ready, Service, ServiceRequest, ServiceResponse, Transform},
    Error, HttpMessage,
};
use serde_json::json;

use super::config::AuthConfig;
use super::extractors::AuthenticatedUser;
use super::jwt;

pub struct AuthMiddleware;

impl<S, B> Transform<S, ServiceRequest> for AuthMiddleware
where
    S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = Error>,
    S::Future: 'static,
    B: MessageBody + 'static,
{
    type Response = ServiceResponse<B>;
    type Error = Error;
    type InitError = ();
    type Transform = AuthMiddlewareService<S>;
    type Future = Ready<Result<Self::Transform, Self::InitError>>;

    fn new_transform(&self, service: S) -> Self::Future {
        ready(Ok(AuthMiddlewareService { service }))
    }
}

pub struct AuthMiddlewareService<S> {
    service: S,
}

impl<S, B> Service<ServiceRequest> for AuthMiddlewareService<S>
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

        let token = req.cookie("Authorization").map(|c| c.value().to_string());

        let is_authenticated = match (&auth_config, &token) {
            (Some(config), Some(t)) => match jwt::validate_token(t, config) {
                Ok(claims) => {
                    req.extensions_mut().insert(AuthenticatedUser {
                        user_id: claims.sub,
                    });
                    true
                }
                Err(_) => false,
            },
            _ => false,
        };

        if is_authenticated {
            let fut = self.service.call(req);
            Box::pin(fut)
        } else {
            let error = actix_web::error::ErrorUnauthorized(
                json!({"success": false, "error": "Authentication required"}).to_string(),
            );
            Box::pin(async move { Err(error) })
        }
    }
}
