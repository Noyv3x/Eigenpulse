use axum::{extract::State, Extension, Json};
use ep_auth::{require_scope, AuthPat};
use ep_core::{ApiJson, AppState, NotifyMessage, Severity, SCOPE_NOTIFY_WRITE};
use serde::{Deserialize, Serialize};

use crate::errors::ApiError;

const MAX_TITLE_CHARS: usize = 128;
const MAX_BODY_CHARS: usize = 2_000;
const MAX_LINK_CHARS: usize = 512;

#[derive(Debug, Deserialize)]
pub struct NotifyInput {
    #[serde(default)]
    pub severity: Option<String>,
    pub title: String,
    pub body: Option<String>,
    pub link: Option<String>,
    pub doc_ref: Option<String>,
    pub module: Option<String>,
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

fn normalize_doc_ref(input: Option<String>) -> Result<Option<String>, ApiError> {
    let Some(doc_ref) = input.and_then(|v| ep_core::trim_to_option(&v)) else {
        return Ok(None);
    };
    if ep_core::safe_doc_id(&doc_ref).is_some() {
        Ok(Some(doc_ref))
    } else {
        Err(ApiError::BadRequest(
            "doc_ref must look like an Eigenpulse doc id".into(),
        ))
    }
}

fn normalize_module(input: Option<String>) -> Result<Option<String>, ApiError> {
    let Some(module) = input.and_then(|v| ep_core::trim_to_option(&v)) else {
        return Ok(None);
    };
    let module = module.to_ascii_uppercase();
    let valid = (2..=16).contains(&module.len())
        && module
            .bytes()
            .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit() || b == b'-');
    if valid {
        Ok(Some(module))
    } else {
        Err(ApiError::BadRequest(
            "module must be 2..=16 chars, uppercase letters / digits / hyphens only".into(),
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
pub struct NotifyResp {
    pub id: i64,
}

pub async fn handler(
    State(state): State<AppState>,
    Extension(pat): Extension<AuthPat>,
    ApiJson(input): ApiJson<NotifyInput>,
) -> Result<Json<NotifyResp>, ApiError> {
    if require_scope(&pat, SCOPE_NOTIFY_WRITE).is_err() {
        return Err(ApiError::Forbidden(format!(
            "requires scope: {SCOPE_NOTIFY_WRITE}"
        )));
    }
    let severity = parse_request_severity(input.severity.as_deref())?;
    let title = normalize_title(&input.title)?;
    let msg = NotifyMessage {
        severity,
        module: normalize_module(input.module)?,
        title,
        body: normalize_body(input.body)?,
        link: normalize_link(input.link)?,
        doc_ref: normalize_doc_ref(input.doc_ref)?,
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

        fn subscribe(&self) -> tokio::sync::broadcast::Receiver<NotifyMessage> {
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
            normalize_link(Some(" /reports?scope=fin ".into())).expect("safe link"),
            Some("/reports?scope=fin".into())
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
    fn normalize_doc_ref_accepts_only_safe_doc_ids() {
        assert_eq!(
            normalize_doc_ref(Some(" FIT-S-0412 ".into())).expect("safe doc_ref"),
            Some("FIT-S-0412".into())
        );
        assert_eq!(
            normalize_doc_ref(Some("   ".into())).expect("blank doc_ref"),
            None
        );

        for raw in [
            "https://example.com",
            "../FIT-S-0412",
            "fit-s-0412",
            "FIT--0412",
        ] {
            let err = normalize_doc_ref(Some(raw.into())).expect_err("unsafe doc_ref should fail");
            assert!(matches!(err, ApiError::BadRequest(_)));
        }
    }

    #[test]
    fn normalize_module_canonicalizes_short_codes() {
        assert_eq!(normalize_module(None).unwrap(), None);
        assert_eq!(normalize_module(Some("   ".into())).unwrap(), None);
        assert_eq!(
            normalize_module(Some(" fin ".into())).expect("module code"),
            Some("FIN".into())
        );
        assert_eq!(
            normalize_module(Some("IOS-SHORTCUT".into())).expect("module code"),
            Some("IOS-SHORTCUT".into())
        );
    }

    #[test]
    fn normalize_module_rejects_free_text() {
        for raw in [
            "x",
            "my script",
            "https://example.com",
            "MODULE-NAME-TOO-LONG",
        ] {
            let err = normalize_module(Some(raw.into())).expect_err("bad module should fail");
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
        let err = ApiError::Forbidden("requires scope: notify:write".into());
        assert_eq!(err.to_string(), "forbidden: requires scope: notify:write");
    }

    #[tokio::test]
    async fn handler_requires_notify_write_scope() {
        let notify = Arc::new(RecordingNotifyBus::default());
        let state = test_state(notify).await;
        let pat = AuthPat {
            id: 1,
            name: "reader".into(),
            scopes: vec![ep_core::SCOPE_FIN_READ.into()],
        };

        let err = handler(
            State(state),
            Extension(pat),
            ApiJson(NotifyInput {
                severity: None,
                title: "hello".into(),
                body: None,
                link: None,
                doc_ref: None,
                module: None,
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
            scopes: vec![SCOPE_NOTIFY_WRITE.into()],
        };

        let Json(resp) = handler(
            State(state),
            Extension(pat),
            ApiJson(NotifyInput {
                severity: Some(" warn ".into()),
                title: "  Build done  ".into(),
                body: Some("  ok  ".into()),
                link: Some("  /reports  ".into()),
                doc_ref: Some("  FIN-26001  ".into()),
                module: Some("  fin  ".into()),
            }),
        )
        .await
        .expect("notify response");

        assert_eq!(resp.id, 9001);
        let messages = notify.messages.lock().expect("messages lock");
        assert_eq!(messages.len(), 1);
        let msg = &messages[0];
        assert_eq!(msg.severity, Severity::Warn);
        assert_eq!(msg.module.as_deref(), Some("FIN"));
        assert_eq!(msg.title, "Build done");
        assert_eq!(msg.body.as_deref(), Some("ok"));
        assert_eq!(msg.link.as_deref(), Some("/reports"));
        assert_eq!(msg.doc_ref.as_deref(), Some("FIN-26001"));
    }
}
