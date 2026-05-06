//! Error-code wire format: `ServerFnError::Args("err:<code>[:<payload>]")`.
//! The client pattern-matches on the variant (not Display — that depends
//! on third-party formatting that can drift), then renders via i18n.

use leptos::server_fn::ServerFnError;

pub const ERR_PREFIX: &str = "err:";

pub fn err(code: &str) -> ServerFnError {
    ServerFnError::Args(format!("{ERR_PREFIX}{code}"))
}

pub fn err_with(code: &str, payload: impl std::fmt::Display) -> ServerFnError {
    ServerFnError::Args(format!("{ERR_PREFIX}{code}:{payload}"))
}

/// `splitn(2, ':')` keeps payload verbatim if it itself contains a colon.
pub fn parse_err(e: &ServerFnError) -> Option<(&str, Option<&str>)> {
    let ServerFnError::Args(s) = e else {
        return None;
    };
    let rest = s.strip_prefix(ERR_PREFIX)?;
    let mut it = rest.splitn(2, ':');
    let code = it.next()?;
    Some((code, it.next()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_no_payload() {
        let e = err("finance.txn_not_found");
        let (code, payload) = parse_err(&e).expect("should parse");
        assert_eq!(code, "finance.txn_not_found");
        assert_eq!(payload, None);
    }

    #[test]
    fn round_trip_with_payload() {
        let e = err_with("finance.txn_not_found", "FIN-26092");
        let (code, payload) = parse_err(&e).expect("should parse");
        assert_eq!(code, "finance.txn_not_found");
        assert_eq!(payload, Some("FIN-26092"));
    }

    #[test]
    fn payload_with_colon_kept_intact() {
        let e = err_with("auth.invalid_url", "https://example.com:443/x");
        let (code, payload) = parse_err(&e).expect("should parse");
        assert_eq!(code, "auth.invalid_url");
        assert_eq!(payload, Some("https://example.com:443/x"));
    }

    #[test]
    fn non_args_variant_returns_none() {
        let e = ServerFnError::ServerError("boom".into());
        assert!(parse_err(&e).is_none());
    }

    #[test]
    fn args_without_prefix_returns_none() {
        let e = ServerFnError::Args("just a plain message".into());
        assert!(parse_err(&e).is_none());
    }
}
