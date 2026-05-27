use super::*;

fn test_config() -> RateLimitConfig {
    RateLimitConfig {
        max_requests: 3,
        window_seconds: 3600,
        key_prefix: "test",
    }
}

#[test]
fn in_memory_allows_under_limit() {
    let limiter = InMemoryRateLimiter::default();
    let config = test_config();

    for _ in 0..config.max_requests {
        let result = limiter.check("127.0.0.1", &config);
        assert!(result.allowed, "Request under limit should be allowed");
    }
}

#[test]
fn in_memory_blocks_over_limit() {
    let limiter = InMemoryRateLimiter::default();
    let config = test_config();

    for _ in 0..config.max_requests {
        let result = limiter.check("127.0.0.1", &config);
        assert!(result.allowed);
    }

    let result = limiter.check("127.0.0.1", &config);
    assert!(!result.allowed, "Request over limit should be blocked");
    assert!(
        result.retry_after_secs > 0,
        "Should have retry_after_secs > 0, got {}",
        result.retry_after_secs
    );
}

#[test]
fn in_memory_ip_isolation() {
    let limiter = InMemoryRateLimiter::default();
    let config = test_config();

    for _ in 0..config.max_requests {
        assert!(limiter.check("1.1.1.1", &config).allowed);
    }

    assert!(!limiter.check("1.1.1.1", &config).allowed);

    assert!(limiter.check("2.2.2.2", &config).allowed);
    assert!(limiter.check("2.2.2.2", &config).allowed);
}

#[test]
fn in_memory_key_prefix_isolation() {
    let limiter = InMemoryRateLimiter::default();

    let config_a = RateLimitConfig {
        max_requests: 2,
        window_seconds: 3600,
        key_prefix: "a",
    };
    let config_b = RateLimitConfig {
        max_requests: 2,
        window_seconds: 3600,
        key_prefix: "b",
    };

    assert!(limiter.check("127.0.0.1", &config_a).allowed);
    assert!(limiter.check("127.0.0.1", &config_a).allowed);
    assert!(!limiter.check("127.0.0.1", &config_a).allowed);

    assert!(limiter.check("127.0.0.1", &config_b).allowed);
    assert!(limiter.check("127.0.0.1", &config_b).allowed);
}

#[test]
fn rate_limit_check_result_allowed() {
    let result = RateLimitCheckResult::allowed();
    assert!(result.allowed);
    assert_eq!(result.retry_after_secs, 0);
}

#[test]
fn rate_limit_check_result_blocked() {
    let result = RateLimitCheckResult::blocked(42);
    assert!(!result.allowed);
    assert_eq!(result.retry_after_secs, 42);
}

#[test]
fn rate_limit_check_result_fail_closed() {
    let result = RateLimitCheckResult::fail_closed();
    assert!(!result.allowed);
    assert!(result.retry_after_secs > 0);
}

#[test]
fn rate_limit_configs_from_config() {
    let cfg = crate::config::Config::default();
    let configs = RateLimitConfigs::from_config(&cfg);

    assert_eq!(configs.default.key_prefix, "default");
    assert_eq!(
        configs.default.max_requests,
        cfg.rate_limit_default_max_requests
    );
    assert_eq!(
        configs.default.window_seconds,
        cfg.rate_limit_default_window_seconds
    );

    assert_eq!(configs.contact.key_prefix, "contact");
    assert_eq!(
        configs.contact.max_requests,
        cfg.rate_limit_contact_max_requests
    );
    assert_eq!(configs.login.key_prefix, "login");
    assert_eq!(configs.register.key_prefix, "register");
    assert_eq!(configs.forgot_password.key_prefix, "forgot_password");
    assert_eq!(configs.reset_password.key_prefix, "reset_password");
}

#[actix_web::test]
async fn rate_limiter_check_no_redis_falls_back_to_in_memory() {
    let limiter = RateLimiter::new(None, test_config());

    for _ in 0..test_config().max_requests {
        let result = limiter.check("10.0.0.1").await;
        assert!(result.allowed, "Request under limit should be allowed");
    }

    let result = limiter.check("10.0.0.1").await;
    assert!(!result.allowed, "Request over limit should be blocked");
    assert!(result.retry_after_secs > 0);

    let result = limiter.check("10.0.0.2").await;
    assert!(result.allowed, "Different IP should not be blocked");
}

#[actix_web::test]
async fn rate_limiter_fallback_warned_flag_flips_once() {
    let limiter = RateLimiter::new(None, test_config());

    assert!(!limiter.fallback_warned.load(Ordering::Relaxed));

    limiter.check("10.0.0.1").await;

    assert!(limiter.fallback_warned.load(Ordering::Relaxed));
}

#[actix_web::test]
async fn middleware_allows_requests_under_limit() {
    use actix_web::{web, App};
    let translator = Arc::new(Translator::new());
    let config = RateLimitConfig {
        max_requests: 3,
        window_seconds: 3600,
        key_prefix: "test",
    };
    let limiter = RateLimiterMiddleware::new(None, config, translator);

    let app = actix_web::test::init_service(
        App::new().service(
            web::resource("/test")
                .wrap(limiter)
                .route(web::get().to(|| async { HttpResponse::Ok().body("ok") })),
        ),
    )
    .await;

    for _ in 0..3 {
        let req = actix_web::test::TestRequest::get()
            .uri("/test")
            .peer_addr("127.0.0.1:12345".parse().unwrap())
            .to_request();
        let resp = app.call(req).await.unwrap();
        assert_eq!(resp.status(), 200);
    }
}

