use utoipa::openapi::security::{ApiKey, ApiKeyValue, SecurityScheme};
use utoipa::{Modify, OpenApi};

struct SecurityAddon;

impl Modify for SecurityAddon {
    fn modify(&self, openapi: &mut utoipa::openapi::OpenApi) {
        if let Some(components) = openapi.components.as_mut() {
            components.add_security_scheme(
                "cookie_auth",
                SecurityScheme::ApiKey(ApiKey::Cookie(ApiKeyValue::new("Authorization"))),
            );
        }
    }
}

#[derive(OpenApi)]
#[openapi(
    info(
        title = "Jambo API",
        version = "0.2.0",
        description = "REST API for the Jambo card game backend",
    ),
    paths(
        crate::api::system::health_check,
        crate::api::system::metrics,
        crate::api::anonymous::get_anonymous_stats,
        crate::api::config::client_config,
        crate::api::quickie::create_quick_game,
        crate::api::game::play_card,
        crate::api::game::advance_bot,
        crate::api::game::evaluate_round,
        crate::i18n::lang_endpoint::set_language,
        crate::i18n::lang_endpoint::get_current_lang,
        crate::i18n::lang_endpoint::get_languages,
        crate::api::auth::register,
        crate::api::auth::login,
        crate::api::auth::forgot_password,
        crate::api::auth::reset_password,
        crate::api::auth::logout,
        crate::api::auth::me,
        crate::api::contact::send_contact,
        crate::api::dashboard::get_profile,
        crate::api::dashboard::list_games,
        crate::api::dashboard::create_game,
        crate::api::dashboard::get_game,
        crate::api::dashboard::get_active_game,
        crate::api::dashboard::get_invitations,
        crate::api::dashboard::send_invites,
        crate::api::dashboard::respond_to_invite,
        crate::api::dashboard::start_game,
        crate::api::dashboard::play_game,
        crate::api::dashboard::game_state,
        crate::api::dashboard::mint_spectate_token,
        crate::api::dashboard::search_users,
        crate::api::leaderboard::get_leaderboard,
        crate::api::room::create_room,
        crate::api::room::list_rooms,
        crate::api::room::join_room,
        crate::api::room::get_room,
        crate::api::room::invite_to_room,
        crate::api::room::leave_room,
        crate::api::room::create_run,
        crate::api::room::list_runs,
        crate::api::room::get_active_run,
        crate::api::room::join_run,
        crate::api::room::leave_run,
        crate::api::room::start_next_game,
        crate::api::room::get_current_game,
        crate::api::unfreeze::create_unfreeze_order,
        crate::api::unfreeze::capture_unfreeze_order,
        crate::api::unfreeze::paypal_return,
        crate::api::unfreeze::paypal_cancel,
        crate::api::topup::create_topup_order,
        crate::api::topup::capture_topup_order,
        crate::api::topup::paypal_return_topup,
        crate::api::topup::paypal_cancel_topup,
        crate::api::benchmark::create_benchmark_game,
        crate::api::benchmark::cleanup_benchmark_data,
    ),
    modifiers(&SecurityAddon),
    tags(
        (name = "system", description = "Health and metrics endpoints"),
        (name = "config", description = "Client configuration"),
        (name = "anonymous", description = "Anonymous gameplay"),
        (name = "auth", description = "Authentication endpoints"),
        (name = "game", description = "Gameplay endpoints"),
        (name = "dashboard", description = "User dashboard and game management"),
        (name = "rooms", description = "Multiplayer rooms and runs"),
        (name = "leaderboard", description = "Leaderboard"),
        (name = "payments", description = "PayPal top-up and unfreeze"),
        (name = "contact", description = "Contact form"),
        (name = "i18n", description = "Language selection"),
        (name = "benchmark", description = "Benchmark tooling (benchmark mode only)"),
    ),
)]
pub struct ApiDoc;

#[cfg(test)]
mod tests {
    use super::ApiDoc;
    use utoipa::OpenApi;

    #[test]
    fn openapi_spec_is_valid() {
        let doc = ApiDoc::openapi();
        let json = doc.to_json().expect("spec must serialize to JSON");
        assert!(json.contains("\"openapi\""), "spec missing openapi field");
        assert!(
            json.contains("\"/api/auth/login\""),
            "login path missing from spec"
        );
        assert!(
            json.contains("\"/api/me/games\""),
            "game history path missing from spec"
        );
    }

    #[test]
    fn openapi_spec_exposes_cookie_auth_scheme() {
        let doc = ApiDoc::openapi();
        let json = doc.to_json().expect("spec must serialize to JSON");
        assert!(
            json.contains("cookie_auth"),
            "cookie auth security scheme missing from spec"
        );
    }
}
