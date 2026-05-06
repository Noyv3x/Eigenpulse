//! Eigenpulse i18n — phf-backed catalog with reload-mode switching.
//!
//! Reload-mode (vs reactive): switching locale always does a full page
//! reload so SSR sees the new cookie. Trades a ~200 ms reload for a
//! minimal wasm bundle (no reactive context, no ICU runtime).

pub mod cookie;
pub mod errors;
pub mod locale;
pub mod server_fns;

#[cfg(feature = "hydrate")]
pub mod client;

#[cfg(feature = "ssr")]
pub mod middleware;

pub use crate::cookie::build_set_cookie;
pub use crate::errors::{err, err_with, parse_err, ERR_PREFIX};
pub use crate::locale::{Locale, LOCALE_COOKIE, LOCALE_COOKIE_MAX_AGE_SECS};
pub use crate::server_fns::{set_user_locale, SetUserLocale};

#[cfg(feature = "hydrate")]
pub use crate::client::switch_locale_via_reload;

#[cfg(feature = "ssr")]
pub use crate::middleware::locale_layer;

include!(concat!(env!("OUT_DIR"), "/generated.rs"));

pub fn t(locale: Locale, key: &str) -> &'static str {
    let map = match locale {
        Locale::ZhCn => &ZH_CN,
        Locale::En => &EN,
    };
    if let Some(v) = map.get(key) {
        return v;
    }
    #[cfg(feature = "ssr")]
    tracing::warn!(
        target: "ep_i18n",
        locale = locale.as_code(),
        key,
        "i18n: missing key (rendered as key string)"
    );
    "[[missing i18n key]]"
}

/// Substitute `{name}` placeholders in the template. Single-pass scan
/// allocates exactly one buffer (vs `String::replace` per-arg, which
/// reallocates per call).
pub fn tf(locale: Locale, key: &str, args: &[(&str, &str)]) -> String {
    let template = t(locale, key);
    if args.is_empty() || !template.contains('{') {
        return template.to_string();
    }
    let mut out = String::with_capacity(template.len());
    let bytes = template.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'{' {
            if let Some(end_rel) = template[i + 1..].find('}') {
                let name = &template[i + 1..i + 1 + end_rel];
                if let Some((_, value)) = args.iter().find(|(n, _)| *n == name) {
                    out.push_str(value);
                    i += 1 + end_rel + 1;
                    continue;
                }
            }
        }
        let ch = template[i..].chars().next().unwrap();
        out.push(ch);
        i += ch.len_utf8();
    }
    out
}

/// `t!(locale, app.dashboard.title)` → `t(locale, "app.dashboard.title")`.
#[macro_export]
macro_rules! t {
    ($locale:expr, $first:ident $(. $rest:ident)*) => {
        $crate::t(
            $locale,
            concat!(stringify!($first) $(, ".", stringify!($rest))*),
        )
    };
}

/// `tf!(locale, finance.err.txn_not_found, doc_id = id)`.
#[macro_export]
macro_rules! tf {
    ($locale:expr, $first:ident $(. $rest:ident)*, $($name:ident = $value:expr),* $(,)?) => {
        $crate::tf(
            $locale,
            concat!(stringify!($first) $(, ".", stringify!($rest))*),
            &[$((stringify!($name), $value)),*],
        )
    };
}

#[cfg(feature = "ssr")]
pub fn provide_locale_from_request_parts() -> Locale {
    use leptos::prelude::*;
    let locale = use_context::<axum::http::request::Parts>()
        .and_then(|p| p.extensions.get::<Locale>().copied())
        .unwrap_or_default();
    provide_context(locale);
    locale
}

/// SSR: pulls from leptos context.
/// Hydrate: leptos context first, otherwise reads `<html lang>` once and
/// caches — `<html lang>` cannot change without a page reload (the
/// design contract), so a single read per wasm instance is sound.
pub fn use_locale() -> Locale {
    if let Some(loc) = leptos::prelude::use_context::<Locale>() {
        return loc;
    }
    #[cfg(feature = "hydrate")]
    {
        use std::sync::OnceLock;
        static CACHED: OnceLock<Locale> = OnceLock::new();
        return *CACHED.get_or_init(|| {
            web_sys::window()
                .and_then(|w| w.document())
                .and_then(|d| d.document_element())
                .and_then(|el| el.get_attribute("lang"))
                .map(|lang| Locale::parse_or_default(&lang))
                .unwrap_or(Locale::DEFAULT)
        });
    }
    #[cfg(not(feature = "hydrate"))]
    Locale::DEFAULT
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn t_macro_path_concat() {
        let s: &str = t!(Locale::En, ep_i18n.test.macro_check);
        assert_eq!(s, "[[missing i18n key]]");
    }

    #[test]
    fn tf_no_panic_on_missing_key() {
        let s = tf(Locale::En, "ep_i18n.test.placeholder", &[("x", "VAL")]);
        assert!(!s.is_empty());
        assert!(!s.contains("{x}"));
    }

    #[test]
    fn locale_round_trip() {
        assert_eq!(Locale::ZhCn.toggle(), Locale::En);
        assert_eq!(Locale::En.toggle(), Locale::ZhCn);
        assert_eq!(Locale::DEFAULT, Locale::ZhCn);
    }
}
