use crate::api::dto::auth::{
    AuthResponse, ForgotPasswordRequest, LoginRequest, RegisterRequest, ResetPasswordRequest,
    UserInfo,
};
use uuid::Uuid;

#[test]
fn register_request_deserializes() {
    let req: RegisterRequest = serde_json::from_str(
        r#"{"pseudo":"alice","email":"a@b.co","password":"password1","password_confirm":"password1"}"#,
    )
    .unwrap();
    assert_eq!(req.pseudo, "alice");
    assert_eq!(req.email, "a@b.co");
}

#[test]
fn login_request_deserializes() {
    let req: LoginRequest =
        serde_json::from_str(r#"{"email":"a@b.co","password":"password1"}"#).unwrap();
    assert_eq!(req.email, "a@b.co");
    assert_eq!(req.password, "password1");
}

#[test]
fn forgot_password_request_deserializes() {
    let req: ForgotPasswordRequest = serde_json::from_str(r#"{"email":"a@b.co"}"#).unwrap();
    assert_eq!(req.email, "a@b.co");
}

#[test]
fn reset_password_request_deserializes() {
    let req: ResetPasswordRequest = serde_json::from_str(
        r#"{"token":"t","password":"password1","password_confirm":"password1"}"#,
    )
    .unwrap();
    assert_eq!(req.token, "t");
}

#[test]
fn auth_response_serializes_optional_user() {
    let with_user = AuthResponse {
        success: true,
        message: "ok".into(),
        user: Some(UserInfo {
            id: Uuid::now_v7(),
            pseudo: "alice".into(),
            email: "a@b.co".into(),
            language: "en".into(),
        }),
    };
    let json = serde_json::to_value(&with_user).unwrap();
    assert_eq!(json["success"], true);
    assert!(json["user"].is_object());

    let without_user = AuthResponse {
        success: false,
        message: "err".into(),
        user: None,
    };
    let json = serde_json::to_value(&without_user).unwrap();
    assert!(json["user"].is_null());
}
