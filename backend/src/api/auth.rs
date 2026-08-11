use std::sync::Arc;

use actix_web::{web, HttpMessage, HttpRequest, HttpResponse, ResponseError};

use crate::api::dto::auth::{
    ForgotPasswordRequest, LoginRequest, RegisterRequest, ResetPasswordRequest,
};
use crate::api::services::auth_service::AuthService;
use crate::auth::config::AuthConfig;
use crate::auth::cookie;
use crate::auth::extractors::{AuthenticatedUser, ClientIp};
use crate::auth::jwt;
use crate::i18n::I18n;
use crate::messaging::RedisClient;

pub type AuthServiceType = AuthService<super::super::database::repositories::UserRepository>;

pub async fn register(
    req: HttpRequest,
    body: web::Json<RegisterRequest>,
    service: web::Data<Arc<AuthServiceType>>,
    i18n: I18n,
) -> HttpResponse {
    let client_ip = req.extensions().get::<ClientIp>().cloned();
    let ip_hash = client_ip.map(|c| c.hash);
    let config = req.app_data::<web::Data<AuthConfig>>().cloned();
    let jwt_expiry_hours = match config {
        Some(c) => c.jwt_expiry_hours,
        None => 24,
    };
    match service
        .register(body.into_inner(), ip_hash, i18n.lang)
        .await
    {
        Ok(result) => {
            let mut resp = HttpResponse::Created();
            cookie::set_auth_cookie(&mut resp, &result.token, jwt_expiry_hours);
            resp.json(result.response)
        }
        Err(e) => e.error_response(),
    }
}

pub async fn login(
    req: HttpRequest,
    body: web::Json<LoginRequest>,
    service: web::Data<Arc<AuthServiceType>>,
    i18n: I18n,
) -> HttpResponse {
    let client_ip = req.extensions().get::<ClientIp>().cloned();
    let ip_hash = client_ip.map(|c| c.hash);
    let config = req.app_data::<web::Data<AuthConfig>>().cloned();
    let jwt_expiry_hours = match config {
        Some(c) => c.jwt_expiry_hours,
        None => 24,
    };
    match service.login(body.into_inner(), ip_hash, i18n.lang).await {
        Ok(result) => {
            let mut resp = HttpResponse::Ok();
            cookie::set_auth_cookie(&mut resp, &result.token, jwt_expiry_hours);
            resp.json(result.response)
        }
        Err(e) => e.error_response(),
    }
}

pub async fn forgot_password(
    body: web::Json<ForgotPasswordRequest>,
    service: web::Data<Arc<AuthServiceType>>,
    i18n: I18n,
) -> HttpResponse {
    HttpResponse::Ok().json(service.forgot_password(body.into_inner(), i18n.lang).await)
}

pub async fn reset_password(
    body: web::Json<ResetPasswordRequest>,
    service: web::Data<Arc<AuthServiceType>>,
    i18n: I18n,
) -> HttpResponse {
    match service.reset_password(body.into_inner(), i18n.lang).await {
        Ok(response) => HttpResponse::Ok().json(response),
        Err(e) => e.error_response(),
    }
}

pub async fn logout(
    req: HttpRequest,
    auth_config: web::Data<AuthConfig>,
    redis_client: web::Data<Option<RedisClient>>,
    i18n: I18n,
) -> HttpResponse {
    if let Some(token) = req.cookie("Authorization").map(|c| c.value().to_string()) {
        if let Ok(claims) = jwt::validate_token(&token, &auth_config) {
            if let Some(mut redis) = redis_client.get_ref().clone() {
                let remaining_ttl = claims.exp as i64 - chrono::Utc::now().timestamp();
                if remaining_ttl > 0 {
                    let _ = redis
                        .set_ex(
                            &format!("token:blacklist:{}", claims.jti),
                            "1",
                            remaining_ttl as u64,
                        )
                        .await;
                }
            }
        }
    }

    let mut resp = HttpResponse::Ok();
    cookie::clear_auth_cookie(&mut resp);
    resp.json(serde_json::json!({
        "success": true,
        "message": i18n.t("auth.logged_out"),
        "user": null
    }))
}

pub async fn me(
    auth_user: AuthenticatedUser,
    service: web::Data<Arc<AuthServiceType>>,
    i18n: I18n,
) -> HttpResponse {
    match service.me(auth_user.user_id, i18n.lang).await {
        Ok(user_info) => HttpResponse::Ok().json(user_info),
        Err(e) => e.error_response(),
    }
}
