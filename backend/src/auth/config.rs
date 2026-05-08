use config::ConfigError;

#[derive(Debug, Clone)]
pub struct AuthConfig {
    pub jwt_secret: String,
    pub jwt_expiry_hours: i64,
    pub ip_hash_pepper: String,
    pub frontend_url: String,
}

impl AuthConfig {
    pub fn from_env() -> Result<Self, ConfigError> {
        let jwt_secret = std::env::var("JWT_SECRET").unwrap_or_else(|_| {
            "super-secret-key-change-me-in-production-please-do-it-now".to_string()
        });
        let jwt_expiry_hours = std::env::var("JWT_EXPIRY_HOURS")
            .unwrap_or_else(|_| "24".to_string())
            .parse()
            .unwrap_or(24);
        let ip_hash_pepper = std::env::var("IP_HASH_PEPPER")
            .unwrap_or_else(|_| "ip-pepper-change-me-1234567890abcdef".to_string());
        let frontend_url =
            std::env::var("FRONTEND_URL").unwrap_or_else(|_| "http://localhost:5173".to_string());

        Ok(Self {
            jwt_secret,
            jwt_expiry_hours,
            ip_hash_pepper,
            frontend_url,
        })
    }
}
