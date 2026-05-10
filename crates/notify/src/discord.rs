use crate::Notifier;
use async_trait::async_trait;
use ep_core::{NotifyMessage, Severity};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize)]
pub struct DiscordConfig {
    pub webhook_url: String,
}

#[derive(Debug, Serialize)]
struct Embed {
    title: String,
    description: Option<String>,
    color: u32,
    url: Option<String>,
    fields: Vec<EmbedField>,
}
#[derive(Debug, Serialize)]
struct EmbedField {
    name: String,
    value: String,
    inline: bool,
}

#[derive(Debug, Serialize)]
struct WebhookBody {
    username: &'static str,
    embeds: Vec<Embed>,
}

pub struct DiscordNotifier {
    cfg: DiscordConfig,
}

impl DiscordNotifier {
    pub fn from_value(v: serde_json::Value) -> anyhow::Result<Self> {
        let mut cfg: DiscordConfig = serde_json::from_value(v)?;
        cfg.webhook_url = cfg.webhook_url.trim().to_string();
        if cfg.webhook_url.trim().is_empty() {
            anyhow::bail!("discord config `webhook_url` is required");
        }
        let webhook_url = reqwest::Url::parse(&cfg.webhook_url)
            .map_err(|e| anyhow::anyhow!("discord config `webhook_url` is invalid: {e}"))?;
        if !matches!(webhook_url.scheme(), "http" | "https") {
            anyhow::bail!("discord config `webhook_url` must use http or https");
        }
        Ok(Self { cfg })
    }
}

fn color_for(sev: Severity) -> u32 {
    match sev {
        Severity::Info => 0x5cb88a,
        Severity::Warn => 0xc88a4a,
        Severity::Crit => 0xc0506b,
    }
}

#[async_trait]
impl Notifier for DiscordNotifier {
    fn kind(&self) -> &'static str {
        "discord"
    }
    async fn send(&self, msg: &NotifyMessage) -> anyhow::Result<()> {
        let mut fields = Vec::new();
        if let Some(d) = &msg.doc_ref {
            fields.push(EmbedField {
                name: "Doc".into(),
                value: format!("`{d}`"),
                inline: true,
            });
        }
        if let Some(m) = &msg.module {
            fields.push(EmbedField {
                name: "Module".into(),
                value: m.clone(),
                inline: true,
            });
        }
        let embed = Embed {
            title: msg.title.clone(),
            description: msg.body.clone(),
            color: color_for(msg.severity),
            url: msg.link.clone(),
            fields,
        };
        let body = WebhookBody {
            username: "Eigenpulse",
            embeds: vec![embed],
        };
        let resp = crate::http_client()
            .post(&self.cfg.webhook_url)
            .json(&body)
            .send()
            .await?;
        if !resp.status().is_success() {
            anyhow::bail!(
                "discord status {}: {}",
                resp.status(),
                resp.text().await.unwrap_or_default()
            );
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_value_trims_webhook_url() {
        let notifier = DiscordNotifier::from_value(serde_json::json!({
            "webhook_url": " https://discord.com/api/webhooks/abc "
        }))
        .expect("valid discord config");

        assert_eq!(
            notifier.cfg.webhook_url,
            "https://discord.com/api/webhooks/abc"
        );
    }
}