#[actix_web::test]
async fn middleware_blocks_request_over_limit() {
    use actix_web::{web, App};
    let translator = Arc::new(Translator::new());
    let config = RateLimitConfig {
        max_requests: 2,
        window_seconds: 3600,
        key_prefix: "test",
    };
    let limiter = RateLimiterMiddleware::new(None, config, translator);

    let app = actix_web::test::init_service(
        App::new().service(
            web::resource("/test")
                .wrap(limiter)
                .route(web::get().to(|| async { HttpResponse::Ok().body("ok") })),
        ),
    )
    .await;

    for _ in 0..2 {
        let req = actix_web::test::TestRequest::get()
            .uri("/test")
            .peer_addr("127.0.0.1:12345".parse().unwrap())
            .to_request();
        let resp = app.call(req).await.unwrap();
        assert_eq!(resp.status(), 200);
    }

    let req = actix_web::test::TestRequest::get()
        .uri("/test")
        .peer_addr("127.0.0.1:12345".parse().unwrap())
        .to_request();
    let result = app.call(req).await;
    match result {
        Ok(_) => panic!("Expected error response, got Ok"),
        Err(e) => {
            let resp = e.error_response();
            assert_eq!(resp.status(), 429);
        }
    }
}

#[actix_web::test]
async fn middleware_returns_retry_after_header_on_429() {
    use actix_web::{web, App};
    let translator = Arc::new(Translator::new());
    let config = RateLimitConfig {
        max_requests: 1,
        window_seconds: 120,
        key_prefix: "test",
    };
    let limiter = RateLimiterMiddleware::new(None, config, translator);

    let app = actix_web::test::init_service(
        App::new().service(
            web::resource("/test")
                .wrap(limiter)
                .route(web::get().to(|| async { HttpResponse::Ok().body("ok") })),
        ),
    )
    .await;

    let req = actix_web::test::TestRequest::get()
        .uri("/test")
        .peer_addr("127.0.0.1:12345".parse().unwrap())
        .to_request();
    app.call(req).await.unwrap();

    let req = actix_web::test::TestRequest::get()
        .uri("/test")
        .peer_addr("127.0.0.1:12345".parse().unwrap())
        .to_request();
    let result = app.call(req).await;
    match result {
        Ok(_) => panic!("Expected error response, got Ok"),
        Err(e) => {
            let resp = e.error_response();
            assert_eq!(resp.status(), 429);
            let retry_after = resp
                .headers()
                .get("Retry-After")
                .expect("429 response must include Retry-After header")
                .to_str()
                .unwrap();
            let retry_secs: u64 = retry_after.parse().unwrap();
            assert!(retry_secs > 0);
        }
    }
}

#[actix_web::test]
async fn middleware_respects_different_peer_ips() {
    use actix_web::{web, App};
    let translator = Arc::new(Translator::new());
    let config = RateLimitConfig {
        max_requests: 2,
        window_seconds: 3600,
        key_prefix: "test",
    };
    let limiter = RateLimiterMiddleware::new(None, config, translator);

    let app = actix_web::test::init_service(
        App::new().service(
            web::resource("/test")
                .wrap(limiter)
                .route(web::get().to(|| async { HttpResponse::Ok().body("ok") })),
        ),
    )
    .await;

    for _ in 0..2 {
        let req = actix_web::test::TestRequest::get()
            .uri("/test")
            .peer_addr("1.1.1.1:11111".parse().unwrap())
            .to_request();
        let resp = app.call(req).await.unwrap();
        assert_eq!(resp.status(), 200);
    }

    let req = actix_web::test::TestRequest::get()
        .uri("/test")
        .peer_addr("1.1.1.1:11111".parse().unwrap())
        .to_request();
    let result = app.call(req).await;
    match result {
        Ok(_) => panic!("Expected 429 for IP 1.1.1.1"),
        Err(e) => {
            assert_eq!(e.error_response().status(), 429);
        }
    }

    let req = actix_web::test::TestRequest::get()
        .uri("/test")
        .peer_addr("2.2.2.2:22222".parse().unwrap())
        .to_request();
    let resp = app.call(req).await.unwrap();
    assert_eq!(resp.status(), 200);
}

#[actix_web::test]
async fn middleware_uses_client_ip_hash_from_extensions() {
    use actix_web::{web, App};
    let translator = Arc::new(Translator::new());
    let config = RateLimitConfig {
        max_requests: 2,
        window_seconds: 3600,
        key_prefix: "test",
    };
    let limiter = RateLimiterMiddleware::new(None, config, translator);

    let app = actix_web::test::init_service(
        App::new().service(
            web::resource("/test")
                .wrap(limiter)
                .route(web::get().to(|| async { HttpResponse::Ok().body("ok") })),
        ),
    )
    .await;

    let client_ip = crate::auth::extractors::ClientIp::from_raw_ip("10.0.0.55", "test-pepper");

    for _ in 0..2 {
        let req = actix_web::test::TestRequest::get()
            .uri("/test")
            .to_request();
        req.extensions_mut().insert(client_ip.clone());
        let resp = app.call(req).await.unwrap();
        assert_eq!(resp.status(), 200);
    }

    let req = actix_web::test::TestRequest::get()
        .uri("/test")
        .to_request();
    req.extensions_mut().insert(client_ip.clone());
    let result = app.call(req).await;
    match result {
        Ok(_) => panic!("Expected 429 for ClientIp hash"),
        Err(e) => {
            assert_eq!(e.error_response().status(), 429);
        }
    }
}
