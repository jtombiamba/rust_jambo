use crate::api::contact::send_contact;
use crate::i18n::Translator;
use crate::mailer::{EmailJob, EmailQueue};
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
    email_queue: EmailQueue,
) -> impl actix_web::dev::Service<
    actix_http::Request,
    Response = actix_web::dev::ServiceResponse,
    Error = actix_web::Error,
> {
    test::init_service(
        App::new()
            .app_data(web::Data::new(email_queue))
            .app_data(web::Data::new(Arc::new(Translator::new())))
            .service(web::resource("/contact").route(web::post().to(send_contact))),
    )
    .await
}

#[actix_web::test]
async fn send_contact_success() {
    let (email_queue, mut rx) = EmailQueue::channel();
    let app = make_app(email_queue).await;
    let req = test::TestRequest::post()
        .uri("/contact")
        .set_json(valid_body())
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = test::read_body_json(resp).await;
    assert!(body["message"].is_string());

    let job = rx
        .try_recv()
        .expect("expected a contact form email to be enqueued");
    match job {
        EmailJob::ContactForm {
            name,
            email,
            subject,
            message,
            ..
        } => {
            assert_eq!(name, "Alice");
            assert_eq!(email, "alice@example.com");
            assert_eq!(subject, "Hello");
            assert_eq!(message, "This is a message");
        }
        other => panic!("expected ContactForm job, got {:?}", other),
    }
}

#[actix_web::test]
async fn send_contact_missing_fields_returns_400() {
    let (email_queue, mut rx) = EmailQueue::channel();
    let app = make_app(email_queue).await;
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

    assert!(
        rx.try_recv().is_err(),
        "no email should be enqueued when validation fails"
    );
}
