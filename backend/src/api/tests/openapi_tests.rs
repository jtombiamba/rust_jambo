use crate::api::openapi::ApiDoc;
use utoipa::OpenApi;

#[test]
fn openapi_spec_is_valid() {
    let doc = ApiDoc::openapi();
    let json = doc.to_json().expect("spec must serialize to JSON");
    assert!(json.contains("\"openapi\""), "spec missing openapi field");
    assert!(
        json.contains("\"/api/auth/login\""),
        "login path missing from spec"
    );
    assert!(
        json.contains("\"/api/me/games\""),
        "game history path missing from spec"
    );
}

#[test]
fn openapi_spec_exposes_cookie_auth_scheme() {
    let doc = ApiDoc::openapi();
    let json = doc.to_json().expect("spec must serialize to JSON");
    assert!(
        json.contains("cookie_auth"),
        "cookie auth security scheme missing from spec"
    );
}
