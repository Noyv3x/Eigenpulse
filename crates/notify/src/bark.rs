use crate::Notifier;
use async_trait::async_trait;
use ep_core::NotifyMessage;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize)]
pub struct BarkConfig {
    /// e.g. "https://api.day.app" or self-hosted base URL.
    pub base_url: String,
    pub device_key: String,
    #[serde(default)] pub sound: Option<String>,
    #[serde(default)] pub group: Option<String>,
    #[serde(default)] pub icon_url: Option<String>,
}

#[derive(Debug, Serialize)]
struct BarkBody<'a> {
    title: &'a str,
    body: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")] url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")] group: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")] sound: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")] icon: Option<&'a str>,
}

pub struct BarkNotifier { cfg: BarkConfig }

impl BarkNotifier {
    pub fn from_value(v: serde_json::Value) -> anyhow::Result<Self> {
        Ok(Self { cfg: serde_json::from_value(v)? })
    }
}

#[async_trait]
impl Notifier for BarkNotifier {
    fn kind(&self) -> &'static str { "bark" }
    async fn send(&self, msg: &NotifyMessage) -> anyhow::Result<()> {
        let url = format!("{}/{}", self.cfg.base_url.trim_end_matches('/'), self.cfg.device_key);
        let body = BarkBody {
            title: &msg.title,
            body: msg.body.as_deref().unwrap_or(""),
            url: msg.link.clone(),
            group: self.cfg.group.as_deref(),
            sound: self.cfg.sound.as_deref(),
            icon: self.cfg.icon_url.as_deref(),
        };
        let resp = crate::http_client().post(&url).json(&body).send().await?;
        if !resp.status().is_success() {
            anyhow::bail!("bark status {}: {}", resp.status(), resp.text().await.unwrap_or_default());
        }
        Ok(())
    }
}
