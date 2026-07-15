use crate::Severity;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct NotifyMessage {
    pub severity: Severity,
    pub source: Option<String>,
    pub title: String,
    pub body: Option<String>,
    pub link: Option<String>,
}

/// A notification after its durable row has been committed. The database id
/// lets SSE clients detect gaps and reconcile their unread count.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct NotifyEvent {
    pub id: i64,
    pub message: NotifyMessage,
}

impl NotifyMessage {
    pub fn info(title: impl Into<String>) -> Self {
        Self {
            severity: Severity::Info,
            source: None,
            title: title.into(),
            body: None,
            link: None,
        }
    }
    pub fn warn(title: impl Into<String>) -> Self {
        Self {
            severity: Severity::Warn,
            source: None,
            title: title.into(),
            body: None,
            link: None,
        }
    }
    pub fn crit(title: impl Into<String>) -> Self {
        Self {
            severity: Severity::Crit,
            source: None,
            title: title.into(),
            body: None,
            link: None,
        }
    }
    pub fn source(mut self, source: impl Into<String>) -> Self {
        self.source = Some(source.into());
        self
    }
    pub fn body(mut self, b: impl Into<String>) -> Self {
        self.body = Some(b.into());
        self
    }
    pub fn link(mut self, l: impl Into<String>) -> Self {
        self.link = Some(l.into());
        self
    }
}

/// Trait object stored in `AppState` so modules can dispatch without depending on `ep-notify`.
#[async_trait::async_trait]
pub trait NotifyBusTrait: Send + Sync + 'static {
    async fn dispatch(&self, msg: NotifyMessage) -> anyhow::Result<i64>;
    fn subscribe(&self) -> tokio::sync::broadcast::Receiver<NotifyEvent>;
}

pub type NotifyBusHandle = Arc<dyn NotifyBusTrait>;
