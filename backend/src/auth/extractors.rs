use actix_web::{Error, FromRequest, HttpMessage, HttpRequest};
use serde_json::json;
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct AuthenticatedUser {
    pub user_id: Uuid,
    pub pseudo: String,
}

impl FromRequest for AuthenticatedUser {
    type Error = Error;
    type Future = std::future::Ready<Result<Self, Self::Error>>;

    fn from_request(req: &HttpRequest, _: &mut actix_web::dev::Payload) -> Self::Future {
        let result = req
            .extensions()
            .get::<AuthenticatedUser>()
            .cloned()
            .ok_or_else(|| {
                actix_web::error::ErrorUnauthorized(
                    json!({"success": false, "error": "Authentication required"}).to_string(),
                )
            });
        std::future::ready(result)
    }
}

#[derive(Debug, Clone)]
pub struct ClientIp {
    #[allow(dead_code)]
    pub ip: String,
    pub hash: String,
}

impl ClientIp {
    pub fn from_raw_ip(ip: &str, pepper: &str) -> Self {
        use sha2::{Digest, Sha256};
        let mut hasher = Sha256::new();
        hasher.update(ip.as_bytes());
        hasher.update(pepper.as_bytes());
        let hash = hex::encode(hasher.finalize());
        Self {
            ip: ip.to_string(),
            hash,
        }
    }
}

impl FromRequest for ClientIp {
    type Error = Error;
    type Future = std::future::Ready<Result<Self, Self::Error>>;

    fn from_request(req: &HttpRequest, _: &mut actix_web::dev::Payload) -> Self::Future {
        let result = req.extensions().get::<ClientIp>().cloned().ok_or_else(|| {
            actix_web::error::ErrorInternalServerError("Client IP not found in request extensions")
        });
        std::future::ready(result)
    }
}
