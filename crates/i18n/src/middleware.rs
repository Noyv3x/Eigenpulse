//! Detection priority: `?locale=` query → `ep_locale` cookie →
//! `Accept-Language` header → `Locale::DEFAULT`.

#![cfg(feature = "ssr")]

use crate::locale::{Locale, LOCALE_COOKIE};
use axum::{extract::Request, middleware::Next, response::Response};
use axum_extra::extract::cookie::CookieJar;

pub async fn locale_layer(mut req: Request, next: Next) -> Response {
    let locale = detect_locale(&req);
    req.extensions_mut().insert(locale);
    next.run(req).await
}

fn detect_locale(req: &Request) -> Locale {
    if let Some(q) = req.uri().query() {
        for pair in q.split('&') {
            if let Some(v) = pair.strip_prefix("locale=") {
                if let Some(loc) = Locale::parse(v) {
                    return loc;
                }
            }
        }
    }
    let jar = CookieJar::from_headers(req.headers());
    if let Some(value) = jar.get(LOCALE_COOKIE).map(|c| c.value()) {
        if let Some(loc) = Locale::parse(value) {
            return loc;
        }
    }
    if let Some(al) = req
        .headers()
        .get(axum::http::header::ACCEPT_LANGUAGE)
        .and_then(|v| v.to_str().ok())
    {
        return Locale::parse_accept_language(al);
    }
    Locale::DEFAULT
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;

    fn req_with(headers: &[(&str, &str)], uri: &str) -> Request {
        let mut b = Request::builder().uri(uri);
        for (k, v) in headers {
            b = b.header(*k, *v);
        }
        b.body(Body::empty()).unwrap()
    }

    #[test]
    fn url_query_wins() {
        let r = req_with(
            &[("cookie", "ep_locale=zh-CN"), ("accept-language", "zh-CN")],
            "/x?locale=en",
        );
        assert_eq!(detect_locale(&r), Locale::En);
    }

    #[test]
    fn cookie_beats_accept_language() {
        let r = req_with(
            &[
                ("cookie", "foo=bar; ep_locale=en; baz=qux"),
                ("accept-language", "zh-CN"),
            ],
            "/x",
        );
        assert_eq!(detect_locale(&r), Locale::En);
    }

    #[test]
    fn accept_language_fallback() {
        let r = req_with(&[("accept-language", "fr,en;q=0.5")], "/x");
        assert_eq!(detect_locale(&r), Locale::En);
    }

    #[test]
    fn default_when_nothing() {
        let r = req_with(&[], "/x");
        assert_eq!(detect_locale(&r), Locale::DEFAULT);
    }
}
