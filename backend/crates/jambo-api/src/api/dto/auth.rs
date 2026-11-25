use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct RegisterRequest {
    pub pseudo: String,
    pub email: String,
    pub password: String,
    pub password_confirm: String,
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct LoginRequest {
    pub email: String,
    pub password: String,
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct ForgotPasswordRequest {
    pub email: String,
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct ResetPasswordRequest {
    pub token: String,
    pub password: String,
    pub password_confirm: String,
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct ChangePasswordRequest {
    pub current_password: String,
    pub new_password: String,
    pub new_password_confirm: String,
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct ChangeEmailRequest {
    pub new_email: String,
    pub current_password: String,
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct ChangeLanguageRequest {
    pub language: String,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct AuthResponse {
    pub success: bool,
    pub message: String,
    pub user: Option<UserInfo>,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct UserInfo {
    pub id: Uuid,
    pub pseudo: String,
    pub email: String,
    pub language: String,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct ForgotPasswordResponse {
    pub success: bool,
    pub message: String,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct ResetPasswordResponse {
    pub success: bool,
    pub message: String,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct LogoutResponse {
    pub success: bool,
    pub message: String,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct ChangePasswordResponse {
    pub success: bool,
    pub message: String,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct ChangeEmailResponse {
    pub success: bool,
    pub message: String,
    pub user: Option<UserInfo>,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct ChangeLanguageResponse {
    pub success: bool,
    pub message: String,
    pub language: String,
}
