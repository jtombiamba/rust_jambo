use crate::api::contact::send_contact;
use crate::i18n::Translator;
use crate::mailer::Mailer;
use crate::test_helpers::MockMailer;
use actix_web::{test, web, App};
use std::sync::Arc;

fn valid_body() -> serde_json::Value {
    serde_json::json!({
        "name": "Alice",
        "email": "alice@example.com",
        "subject": "Hello",
        "message": "This is a message"
    })
}

async fn make_app(
    mailer: Arc<dyn Mailer>,
) -> impl actix_web::dev::Service<
    actix_http::Request,
    Response = actix_web::dev::ServiceResponse,
    Error = actix_web::Error,
> {
    test::init_service(
        App::new()
            .app_data(web::Data::new(mailer))
            .app_data(web::Data::new(Arc::new(Translator::new())))
            .service(web::resource("/contact").route(web::post().to(send_contact))),
    )
    .await
}

#[actix_web::test]
async fn send_contact_success() {
    let app = make_app(Arc::new(MockMailer::ok())).await;
    let req = test::TestRequest::post()
        .uri("/contact")
        .set_json(valid_body())
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = test::read_body_json(resp).await;
    assert!(body["message"].is_string());
}

#[actix_web::test]
async fn send_contact_missing_fields_returns_400() {
    let app = make_app(Arc::new(MockMailer::ok())).await;
    let req = test::TestRequest::post()
        .uri("/contact")
        .set_json(serde_json::json!({
            "name": "",
            "email": "alice@example.com",
            "subject": "Hello",
            "message": "This is a message"
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 400);
    let body: serde_json::Value = test::read_body_json(resp).await;
    assert_eq!(body["success"], false);
}

#[actix_web::test]
async fn send_contact_mailer_error_returns_500() {
    let app = make_app(Arc::new(MockMailer::with_result(Err("smtp down".into())))).await;
    let req = test::TestRequest::post()
        .uri("/contact")
        .set_json(valid_body())
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 500);
    let body: serde_json::Value = test::read_body_json(resp).await;
    assert_eq!(body["success"], false);
    assert_eq!(body["source"], "contact:email");
}
