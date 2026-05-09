use leptos::prelude::*;
use leptos::server_fn::ServerFnError;

#[cfg(feature = "ssr")]
use crate::locale::Locale;

#[cfg(feature = "ssr")]
const MAX_LOCALE_CODE_CHARS: usize = 16;

#[cfg(feature = "ssr")]
fn parse_locale_input(locale: &str) -> Result<Locale, ServerFnError> {
    let locale = locale.trim();
    Locale::parse(locale).ok_or_else(|| {
        let payload = if locale.chars().count() > MAX_LOCALE_CODE_CHARS {
            "too-long".to_string()
        } else {
            locale.to_string()
        };
        crate::errors::err_with("app.common.unknown_locale", payload)
    })
}

/// Persist the user's locale to `app_user.locale` and append a Set-Cookie
/// so the next request reflects the choice without a topbar re-toggle.
#[server(SetUserLocale, "/api/_internal/i18n", "Url", "set_user_locale")]
pub async fn set_user_locale(locale: String) -> Result<(), ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        ep_auth::require_user_for_server_fn().await?;

        let parsed = parse_locale_input(&locale)?;

        let state = ep_core::app_state_context()?;
        sqlx::query("UPDATE app_user SET locale = ?1 WHERE id = 1")
            .bind(parsed.as_code())
            .execute(&state.db)
            .await
            .map_err(ep_core::server_err)?;

        // Overwrite the existing cookie. The `locale_layer` only seeds on
        // first touch; once a value is set, only this Set-Cookie can change it.
        if let Some(resp) = use_context::<leptos_axum::ResponseOptions>() {
            if let Ok(value) =
                axum::http::HeaderValue::from_str(&crate::cookie::build_set_cookie(parsed))
            {
                resp.append_header(axum::http::header::SET_COOKIE, value);
            }
        }

        Ok(())
    }
    #[cfg(not(feature = "ssr"))]
    Err(ep_core::server_err("ssr-only"))
}

#[cfg(all(test, feature = "ssr"))]
mod tests {
    use super::*;

    #[test]
    fn parse_locale_input_trims_supported_codes() {
        assert_eq!(parse_locale_input(" zh-CN ").unwrap(), Locale::ZhCn);
        assert_eq!(parse_locale_input(" en-US ").unwrap(), Locale::En);
    }

    #[test]
    fn parse_locale_input_caps_unknown_payload() {
        let err = parse_locale_input(&"x".repeat(MAX_LOCALE_CODE_CHARS + 1))
            .expect_err("overlong locale should fail");
        assert_eq!(
            crate::errors::parse_err(&err).map(|(code, payload)| (code, payload.unwrap_or(""))),
            Some(("app.common.unknown_locale", "too-long"))
        );
    }
}
