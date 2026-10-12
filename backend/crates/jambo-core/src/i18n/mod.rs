use actix_web::{FromRequest, HttpRequest};

pub use jambo_i18n::{Lang, Translator, TRANSLATOR};

pub mod lang_endpoint;
pub mod middleware;

tokio::task_local! {
    /// Language of the current request, scoped by `I18nMiddleware`.
    /// Mirrors `CORRELATION_ID` so that `ResponseError::error_response` can
    /// translate messages without holding a reference to the request.
    pub static CURRENT_LANG: Lang;
}

pub fn get_lang_from_cookie(req: &HttpRequest) -> Option<Lang> {
    req.cookie("lang").and_then(|c| Lang::parse(c.value()))
}

pub fn extract_lang_from_req(req: &HttpRequest) -> Lang {
    if let Some(lang) = get_lang_from_cookie(req) {
        return lang;
    }

    if let Some(header) = req.headers().get("Accept-Language") {
        if let Ok(value) = header.to_str() {
            return Lang::from_accept_language(value);
        }
    }

    Lang::En
}

pub fn extract_lang(req: &actix_web::dev::ServiceRequest) -> Lang {
    extract_lang_from_req(req.request())
}

pub struct I18n {
    pub lang: Lang,
    translator: std::sync::Arc<Translator>,
}

impl I18n {
    pub fn t(&self, key: &str) -> String {
        self.translator.t(key, self.lang)
    }

    #[allow(dead_code)]
    pub fn t_replace(&self, key: &str, placeholder: &str, replacement: &str) -> String {
        self.translator
            .t_replace(key, self.lang, placeholder, replacement)
    }
}

impl FromRequest for I18n {
    type Error = actix_web::Error;
    type Future = std::future::Ready<Result<Self, Self::Error>>;

    fn from_request(req: &HttpRequest, _payload: &mut actix_web::dev::Payload) -> Self::Future {
        let translator = req
            .app_data::<actix_web::web::Data<std::sync::Arc<Translator>>>()
            .cloned()
            .expect("Translator must be registered in AppState");

        let lang = extract_lang_from_req(req);

        std::future::ready(Ok(I18n {
            lang,
            translator: translator.get_ref().clone(),
        }))
    }
}
