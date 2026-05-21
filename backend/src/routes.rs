use actix_web::{get, web, HttpResponse, Responder};
use prometheus::Encoder;

use crate::api::anonymous::get_anonymous_stats;
use crate::api::game::play_card;
use crate::api::quickie::create_quick_game;
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
        .app_data(state.config.clone())
        .service(health_check)
        .service(metrics)
        .service(
            web::scope("/api")
                .service(get_anonymous_stats)
                .service(create_quick_game)
                .service(play_card)
                .service(
                    web::scope("/auth")
                        .route("/register", web::post().to(crate::api::auth::register))
                        .route("/login", web::post().to(crate::api::auth::login))
                        .route(
                            "/forgot-password",
                            web::post().to(crate::api::auth::forgot_password),
                        )
                        .route(
                            "/reset-password",
                            web::post().to(crate::api::auth::reset_password),
                        )
                        .route("/logout", web::post().to(crate::api::auth::logout))
                        .service(
                            web::resource("/me")
                                .wrap(auth_mw.clone())
                                .route(web::get().to(crate::api::auth::me)),
                        ),
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
                ),
        )
        .service(crate::websocket::scope());
}
