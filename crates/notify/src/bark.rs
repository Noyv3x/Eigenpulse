use crate::bus::Notifier;
use async_trait::async_trait;
use ep_core::NotifyMessage;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize)]
pub struct BarkConfig {
    /// e.g. "https://api.day.app" or self-hosted base URL.
    pub base_url: String,
    pub device_key: String,
    #[serde(default)]
    pub sound: Option<String>,
    #[serde(default)]
    pub group: Option<String>,
    #[serde(default)]
    pub icon_url: Option<String>,
}

#[derive(Debug, Serialize)]
struct BarkBody<'a> {
    title: &'a str,
    body: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    group: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    sound: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    icon: Option<&'a str>,
}

pub struct BarkNotifier {
    cfg: BarkConfig,
}

impl BarkNotifier {
    pub fn from_value(v: serde_json::Value) -> anyhow::Result<Self> {
        fn require_non_empty(value: &str, field: &str) -> anyhow::Result<()> {
            if value.trim().is_empty() {
                anyhow::bail!("bark config `{field}` is required");
            }
            Ok(())
        }

        Ok(Self {
            cfg: {
                let mut cfg: BarkConfig = serde_json::from_value(v)?;
                cfg.base_url = cfg.base_url.trim().to_string();
                cfg.device_key = cfg.device_key.trim().to_string();
                cfg.sound = normalize_optional_text(cfg.sound);
                cfg.group = normalize_optional_text(cfg.group);
                cfg.icon_url = normalize_optional_text(cfg.icon_url);
                require_non_empty(&cfg.base_url, "base_url")?;
                require_non_empty(&cfg.device_key, "device_key")?;
                let base_url = reqwest::Url::parse(&cfg.base_url)
                    .map_err(|e| anyhow::anyhow!("bark config `base_url` is invalid: {e}"))?;
                if !matches!(base_url.scheme(), "http" | "https") {
                    anyhow::bail!("bark config `base_url` must use http or https");
                }
                if let Some(icon_url) = cfg.icon_url.as_deref() {
                    let icon_url = reqwest::Url::parse(icon_url)
                        .map_err(|e| anyhow::anyhow!("bark config `icon_url` is invalid: {e}"))?;
                    if !matches!(icon_url.scheme(), "http" | "https") {
                        anyhow::bail!("bark config `icon_url` must use http or https");
                    }
                }
                cfg
            },
        })
    }
}

fn normalize_optional_text(value: Option<String>) -> Option<String> {
    value
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

#[async_trait]
impl Notifier for BarkNotifier {
    async fn send(&self, msg: &NotifyMessage) -> anyhow::Result<()> {
        let url = format!(
            "{}/{}",
            self.cfg.base_url.trim_end_matches('/'),
            self.cfg.device_key
        );
        let body = BarkBody {
            title: &msg.title,
            body: msg.body.as_deref().unwrap_or(""),
            url: msg.link.clone(),
            group: self.cfg.group.as_deref(),
            sound: self.cfg.sound.as_deref(),
            icon: self.cfg.icon_url.as_deref(),
        };
        let resp = crate::http_client()
            .post(&url)
            .json(&body)
            .send()
            .await
            .map_err(|e| anyhow::anyhow!("bark request failed: {}", e.without_url()))?;
        if !resp.status().is_success() {
            let status = resp.status();
            let body = crate::capped_response_text(resp).await;
            anyhow::bail!("bark status {}: {}", status, body);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_value_trims_urls_and_optional_fields() {
        let notifier = BarkNotifier::from_value(serde_json::json!({
            "base_url": " https://api.day.app/ ",
            "device_key": " DEVICE ",
            "sound": " bell ",
            "group": " Eigenpulse ",
            "icon_url": " https://example.com/icon.png "
        }))
        .expect("valid bark config");

        assert_eq!(notifier.cfg.base_url, "https://api.day.app/");
        assert_eq!(notifier.cfg.device_key, "DEVICE");
        assert_eq!(notifier.cfg.sound.as_deref(), Some("bell"));
        assert_eq!(notifier.cfg.group.as_deref(), Some("Eigenpulse"));
        assert_eq!(
            notifier.cfg.icon_url.as_deref(),
            Some("https://example.com/icon.png")
        );
    }

    #[test]
    fn from_value_drops_blank_optional_fields() {
        let notifier = BarkNotifier::from_value(serde_json::json!({
            "base_url": "https://api.day.app",
            "device_key": "DEVICE",
            "sound": " ",
            "group": ""
        }))
        .expect("valid bark config");

        assert_eq!(notifier.cfg.sound, None);
        assert_eq!(notifier.cfg.group, None);
    }

    /// Drive `send()` against a loopback recorder (no real network) and assert
    /// the request reqwest actually built: POST, device key in the URL path,
    /// and the JSON body carrying title/body/url/group/sound. This is the only
    /// coverage of the request-construction path; everything else stubs it.
    #[tokio::test]
    async fn send_posts_expected_url_and_json_body() {
        let server = crate::test_server::RecordingServer::start("200 OK", r#"{"code":200}"#).await;

        let notifier = BarkNotifier::from_value(serde_json::json!({
            "base_url": server.base_url,
            "device_key": "DEVICE_SECRET_KEY",
            "group": "Eigenpulse",
            "sound": "bell"
        }))
        .expect("valid bark config");

        let msg = NotifyMessage::info("Budget breached")
            .body("F&B over budget")
            .link("/finance?month=2026-05");
        notifier
            .send(&msg)
            .await
            .expect("send succeeds against 200");

        let req = server.captured().await;
        assert_eq!(req.method, "POST");
        // Bark embeds the device key in the URL path: `{base}/{device_key}`.
        assert_eq!(req.path, "/DEVICE_SECRET_KEY");

        let body: serde_json::Value = serde_json::from_str(&req.body).expect("json body");
        assert_eq!(body["title"], "Budget breached");
        assert_eq!(body["body"], "F&B over budget");
        assert_eq!(body["url"], "/finance?month=2026-05");
        assert_eq!(body["group"], "Eigenpulse");
        assert_eq!(body["sound"], "bell");
        // `icon` was never configured → omitted by `skip_serializing_if`.
        assert!(body.get("icon").is_none(), "unset icon must not serialize");
    }

    /// On a non-2xx response the error bubbled out of `send()` must not contain
    /// the device key (it lives in the request URL). `e.without_url()` strips
    /// transport-error URLs; the explicit status-error path here builds its own
    /// message from status + response text, so we assert the key never appears.
    #[tokio::test]
    async fn send_error_response_does_not_leak_device_key() {
        let server = crate::test_server::RecordingServer::start(
            "401 Unauthorized",
            r#"{"message":"bad device key"}"#,
        )
        .await;

        let notifier = BarkNotifier::from_value(serde_json::json!({
            "base_url": server.base_url,
            "device_key": "DEVICE_SECRET_KEY",
        }))
        .expect("valid bark config");

        let err = notifier
            .send(&NotifyMessage::info("probe"))
            .await
            .expect_err("non-2xx must error");
        let rendered = format!("{err:#}");
        assert!(
            !rendered.contains("DEVICE_SECRET_KEY"),
            "device key leaked into error: {rendered}"
        );
        // It should still surface the status so operators see *why* it failed.
        assert!(
            rendered.contains("401"),
            "error should name the status: {rendered}"
        );

        // Drain the recorder so its task isn't left dangling.
        let _ = server.captured().await;
    }
}
