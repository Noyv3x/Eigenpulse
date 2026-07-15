use axum::{extract::State, Extension, Json};
use ep_auth::{require_scope, AuthPat};
use ep_core::{ApiJson, AppState, NotifyMessage, Severity, SCOPE_NOTIFICATIONS_WRITE};
use serde::{Deserialize, Serialize};

use crate::errors::ApiError;

const MAX_TITLE_CHARS: usize = 128;
const MAX_BODY_CHARS: usize = 2_000;
const MAX_LINK_CHARS: usize = 512;

#[derive(Debug, Deserialize)]
pub(super) struct NotifyInput {
    #[serde(default)]
    pub severity: Option<String>,
    pub title: String,
    pub body: Option<String>,
    pub link: Option<String>,
    pub source: Option<String>,
}

fn parse_request_severity(input: Option<&str>) -> Result<Severity, ApiError> {
    match input.map(str::trim).filter(|s| !s.is_empty()) {
        Some(raw) => Severity::try_parse(raw)
            .ok_or_else(|| ApiError::BadRequest(format!("unknown severity: {raw}"))),
        None => Ok(Severity::Info),
    }
}

fn normalize_link(input: Option<String>) -> Result<Option<String>, ApiError> {
    let Some(link) = input.and_then(|v| ep_core::trim_to_option(&v)) else {
        return Ok(None);
    };
    if link.chars().count() > MAX_LINK_CHARS {
        return Err(ApiError::BadRequest(format!(
            "link must be at most {MAX_LINK_CHARS} characters"
        )));
    }
    if ep_core::safe_in_app_path(&link).is_some() {
        Ok(Some(link))
    } else {
        Err(ApiError::BadRequest(
            "link must be an in-app absolute path".into(),
        ))
    }
}

fn normalize_source(input: Option<String>) -> Result<Option<String>, ApiError> {
    let Some(source) = input.and_then(|v| ep_core::trim_to_option(&v)) else {
        return Ok(None);
    };
    let source = source.to_ascii_lowercase();
    let valid = (2..=32).contains(&source.len())
        && source
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-');
    if valid {
        Ok(Some(source))
    } else {
        Err(ApiError::BadRequest(
            "source must be 2..=32 lowercase letters, digits, or hyphens".into(),
        ))
    }
}

fn normalize_title(input: &str) -> Result<String, ApiError> {
    let title = ep_core::trim_to_option(input)
        .ok_or_else(|| ApiError::BadRequest("title is required".into()))?;
    if title.chars().count() > MAX_TITLE_CHARS {
        return Err(ApiError::BadRequest(format!(
            "title must be at most {MAX_TITLE_CHARS} characters"
        )));
    }
    Ok(title)
}

fn normalize_body(input: Option<String>) -> Result<Option<String>, ApiError> {
    let Some(body) = input.and_then(|v| ep_core::trim_to_option(&v)) else {
        return Ok(None);
    };
    if body.chars().count() > MAX_BODY_CHARS {
        return Err(ApiError::BadRequest(format!(
            "body must be at most {MAX_BODY_CHARS} characters"
        )));
    }
    Ok(Some(body))
}

#[derive(Debug, Serialize)]
pub(super) struct NotifyResp {
    pub id: i64,
}

