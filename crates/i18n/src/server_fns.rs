use leptos::prelude::*;
use leptos::server_fn::ServerFnError;
use serde::{Deserialize, Serialize};

#[cfg(feature = "ssr")]
use crate::locale::Locale;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SetLocale {
    pub locale: String,
}

/// Persist the user's locale to `app_user.locale` and append a Set-Cookie
/// so the next request reflects the choice without a topbar re-toggle.
#[server(SetUserLocale, "/api/_internal/i18n", "Url", "set_user_locale")]
pub async fn set_user_locale(locale: String) -> Result<(), ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        ep_auth::require_user_for_server_fn().await?;

        let parsed = Locale::parse(&locale)
            .ok_or_else(|| crate::errors::err_with("app.common.unknown_locale", &locale))?;

        let state: ep_core::AppState = expect_context();
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
