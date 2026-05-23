use actix_web::{web, HttpMessage, HttpResponse};
use serde::{Deserialize, Serialize};

use super::{Lang, Translator};
use crate::auth::extractors::AuthenticatedUser;
use crate::database::repositories::UserRepository;

#[derive(Debug, Serialize)]
pub struct LanguagesResponse {
    pub current: String,
    pub languages: Vec<LanguageInfo>,
}

#[derive(Debug, Serialize)]
pub struct LanguageInfo {
    pub code: String,
    pub label: String,
}

pub async fn get_languages(req: actix_web::HttpRequest) -> HttpResponse {
    let lang = super::extract_lang_from_req(&req);
    let languages: Vec<LanguageInfo> = Lang::all()
        .into_iter()
        .map(|l| LanguageInfo {
            code: l.as_str().to_string(),
            label: l.label().to_string(),
        })
        .collect();

    HttpResponse::Ok().json(LanguagesResponse {
        current: lang.as_str().to_string(),
        languages,
    })
}

#[derive(Debug, Deserialize)]
pub struct SetLanguageRequest {
    pub lang: String,
}

#[derive(Debug, Serialize)]
pub struct SetLanguageResponse {
    pub success: bool,
    pub message: String,
    pub lang: String,
}

pub async fn set_language(
    req: actix_web::HttpRequest,
    body: web::Json<SetLanguageRequest>,
    translator: web::Data<std::sync::Arc<Translator>>,
    user_repo: web::Data<std::sync::Arc<UserRepository>>,
) -> HttpResponse {
    let requested_lang = match Lang::parse(&body.lang) {
        Some(l) => l,
        None => {
            let current = super::extract_lang_from_req(&req);
            return HttpResponse::BadRequest().json(serde_json::json!({
                "success": false,
                "error": translator.t("language.invalid_lang", current)
            }));
        }
    };

    let auth_user_id = req
        .extensions()
        .get::<AuthenticatedUser>()
        .map(|u| u.user_id);

    if let Some(user_id) = auth_user_id {
        let _ = user_repo
            .update_language(user_id, requested_lang.as_str())
            .await;
    }

    let mut resp = HttpResponse::Ok();
    let mut lang_cookie =
        actix_web::cookie::Cookie::new("lang", requested_lang.as_str().to_string());
    lang_cookie.set_path("/");
    lang_cookie.set_http_only(false);
    lang_cookie.set_secure(false);
    lang_cookie.set_same_site(actix_web::cookie::SameSite::Strict);
    lang_cookie.set_max_age(actix_web::cookie::time::Duration::days(365));
    resp.cookie(lang_cookie);

    resp.json(SetLanguageResponse {
        success: true,
        message: translator.t("language.set_success", requested_lang),
        lang: requested_lang.as_str().to_string(),
    })
}

pub async fn get_current_lang(req: actix_web::HttpRequest) -> HttpResponse {
    let lang = super::extract_lang_from_req(&req);
    HttpResponse::Ok().json(serde_json::json!({
        "lang": lang.as_str()
    }))
}
