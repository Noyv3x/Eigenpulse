use axum::{extract::State, Extension, Json};
use ep_auth::{AuthPat, pat::require_scope};
use ep_core::{AppState, NotifyMessage, Severity};
use serde::{Deserialize, Serialize};

use crate::errors::ApiError;

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

#[derive(Debug, Serialize)]
pub struct NotifyResp { pub id: i64 }

pub async fn handler(
    State(state): State<AppState>,
    Extension(pat): Extension<AuthPat>,
    Json(input): Json<NotifyInput>,
) -> Result<Json<NotifyResp>, ApiError> {
    if let Err(r) = require_scope(&pat, "notify:write") {
        return Err(ApiError::Forbidden(format!("{:?}", r.status())));
    }
    let sev = input.severity.as_deref().map(Severity::parse).unwrap_or(Severity::Info);
    let msg = NotifyMessage {
        severity: sev,
        module: input.module,
        title: input.title,
        body: input.body,
        link: input.link,
        doc_ref: input.doc_ref,
    };
    let id = state.notify.dispatch(msg).await
        .map_err(|e| ApiError::Internal(e.to_string()))?;
    Ok(Json(NotifyResp { id }))
}
