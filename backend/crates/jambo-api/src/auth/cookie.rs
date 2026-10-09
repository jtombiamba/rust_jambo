use actix_web::cookie::{time, Cookie};
use actix_web::HttpResponseBuilder;

pub fn set_auth_cookie(builder: &mut HttpResponseBuilder, token: &str, jwt_expiry_hours: i64) {
    let mut cookie = Cookie::new("Authorization", token.to_string());
    cookie.set_path("/");
    cookie.set_http_only(true);
    cookie.set_secure(false);
    cookie.set_same_site(actix_web::cookie::SameSite::Strict);
    cookie.set_max_age(time::Duration::seconds(jwt_expiry_hours * 3600));
    builder.cookie(cookie);
}

pub fn clear_auth_cookie(builder: &mut HttpResponseBuilder) {
    let mut cookie = Cookie::new("Authorization", "");
    cookie.set_path("/");
    cookie.set_http_only(true);
    cookie.set_secure(false);
    cookie.set_same_site(actix_web::cookie::SameSite::Strict);
    cookie.set_max_age(time::Duration::seconds(1));
    builder.cookie(cookie);
}
