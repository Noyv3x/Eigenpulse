use crate::Severity;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct NotifyMessage {
    pub severity: Severity,
    pub module: Option<String>,
    pub title: String,
    pub body: Option<String>,
    pub link: Option<String>,
    pub doc_ref: Option<String>,
}

impl NotifyMessage {
    pub fn info(title: impl Into<String>) -> Self {
        Self { severity: Severity::Info, module: None, title: title.into(), body: None, link: None, doc_ref: None }
    }
    pub fn warn(title: impl Into<String>) -> Self {
        Self { severity: Severity::Warn, module: None, title: title.into(), body: None, link: None, doc_ref: None }
    }
    pub fn crit(title: impl Into<String>) -> Self {
        Self { severity: Severity::Crit, module: None, title: title.into(), body: None, link: None, doc_ref: None }
    }
    pub fn module(mut self, m: impl Into<String>) -> Self { self.module = Some(m.into()); self }
    pub fn body(mut self, b: impl Into<String>) -> Self { self.body = Some(b.into()); self }
    pub fn link(mut self, l: impl Into<String>) -> Self { self.link = Some(l.into()); self }
    pub fn doc_ref(mut self, d: impl Into<String>) -> Self { self.doc_ref = Some(d.into()); self }
}

/// Trait object stored in `AppState` so modules can dispatch without depending on `ep-notify`.
#[async_trait::async_trait]
pub trait NotifyBusTrait: Send + Sync + 'static {
    async fn dispatch(&self, msg: NotifyMessage) -> anyhow::Result<i64>;
    fn subscribe(&self) -> tokio::sync::broadcast::Receiver<NotifyMessage>;
}

pub type NotifyBusHandle = Arc<dyn NotifyBusTrait>;