pub(super) async fn handler(
    State(state): State<AppState>,
    Extension(pat): Extension<AuthPat>,
    ApiJson(input): ApiJson<NotifyInput>,
) -> Result<Json<NotifyResp>, ApiError> {
    if require_scope(&pat, SCOPE_NOTIFICATIONS_WRITE).is_err() {
        return Err(ApiError::Forbidden(format!(
            "requires scope: {SCOPE_NOTIFICATIONS_WRITE}"
        )));
    }
    let severity = parse_request_severity(input.severity.as_deref())?;
    let title = normalize_title(&input.title)?;
    let msg = NotifyMessage {
        severity,
        source: normalize_source(input.source)?,
        title,
        body: normalize_body(input.body)?,
        link: normalize_link(input.link)?,
    };
    let id = state
        .notify
        .dispatch(msg)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;
    Ok(Json(NotifyResp { id }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::app_state;
    use axum::{extract::State, Extension, Json};
    use std::sync::{Arc, Mutex};

    #[derive(Default)]
    struct RecordingNotifyBus {
        messages: Mutex<Vec<NotifyMessage>>,
    }

    #[async_trait::async_trait]
    impl ep_core::NotifyBusTrait for RecordingNotifyBus {
        async fn dispatch(&self, msg: NotifyMessage) -> anyhow::Result<i64> {
            self.messages.lock().expect("messages lock").push(msg);
            Ok(9001)
        }

        fn subscribe(&self) -> tokio::sync::broadcast::Receiver<ep_core::NotifyEvent> {
            let (_tx, rx) = tokio::sync::broadcast::channel(1);
            rx
        }
    }

    async fn test_state(notify: Arc<RecordingNotifyBus>) -> AppState {
        let db = sqlx::SqlitePool::connect("sqlite::memory:")
            .await
            .expect("pool");
        app_state(db, notify)
    }

    #[test]
    fn parse_request_severity_defaults_blank_to_info() {
        assert_eq!(parse_request_severity(None).unwrap(), Severity::Info);
        assert_eq!(parse_request_severity(Some(" ")).unwrap(), Severity::Info);
    }

    #[test]
    fn parse_request_severity_rejects_unknown_values() {
        let err = parse_request_severity(Some("urgent")).expect_err("unknown should fail");
        assert!(matches!(err, ApiError::BadRequest(_)));
    }

    #[test]
    fn normalize_link_accepts_only_safe_in_app_paths() {
        assert_eq!(
            normalize_link(Some(" /finance?tab=reports ".into())).expect("safe link"),
            Some("/finance?tab=reports".into())
        );
        assert_eq!(
            normalize_link(Some("   ".into())).expect("blank link"),
            None
        );

        for raw in [
            "https://example.com",
            "//example.com",
            "javascript:alert(1)",
            "/finance\\evil",
            "/finance%0d%0aevil",
            "/finance%7F",
        ] {
            let err = normalize_link(Some(raw.into())).expect_err("unsafe link should fail");
            assert!(matches!(err, ApiError::BadRequest(_)));
        }
    }

    #[test]
    fn normalize_link_rejects_overlong_targets() {
        let overlong = format!("/{}", "x".repeat(MAX_LINK_CHARS));
        let err = normalize_link(Some(overlong)).expect_err("overlong link should fail");
        assert!(matches!(err, ApiError::BadRequest(_)));
    }

    #[test]
    fn normalize_source_canonicalizes_slugs() {
        assert_eq!(normalize_source(None).unwrap(), None);
        assert_eq!(normalize_source(Some("   ".into())).unwrap(), None);
        assert_eq!(
            normalize_source(Some(" Finance ".into())).expect("source slug"),
            Some("finance".into())
        );
        assert_eq!(
            normalize_source(Some("IOS-SHORTCUT".into())).expect("source slug"),
            Some("ios-shortcut".into())
        );
    }

    #[test]
    fn normalize_source_rejects_free_text() {
        for raw in [
            "x",
            "my script",
            "https://example.com",
            "module-name-that-is-definitely-over-thirty-two-chars",
        ] {
            let err = normalize_source(Some(raw.into())).expect_err("bad source should fail");
            assert!(matches!(err, ApiError::BadRequest(_)));
        }
    }

    #[test]
    fn normalize_title_and_body_enforce_reasonable_lengths() {
        assert_eq!(normalize_title("  hello  ").unwrap(), "hello");
        assert!(matches!(
            normalize_title("   ").expect_err("blank title"),
            ApiError::BadRequest(_)
        ));
        assert!(matches!(
            normalize_title(&"x".repeat(MAX_TITLE_CHARS + 1)).expect_err("long title"),
            ApiError::BadRequest(_)
        ));

        assert_eq!(
            normalize_body(Some("  body  ".into())).unwrap(),
            Some("body".into())
        );
        assert_eq!(normalize_body(Some("   ".into())).unwrap(), None);
        assert!(matches!(
            normalize_body(Some("x".repeat(MAX_BODY_CHARS + 1))).expect_err("long body"),
            ApiError::BadRequest(_)
        ));
    }

    #[test]
    fn forbidden_error_keeps_required_scope_message() {
        let err = ApiError::Forbidden("requires scope: notifications:write".into());
        assert_eq!(
            err.to_string(),
            "forbidden: requires scope: notifications:write"
        );
    }

    #[tokio::test]
    async fn handler_requires_notify_write_scope() {
        let notify = Arc::new(RecordingNotifyBus::default());
        let state = test_state(notify).await;
        let pat = AuthPat {
            id: 1,
            name: "reader".into(),
            scopes: vec!["example:read".into()],
        };

        let err = handler(
            State(state),
            Extension(pat),
            ApiJson(NotifyInput {
                severity: None,
                title: "hello".into(),
                body: None,
                link: None,
                source: None,
            }),
        )
        .await
        .expect_err("missing scope should fail");

        assert!(matches!(err, ApiError::Forbidden(_)));
    }

    #[tokio::test]
    async fn handler_trims_input_and_dispatches_notification() {
        let notify = Arc::new(RecordingNotifyBus::default());
        let state = test_state(notify.clone()).await;
        let pat = AuthPat {
            id: 1,
            name: "writer".into(),
            scopes: vec![SCOPE_NOTIFICATIONS_WRITE.into()],
        };

        let Json(resp) = handler(
            State(state),
            Extension(pat),
            ApiJson(NotifyInput {
                severity: Some(" warn ".into()),
                title: "  Build done  ".into(),
                body: Some("  ok  ".into()),
                link: Some("  /finance  ".into()),
                source: Some("  finance  ".into()),
            }),
        )
        .await
        .expect("notify response");

        assert_eq!(resp.id, 9001);
        let messages = notify.messages.lock().expect("messages lock");
        assert_eq!(messages.len(), 1);
        let msg = &messages[0];
        assert_eq!(msg.severity, Severity::Warn);
        assert_eq!(msg.source.as_deref(), Some("finance"));
        assert_eq!(msg.title, "Build done");
        assert_eq!(msg.body.as_deref(), Some("ok"));
        assert_eq!(msg.link.as_deref(), Some("/finance"));
    }
}
