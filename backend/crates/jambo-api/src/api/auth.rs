use std::sync::Arc;

use actix_web::{web, HttpMessage, HttpRequest, HttpResponse, ResponseError};

use crate::api::dto::auth::{
    AuthResponse, ChangeEmailRequest, ChangeEmailResponse, ChangeLanguageRequest,
    ChangeLanguageResponse, ChangePasswordRequest, ChangePasswordResponse, ForgotPasswordRequest,
    ForgotPasswordResponse, LoginRequest, LogoutResponse, RegisterRequest, ResetPasswordRequest,
    ResetPasswordResponse, UserInfo,
};
use crate::api::dto::responses::ApiErrorResponse;
use crate::api::services::auth_service::AuthServiceTrait;
use crate::auth::config::AuthConfig;
use crate::auth::cookie;
use crate::auth::extractors::{AuthenticatedUser, ClientIp};
use crate::auth::jwt;
use jambo_core::i18n::I18n;
use jambo_domain::messaging::RedisClient;

#[utoipa::path(
    post,
    path = "/api/auth/register",
    tag = "auth",
    request_body = RegisterRequest,
    responses(
        (status = 201, description = "Registration successful", body = AuthResponse),
        (status = 400, description = "Validation error", body = ApiErrorResponse),
        (status = 409, description = "Email already in use", body = ApiErrorResponse),
        (status = 429, description = "Rate limited", body = ApiErrorResponse),
    )
)]
pub async fn register(
    req: HttpRequest,
    body: web::Json<RegisterRequest>,
    service: web::Data<Arc<dyn AuthServiceTrait>>,
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

#[utoipa::path(
    post,
    path = "/api/auth/login",
    tag = "auth",
    request_body = LoginRequest,
    responses(
        (status = 200, description = "Login successful", body = AuthResponse),
        (status = 401, description = "Invalid credentials", body = ApiErrorResponse),
        (status = 429, description = "Rate limited", body = ApiErrorResponse),
    )
)]
pub async fn login(
    req: HttpRequest,
    body: web::Json<LoginRequest>,
    service: web::Data<Arc<dyn AuthServiceTrait>>,
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

#[utoipa::path(
    post,
    path = "/api/auth/forgot-password",
    tag = "auth",
    request_body = ForgotPasswordRequest,
    responses(
        (status = 200, description = "Reset email sent", body = ForgotPasswordResponse),
        (status = 429, description = "Rate limited", body = ApiErrorResponse),
    )
)]
pub async fn forgot_password(
    body: web::Json<ForgotPasswordRequest>,
    service: web::Data<Arc<dyn AuthServiceTrait>>,
    i18n: I18n,
) -> HttpResponse {
    HttpResponse::Ok().json(service.forgot_password(body.into_inner(), i18n.lang).await)
}

#[utoipa::path(
    post,
    path = "/api/auth/reset-password",
    tag = "auth",
    request_body = ResetPasswordRequest,
    responses(
        (status = 200, description = "Password reset", body = ResetPasswordResponse),
        (status = 400, description = "Invalid token or validation error", body = ApiErrorResponse),
        (status = 429, description = "Rate limited", body = ApiErrorResponse),
    )
)]
pub async fn reset_password(
    body: web::Json<ResetPasswordRequest>,
    service: web::Data<Arc<dyn AuthServiceTrait>>,
    i18n: I18n,
) -> HttpResponse {
    match service.reset_password(body.into_inner(), i18n.lang).await {
        Ok(response) => HttpResponse::Ok().json(response),
        Err(e) => e.error_response(),
    }
}

#[utoipa::path(
    post,
    path = "/api/auth/logout",
    tag = "auth",
    responses((status = 200, description = "Logged out", body = LogoutResponse))
)]
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
    resp.json(LogoutResponse {
        success: true,
        message: i18n.t("auth.logged_out"),
    })
}

#[utoipa::path(
    get,
    path = "/api/auth/me",
    tag = "auth",
    security(("cookie_auth" = [])),
    responses(
        (status = 200, description = "Authenticated user info", body = UserInfo),
        (status = 401, description = "Authentication required", body = ApiErrorResponse),
    )
)]
pub async fn me(
    auth_user: AuthenticatedUser,
    service: web::Data<Arc<dyn AuthServiceTrait>>,
) -> HttpResponse {
    match service.me(auth_user.user_id).await {
        Ok(user_info) => HttpResponse::Ok().json(user_info),
        Err(e) => e.error_response(),
    }
}

#[utoipa::path(
    post,
    path = "/api/auth/change-password",
    tag = "auth",
    security(("cookie_auth" = [])),
    request_body = ChangePasswordRequest,
    responses(
        (status = 200, description = "Password changed", body = ChangePasswordResponse),
        (status = 400, description = "Validation error", body = ApiErrorResponse),
        (status = 401, description = "Invalid current password", body = ApiErrorResponse),
    )
)]
pub async fn change_password(
    auth_user: AuthenticatedUser,
    body: web::Json<ChangePasswordRequest>,
    service: web::Data<Arc<dyn AuthServiceTrait>>,
    i18n: I18n,
) -> HttpResponse {
    match service
        .change_password(auth_user.user_id, body.into_inner(), i18n.lang)
        .await
    {
        Ok(response) => HttpResponse::Ok().json(response),
        Err(e) => e.error_response(),
    }
}

#[utoipa::path(
    post,
    path = "/api/auth/change-email",
    tag = "auth",
    security(("cookie_auth" = [])),
    request_body = ChangeEmailRequest,
    responses(
        (status = 200, description = "Email changed", body = ChangeEmailResponse),
        (status = 400, description = "Validation error", body = ApiErrorResponse),
        (status = 401, description = "Invalid current password", body = ApiErrorResponse),
        (status = 409, description = "Email already in use", body = ApiErrorResponse),
    )
)]
pub async fn change_email(
    auth_user: AuthenticatedUser,
    body: web::Json<ChangeEmailRequest>,
    service: web::Data<Arc<dyn AuthServiceTrait>>,
    i18n: I18n,
) -> HttpResponse {
    match service
        .change_email(auth_user.user_id, body.into_inner(), i18n.lang)
        .await
    {
        Ok(response) => HttpResponse::Ok().json(response),
        Err(e) => e.error_response(),
    }
}

#[utoipa::path(
    post,
    path = "/api/auth/language",
    tag = "auth",
    security(("cookie_auth" = [])),
    request_body = ChangeLanguageRequest,
    responses(
        (status = 200, description = "Language persisted", body = ChangeLanguageResponse),
        (status = 400, description = "Invalid language", body = ApiErrorResponse),
        (status = 401, description = "Authentication required", body = ApiErrorResponse),
    )
)]
pub async fn set_language(
    auth_user: AuthenticatedUser,
    body: web::Json<ChangeLanguageRequest>,
    service: web::Data<Arc<dyn AuthServiceTrait>>,
    i18n: I18n,
) -> HttpResponse {
    let result = service
        .set_language(auth_user.user_id, body.into_inner(), i18n.lang)
        .await;

    match result {
        Ok(response) => {
            let mut resp = HttpResponse::Ok();
            let mut lang_cookie = actix_web::cookie::Cookie::new("lang", response.language.clone());
            lang_cookie.set_path("/");
            lang_cookie.set_http_only(false);
            lang_cookie.set_secure(false);
            lang_cookie.set_same_site(actix_web::cookie::SameSite::Strict);
            lang_cookie.set_max_age(actix_web::cookie::time::Duration::days(365));
            resp.cookie(lang_cookie);
            resp.json(response)
        }
        Err(e) => e.error_response(),
    }
}
