use actix_web::{web, HttpResponse};
use std::sync::Arc;
use utoipa::OpenApi;

use crate::bootstrap::AppState;
use jambo_api::api::anonymous::get_anonymous_stats;
use jambo_api::api::fallback;
use jambo_api::api::game::{
    advance_bot, claim_special, decline_special, evaluate_round, play_card,
};
use jambo_api::api::middleware::rate_limiter::RateLimiterMiddleware;
use jambo_api::api::quickie::create_quick_game;
use jambo_api::api::room;
use jambo_api::api::system::{health_check, metrics};
use jambo_domain::game::service::{
    BenchmarkService, GameLifecycleService, GamePlayService, InviteService,
};

pub fn configure(cfg: &mut web::ServiceConfig, state: &AppState) {
    let auth_mw = state.auth_middleware.clone();
    let redis = state.redis.get_ref().clone();
    let translator = state.translator.get_ref().clone();
    let configs = &state.rate_limit_configs;
    let benchmark_mode = state.config.benchmark_mode;

    let contact_limiter =
        RateLimiterMiddleware::new(redis.clone(), configs.contact.clone(), translator.clone())
            .with_benchmark_mode(benchmark_mode);
    let register_limiter =
        RateLimiterMiddleware::new(redis.clone(), configs.register.clone(), translator.clone())
            .with_benchmark_mode(benchmark_mode);
    let login_limiter =
        RateLimiterMiddleware::new(redis.clone(), configs.login.clone(), translator.clone())
            .with_benchmark_mode(benchmark_mode);
    let forgot_pw_limiter = RateLimiterMiddleware::new(
        redis.clone(),
        configs.forgot_password.clone(),
        translator.clone(),
    )
    .with_benchmark_mode(benchmark_mode);
    let reset_pw_limiter = RateLimiterMiddleware::new(
        redis.clone(),
        configs.reset_password.clone(),
        translator.clone(),
    )
    .with_benchmark_mode(benchmark_mode);
    // let default_limiter =
    //     RateLimiterMiddleware::new(redis.clone(), configs.default.clone(), translator.clone());

    let orchestrator = state.orchestrator.get_ref().clone();

    cfg.app_data(state.db.clone())
        .app_data(state.redis.clone())
        .app_data(state.rabbitmq.clone())
        .app_data(state.ws_manager.clone())
        .app_data(state.orchestrator.clone())
        .app_data(web::Data::new(
            orchestrator.clone() as Arc<dyn GamePlayService>
        ))
        .app_data(web::Data::new(
            orchestrator.clone() as Arc<dyn InviteService>
        ))
        .app_data(web::Data::new(
            orchestrator.clone() as Arc<dyn GameLifecycleService>
        ))
        .app_data(web::Data::new(orchestrator as Arc<dyn BenchmarkService>))
        .app_data(state.auth_config.clone())
        .app_data(state.auth_service.clone())
        .app_data(state.dashboard_service.clone())
        .app_data(state.user_cache.clone())
        .app_data(state.mailer.clone())
        .app_data(state.email_queue.clone())
        .app_data(state.payment_service.clone())
        .app_data(state.room_service.clone())
        .app_data(state.cashout_service.clone())
        .app_data(state.config.clone())
        .app_data(state.translator.clone())
        .service(health_check)
        .service(metrics)
        .service(
            web::scope("/api")
                .service(get_anonymous_stats)
                .service(jambo_api::api::config::client_config)
                .service(create_quick_game)
                .service(play_card)
                .service(web::resource("/game/{id}/advance-bot").to(advance_bot))
                .service(web::resource("/game/{id}/evaluate-round").to(evaluate_round))
                .service(web::resource("/game/{id}/claim-special").to(claim_special))
                .service(web::resource("/game/{id}/decline-special").to(decline_special))
                .route(
                    "/lang",
                    web::post().to(jambo_core::i18n::lang_endpoint::set_language),
                )
                .route(
                    "/lang",
                    web::get().to(jambo_core::i18n::lang_endpoint::get_current_lang),
                )
                .route(
                    "/languages",
                    web::get().to(jambo_core::i18n::lang_endpoint::get_languages),
                )
                .service(
                    web::scope("/auth")
                        .service(
                            web::resource("/register")
                                .wrap(register_limiter.clone())
                                .route(web::post().to(jambo_api::api::auth::register)),
                        )
                        .service(
                            web::resource("/login")
                                .wrap(login_limiter.clone())
                                .route(web::post().to(jambo_api::api::auth::login)),
                        )
                        .service(
                            web::resource("/forgot-password")
                                .wrap(forgot_pw_limiter.clone())
                                .route(web::post().to(jambo_api::api::auth::forgot_password)),
                        )
                        .service(
                            web::resource("/reset-password")
                                .wrap(reset_pw_limiter.clone())
                                .route(web::post().to(jambo_api::api::auth::reset_password)),
                        )
                        .route("/logout", web::post().to(jambo_api::api::auth::logout))
                        .service(
                            web::resource("/me")
                                .wrap(auth_mw.clone())
                                .route(web::get().to(jambo_api::api::auth::me)),
                        ),
                )
                .service(
                    web::resource("/contact")
                        .wrap(contact_limiter.clone())
                        .route(web::post().to(jambo_api::api::contact::send_contact)),
                )
                .service(
                    web::scope("/me")
                        .wrap(auth_mw.clone())
                        .route(
                            "/profile",
                            web::get().to(jambo_api::api::dashboard::get_profile),
                        )
                        .route(
                            "/games",
                            web::get().to(jambo_api::api::dashboard::list_games),
                        )
                        .route(
                            "/games",
                            web::post().to(jambo_api::api::dashboard::create_game),
                        )
                        .route(
                            "/games/{game_id}",
                            web::get().to(jambo_api::api::dashboard::get_game),
                        )
                        .route(
                            "/active-game",
                            web::get().to(jambo_api::api::dashboard::get_active_game),
                        )
                        .route(
                            "/invitations",
                            web::get().to(jambo_api::api::dashboard::get_invitations),
                        )
                        .route(
                            "/leaderboard",
                            web::get().to(jambo_api::api::leaderboard::get_leaderboard),
                        )
                        .route(
                            "/unfreeze",
                            web::post().to(jambo_api::api::unfreeze::create_unfreeze_order),
                        )
                        .route(
                            "/unfreeze/capture",
                            web::post().to(jambo_api::api::unfreeze::capture_unfreeze_order),
                        )
                        .route(
                            "/topup",
                            web::post().to(jambo_api::api::topup::create_topup_order),
                        )
                        .route(
                            "/topup/capture",
                            web::post().to(jambo_api::api::topup::capture_topup_order),
                        )
                        .route(
                            "/cashout",
                            web::post().to(jambo_api::api::cashout::request_cashout),
                        )
                        .route(
                            "/cashout",
                            web::get().to(jambo_api::api::cashout::list_cashouts),
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
                            web::post().to(jambo_api::api::dashboard::send_invites),
                        )
                        .route(
                            "/{game_id}/respond",
                            web::post().to(jambo_api::api::dashboard::respond_to_invite),
                        )
                        .route(
                            "/{game_id}/start",
                            web::post().to(jambo_api::api::dashboard::start_game),
                        )
                        .route(
                            "/{game_id}/play",
                            web::post().to(jambo_api::api::dashboard::play_game),
                        )
                        .route(
                            "/{game_id}/me",
                            web::get().to(jambo_api::api::dashboard::game_state),
                        )
                        .route(
                            "/{game_id}/spectate-token",
                            web::post().to(jambo_api::api::dashboard::mint_spectate_token),
                        ),
                )
                .service(web::scope("/users").wrap(auth_mw.clone()).route(
                    "/search",
                    web::get().to(jambo_api::api::dashboard::search_users),
                ))
                .service(
                    web::scope("/paypal")
                        .route(
                            "/return",
                            web::get().to(jambo_api::api::unfreeze::paypal_return),
                        )
                        .route(
                            "/cancel",
                            web::get().to(jambo_api::api::unfreeze::paypal_cancel),
                        )
                        .route(
                            "/topup/return",
                            web::get().to(jambo_api::api::topup::paypal_return_topup),
                        )
                        .route(
                            "/topup/cancel",
                            web::get().to(jambo_api::api::topup::paypal_cancel_topup),
                        ),
                )
                .configure(|cfg| {
                    if state.config.benchmark_mode {
                        cfg.service(
                            web::scope("/benchmark")
                                .service(
                                    web::resource("/create-multiplayer-game")
                                        .route(
                                            web::post().to(
                                                jambo_api::api::benchmark::create_benchmark_game,
                                            ),
                                        )
                                        .route(web::route().to(fallback::method_not_allowed)),
                                )
                                .service(
                                    web::resource("/cleanup")
                                        .route(
                                            web::post().to(
                                                jambo_api::api::benchmark::cleanup_benchmark_data,
                                            ),
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

    cfg.service(jambo_api::websocket::scope());
    configure_api_docs(cfg, state);
    cfg.default_service(web::route().to(fallback::not_found));
}

fn configure_api_docs(cfg: &mut web::ServiceConfig, state: &AppState) {
    if state.config.enable_api_docs {
        cfg.service(utoipa_swagger_ui::SwaggerUi::new("/swagger-ui/{_:.*}").url(
            "/api-docs/openapi.json",
            jambo_api::api::openapi::ApiDoc::openapi(),
        ));
    } else {
        cfg.route("/api-docs/openapi.json", web::get().to(serve_openapi_json));
    }
}

async fn serve_openapi_json() -> HttpResponse {
    match jambo_api::api::openapi::ApiDoc::openapi().to_pretty_json() {
        Ok(json) => HttpResponse::Ok()
            .content_type("application/json")
            .body(json),
        Err(e) => {
            tracing::error!("Failed to serialize OpenAPI spec: {e}");
            HttpResponse::InternalServerError().body("Failed to serialize OpenAPI spec")
        }
    }
}
