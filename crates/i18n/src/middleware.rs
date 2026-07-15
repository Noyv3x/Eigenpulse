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
        if let Some(loc) = query_locale(q) {
            return loc;
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

fn query_locale(query: &str) -> Option<Locale> {
    for pair in query.split('&') {
        let (key, value) = pair.split_once('=').unwrap_or((pair, ""));
        if key != "locale" {
            continue;
        }
        let Some(value) = decode_form_component(value) else {
            continue;
        };
        if let Some(loc) = Locale::parse(&value) {
            return Some(loc);
        }
    }
    None
}

fn decode_form_component(value: &str) -> Option<String> {
    if !value.as_bytes().iter().any(|b| *b == b'%' || *b == b'+') {
        return Some(value.to_string());
    }

    let mut out = Vec::with_capacity(value.len());
    let mut bytes = value.as_bytes().iter().copied();
    while let Some(b) = bytes.next() {
        match b {
            b'+' => out.push(b' '),
            b'%' => {
                let hi = bytes.next().and_then(hex_value)?;
                let lo = bytes.next().and_then(hex_value)?;
                out.push((hi << 4) | lo);
            }
            _ => out.push(b),
        }
    }
    String::from_utf8(out).ok()
}

fn hex_value(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
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
    fn url_query_decodes_percent_encoded_locale() {
        let r = req_with(
            &[("cookie", "ep_locale=en"), ("accept-language", "en")],
            "/x?foo=1&locale=zh%2DCN",
        );
        assert_eq!(detect_locale(&r), Locale::ZhCn);
    }

    #[test]
    fn invalid_url_query_locale_falls_back_to_cookie() {
        let r = req_with(&[("cookie", "ep_locale=en")], "/x?locale=zh%XXCN");
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
