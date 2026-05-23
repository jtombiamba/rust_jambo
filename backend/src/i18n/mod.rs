use std::collections::HashMap;

use actix_web::{FromRequest, HttpRequest};
use serde::{Deserialize, Serialize};

pub mod lang_endpoint;
pub mod middleware;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Lang {
    #[default]
    En,
    Fr,
}

impl std::str::FromStr for Lang {
    type Err = ();
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "en" => Ok(Lang::En),
            "fr" => Ok(Lang::Fr),
            _ => Err(()),
        }
    }
}

impl Lang {
    pub fn as_str(&self) -> &'static str {
        match self {
            Lang::En => "en",
            Lang::Fr => "fr",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "en" => Some(Lang::En),
            "fr" => Some(Lang::Fr),
            _ => None,
        }
    }

    pub fn from_accept_language(header: &str) -> Self {
        for part in header.split(',') {
            let lang_tag = part.split(';').next().unwrap_or("").trim();
            let primary_lang = lang_tag.split('-').next().unwrap_or(lang_tag);
            if let Some(lang) = Lang::parse(primary_lang) {
                return lang;
            }
        }
        Lang::En
    }

    pub fn all() -> Vec<Lang> {
        vec![Lang::En, Lang::Fr]
    }

    pub fn label(&self) -> &'static str {
        match self {
            Lang::En => "English",
            Lang::Fr => "Fran\u{00e7}ais",
        }
    }
}

pub struct Translator {
    map: HashMap<String, HashMap<String, String>>,
}

impl Translator {
    pub fn new() -> Self {
        let en: HashMap<String, String> =
            serde_json::from_str(include_str!("translations/en.json"))
                .expect("Failed to parse en.json translations");
        let fr: HashMap<String, String> =
            serde_json::from_str(include_str!("translations/fr.json"))
                .expect("Failed to parse fr.json translations");

        let mut map = HashMap::new();
        map.insert("en".to_string(), en);
        map.insert("fr".to_string(), fr);

        Self { map }
    }

    pub fn t(&self, key: &str, lang: Lang) -> String {
        let lang_map = self.map.get(lang.as_str());

        if let Some(map) = lang_map {
            if let Some(value) = map.get(key) {
                return value.clone();
            }
        }

        self.map
            .get("en")
            .and_then(|en_map| en_map.get(key))
            .cloned()
            .unwrap_or_else(|| format!("MISSING: {key}"))
    }

    pub fn t_replace(&self, key: &str, lang: Lang, placeholder: &str, replacement: &str) -> String {
        self.t(key, lang).replace(placeholder, replacement)
    }
}

impl Default for Translator {
    fn default() -> Self {
        Self::new()
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lang_from_str_en() {
        assert_eq!(Lang::parse("en"), Some(Lang::En));
        assert_eq!(Lang::parse("EN"), Some(Lang::En));
        assert_eq!(Lang::parse("En"), Some(Lang::En));
    }

    #[test]
    fn test_lang_from_str_fr() {
        assert_eq!(Lang::parse("fr"), Some(Lang::Fr));
        assert_eq!(Lang::parse("FR"), Some(Lang::Fr));
    }

    #[test]
    fn test_lang_from_str_invalid() {
        assert_eq!(Lang::parse("de"), None);
        assert_eq!(Lang::parse(""), None);
        assert_eq!(Lang::parse("english"), None);
    }

    #[test]
    fn test_lang_as_str() {
        assert_eq!(Lang::En.as_str(), "en");
        assert_eq!(Lang::Fr.as_str(), "fr");
    }

    #[test]
    fn test_lang_default() {
        assert_eq!(Lang::default(), Lang::En);
    }

    #[test]
    fn test_accept_language_fr_simple() {
        assert_eq!(Lang::from_accept_language("fr"), Lang::Fr);
    }

    #[test]
    fn test_accept_language_fr_with_quality() {
        assert_eq!(Lang::from_accept_language("fr;q=0.9,en;q=0.8"), Lang::Fr);
    }

    #[test]
    fn test_accept_language_en_first() {
        assert_eq!(
            Lang::from_accept_language("en-US,en;q=0.9,fr;q=0.8"),
            Lang::En
        );
    }

    #[test]
    fn test_accept_language_fallback_en() {
        assert_eq!(Lang::from_accept_language("de"), Lang::En);
        assert_eq!(Lang::from_accept_language(""), Lang::En);
    }

    #[test]
    fn test_accept_language_multiple() {
        assert_eq!(
            Lang::from_accept_language("de-DE,fr-FR;q=0.9,en;q=0.8"),
            Lang::Fr
        );
    }

    #[test]
    fn test_translator_en() {
        let translator = Translator::new();
        assert_eq!(
            translator.t("auth.pseudo_required", Lang::En),
            "Pseudo is required"
        );
        assert_eq!(
            translator.t("auth.account_created", Lang::En),
            "Account created successfully"
        );
        assert_eq!(
            translator.t("server.internal_error", Lang::En),
            "Internal server error"
        );
    }

    #[test]
    fn test_translator_fr() {
        let translator = Translator::new();
        assert_eq!(
            translator.t("auth.pseudo_required", Lang::Fr),
            "Le pseudo est requis"
        );
        assert_eq!(
            translator.t("auth.account_created", Lang::Fr),
            "Compte cr\u{00e9}\u{00e9} avec succ\u{00e8}s"
        );
        assert_eq!(
            translator.t("server.internal_error", Lang::Fr),
            "Erreur interne du serveur"
        );
    }

    #[test]
    fn test_translator_missing_key() {
        let translator = Translator::new();
        let result = translator.t("nonexistent.key", Lang::En);
        assert!(result.starts_with("MISSING:"));
    }

    #[test]
    fn test_translator_t_replace() {
        let translator = Translator::new();
        let result =
            translator.t_replace("password.forgot", Lang::En, "{email}", "test@example.com");
        assert!(result.contains("test@example.com"));
        assert!(!result.contains("{email}"));
    }

    #[test]
    fn test_translator_all_keys_exist_in_fr() {
        let translator = Translator::new();
        for key in translator.map.get("en").unwrap().keys() {
            let fr_value = translator.map.get("fr").unwrap().get(key);
            assert!(
                fr_value.is_some(),
                "Key '{key}' missing in French translations"
            );
        }
    }

    #[test]
    fn test_lang_all() {
        let all = Lang::all();
        assert_eq!(all.len(), 2);
        assert!(all.contains(&Lang::En));
        assert!(all.contains(&Lang::Fr));
    }

    #[test]
    fn test_lang_label() {
        assert_eq!(Lang::En.label(), "English");
        assert_eq!(Lang::Fr.label(), "Fran\u{00e7}ais");
    }
}
