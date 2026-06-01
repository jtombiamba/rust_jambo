use std::future::{ready, Ready};
use std::sync::Arc;

use actix_web::{
    body::MessageBody,
    dev::{forward_ready, Service, ServiceRequest, ServiceResponse, Transform},
    Error, HttpMessage,
};

use super::config::AuthConfig;
use super::extractors::AuthenticatedUser;
use super::jwt;
use crate::api::dto::responses::ApiErrorResponse;
use crate::i18n::{extract_lang, Translator};
use crate::messaging::RedisClient;

#[derive(Clone)]
pub struct AuthMiddleware {
    redis_client: Option<RedisClient>,
    translator: Arc<Translator>,
}

impl AuthMiddleware {
    pub fn new(redis_client: Option<RedisClient>, translator: Arc<Translator>) -> Self {
        Self {
            redis_client,
            translator,
        }
    }
}

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
        ready(Ok(AuthMiddlewareService {
            service,
            redis_client: self.redis_client.clone(),
            translator: self.translator.clone(),
        }))
    }
}

pub struct AuthMiddlewareService<S> {
    service: S,
    redis_client: Option<RedisClient>,
    translator: Arc<Translator>,
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

        let claims = match (&auth_config, &token) {
            (Some(config), Some(t)) => jwt::validate_token(t, config).ok(),
            _ => None,
        };

        let lang = extract_lang(&req);
        let translator = self.translator.clone();

        let claims = match claims {
            Some(c) => c,
            None => {
                let error_msg = serde_json::to_string(&ApiErrorResponse {
                    success: false,
                    error: translator.t("auth.auth_required", lang),
                    field: None,
                    source: "auth:middleware".to_string(),
                    request_id: crate::observability::CORRELATION_ID
                        .try_with(|id| id.to_string())
                        .ok(),
                })
                .unwrap();
                let error = actix_web::error::ErrorUnauthorized(error_msg);
                return Box::pin(async move { Err(error) });
            }
        };

        let redis_client = self.redis_client.clone();
        let jti = claims.jti.clone();
        let user_id = claims.sub;
        let pseudo = claims.pseudo;

        req.extensions_mut().insert(AuthenticatedUser {
            user_id,
            pseudo: pseudo.clone(),
        });

        let fut = self.service.call(req);

        Box::pin(async move {
            if let Some(mut r) = redis_client {
                if r.exists(&format!("token:blacklist:{jti}"))
                    .await
                    .unwrap_or(false)
                {
                    let error_msg = serde_json::to_string(&ApiErrorResponse {
                        success: false,
                        error: translator.t("auth.token_revoked", lang),
                        field: None,
                        source: "auth:middleware".to_string(),
                        request_id: crate::observability::CORRELATION_ID
                            .try_with(|id| id.to_string())
                            .ok(),
                    })
                    .unwrap();
                    return Err(actix_web::error::ErrorUnauthorized(error_msg));
                }
            }

            let res = fut.await?;
            Ok(res)
        })
    }
}
