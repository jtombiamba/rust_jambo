use actix_web::{get, web, HttpResponse, Responder};
use prometheus::Encoder;

use crate::api::anonymous::get_anonymous_stats;
use crate::api::fallback;
use crate::api::game::play_card;
use crate::api::middleware::rate_limiter::RateLimiterMiddleware;
use crate::api::quickie::create_quick_game;
use crate::api::room;
use crate::bootstrap::AppState;

#[get("/health")]
pub async fn health_check() -> impl Responder {
    HttpResponse::Ok().body("OK")
}

#[get("/metrics")]
pub async fn metrics() -> HttpResponse {
    let encoder = prometheus::TextEncoder::new();
    let metric_families = prometheus::gather();
    let mut buffer = vec![];
    encoder.encode(&metric_families, &mut buffer).unwrap();
    HttpResponse::Ok()
        .content_type("text/plain; version=0.0.4")
        .body(buffer)
}

pub fn configure(cfg: &mut web::ServiceConfig, state: &AppState) {
    let auth_mw = state.auth_middleware.clone();
    let redis = state.redis.get_ref().clone();
    let translator = state.translator.get_ref().clone();
    let configs = &state.rate_limit_configs;

    let contact_limiter =
        RateLimiterMiddleware::new(redis.clone(), configs.contact.clone(), translator.clone());
    let register_limiter =
        RateLimiterMiddleware::new(redis.clone(), configs.register.clone(), translator.clone());
    let login_limiter =
        RateLimiterMiddleware::new(redis.clone(), configs.login.clone(), translator.clone());
    let forgot_pw_limiter = RateLimiterMiddleware::new(
        redis.clone(),
        configs.forgot_password.clone(),
        translator.clone(),
    );
    let reset_pw_limiter = RateLimiterMiddleware::new(
        redis.clone(),
        configs.reset_password.clone(),
        translator.clone(),
    );
    // let default_limiter =
    //     RateLimiterMiddleware::new(redis.clone(), configs.default.clone(), translator.clone());

    cfg.app_data(state.db.clone())
        .app_data(state.redis.clone())
        .app_data(state.rabbitmq.clone())
        .app_data(state.ws_manager.clone())
        .app_data(state.orchestrator.clone())
        .app_data(state.auth_config.clone())
        .app_data(state.auth_service.clone())
        .app_data(state.dashboard_service.clone())
        .app_data(state.user_cache.clone())
        .app_data(state.mailer.clone())
        .app_data(state.payment_service.clone())
        .app_data(state.room_service.clone())
        .app_data(state.config.clone())
        .app_data(state.translator.clone())
        .service(health_check)
        .service(metrics)
        .service(
            web::scope("/api")
                .service(get_anonymous_stats)
                .service(crate::api::config::client_config)
                .service(create_quick_game)
                .service(play_card)
                .route(
                    "/lang",
                    web::post().to(crate::i18n::lang_endpoint::set_language),
                )
                .route(
                    "/lang",
                    web::get().to(crate::i18n::lang_endpoint::get_current_lang),
                )
                .route(
                    "/languages",
                    web::get().to(crate::i18n::lang_endpoint::get_languages),
                )
                .service(
                    web::scope("/auth")
                        .service(
                            web::resource("/register")
                                .wrap(register_limiter.clone())
                                .route(web::post().to(crate::api::auth::register)),
                        )
                        .service(
                            web::resource("/login")
                                .wrap(login_limiter.clone())
                                .route(web::post().to(crate::api::auth::login)),
                        )
                        .service(
                            web::resource("/forgot-password")
                                .wrap(forgot_pw_limiter.clone())
                                .route(web::post().to(crate::api::auth::forgot_password)),
                        )
                        .service(
                            web::resource("/reset-password")
                                .wrap(reset_pw_limiter.clone())
                                .route(web::post().to(crate::api::auth::reset_password)),
                        )
                        .route("/logout", web::post().to(crate::api::auth::logout))
                        .service(
                            web::resource("/me")
                                .wrap(auth_mw.clone())
                                .route(web::get().to(crate::api::auth::me)),
                        ),
                )
                .service(
                    web::resource("/contact")
                        .wrap(contact_limiter.clone())
                        .route(web::post().to(crate::api::contact::send_contact)),
                )
                .service(
                    web::scope("/me")
                        .wrap(auth_mw.clone())
                        .route(
                            "/profile",
                            web::get().to(crate::api::dashboard::get_profile),
                        )
                        .route("/games", web::get().to(crate::api::dashboard::list_games))
                        .route("/games", web::post().to(crate::api::dashboard::create_game))
                        .route(
                            "/games/{game_id}",
                            web::get().to(crate::api::dashboard::get_game),
                        )
                        .route(
                            "/active-game",
                            web::get().to(crate::api::dashboard::get_active_game),
                        )
                        .route(
                            "/invitations",
                            web::get().to(crate::api::dashboard::get_invitations),
                        )
                        .route(
                            "/leaderboard",
                            web::get().to(crate::api::leaderboard::get_leaderboard),
                        )
                        .route(
                            "/unfreeze",
                            web::post().to(crate::api::unfreeze::create_unfreeze_order),
                        )
                        .route(
                            "/unfreeze/capture",
                            web::post().to(crate::api::unfreeze::capture_unfreeze_order),
                        )
                        .route(
                            "/topup",
                            web::post().to(crate::api::topup::create_topup_order),
                        )
                        .route(
                            "/topup/capture",
                            web::post().to(crate::api::topup::capture_topup_order),
                        )
                        .route("/rooms", web::post().to(room::create_room))
                        .route("/rooms", web::get().to(room::list_rooms))
                        .route("/rooms/join", web::post().to(room::join_room))
                        .route("/rooms/{room_id}", web::get().to(room::get_room))
                        .route(
                            "/rooms/{room_id}/invite",
                            web::post().to(room::invite_to_room),
                        )
                        .route("/rooms/{room_id}/leave", web::post().to(room::leave_room))
                        .route("/rooms/{room_id}/runs", web::post().to(room::create_run))
                        .route("/rooms/{room_id}/runs", web::get().to(room::list_runs))
                        .route(
                            "/rooms/{room_id}/runs/active",
                            web::get().to(room::get_active_run),
                        )
                        .route("/runs/{run_id}/join", web::post().to(room::join_run))
                        .route("/runs/{run_id}/leave", web::post().to(room::leave_run))
                        .route(
                            "/runs/{run_id}/next-game",
                            web::post().to(room::start_next_game),
                        )
                        .route(
                            "/runs/{run_id}/current-game",
                            web::get().to(room::get_current_game),
                        ),
                )
                .service(
                    web::scope("/games")
                        .wrap(auth_mw.clone())
                        .route(
                            "/{game_id}/invites",
                            web::post().to(crate::api::dashboard::send_invites),
                        )
                        .route(
                            "/{game_id}/respond",
                            web::post().to(crate::api::dashboard::respond_to_invite),
                        )
                        .route(
                            "/{game_id}/start",
                            web::post().to(crate::api::dashboard::start_game),
                        )
                        .route(
                            "/{game_id}/play",
                            web::post().to(crate::api::dashboard::play_game),
                        )
                        .route(
                            "/{game_id}/me",
                            web::get().to(crate::api::dashboard::game_state),
                        ),
                )
                .service(web::scope("/users").wrap(auth_mw.clone()).route(
                    "/search",
                    web::get().to(crate::api::dashboard::search_users),
                ))
                .service(
                    web::scope("/paypal")
                        .route(
                            "/return",
                            web::get().to(crate::api::unfreeze::paypal_return),
                        )
                        .route(
                            "/cancel",
                            web::get().to(crate::api::unfreeze::paypal_cancel),
                        )
                        .route(
                            "/topup/return",
                            web::get().to(crate::api::topup::paypal_return_topup),
                        )
                        .route(
                            "/topup/cancel",
                            web::get().to(crate::api::topup::paypal_cancel_topup),
                        ),
                )
                .configure(|cfg| {
                    if state.config.benchmark_mode {
                        cfg.service(
                            web::scope("/benchmark")
                                .service(
                                    web::resource("/create-multiplayer-game")
                                        .route(
                                            web::post()
                                                .to(crate::api::benchmark::create_benchmark_game),
                                        )
                                        .route(web::route().to(fallback::method_not_allowed)),
                                )
                                .service(
                                    web::resource("/cleanup")
                                        .route(
                                            web::post()
                                                .to(crate::api::benchmark::cleanup_benchmark_data),
                                        )
                                        .route(web::route().to(fallback::method_not_allowed)),
                                )
                                .default_service(web::route().to(fallback::not_found)),
                        );
                    }
                })
                .default_service(web::route().to(fallback::not_found)),
            // .wrap(default_limiter),
        );

    cfg.service(crate::websocket::scope());
    cfg.default_service(web::route().to(fallback::not_found));
}
