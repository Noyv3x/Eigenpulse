use crate::Notifier;
use async_trait::async_trait;
use ep_core::NotifyMessage;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize)]
pub struct TelegramConfig {
    pub bot_token: String,
    pub chat_id: String, // can be negative for groups
}

#[derive(Debug, Serialize)]
struct TgBody<'a> {
    chat_id: &'a str,
    text: String,
    parse_mode: &'static str,
    disable_web_page_preview: bool,
}

pub struct TelegramNotifier { cfg: TelegramConfig }

impl TelegramNotifier {
    pub fn from_value(v: serde_json::Value) -> anyhow::Result<Self> {
        Ok(Self { cfg: serde_json::from_value(v)? })
    }
}

#[async_trait]
impl Notifier for TelegramNotifier {
    fn kind(&self) -> &'static str { "telegram" }
    async fn send(&self, msg: &NotifyMessage) -> anyhow::Result<()> {
        let url = format!("https://api.telegram.org/bot{}/sendMessage", self.cfg.bot_token);
        let mut text = format!("*{}*", md_escape(&msg.title));
        if let Some(b) = &msg.body { text.push_str(&format!("\n{}", md_escape(b))); }
        if let Some(d) = &msg.doc_ref { text.push_str(&format!("\n`{}`", md_escape(d))); }
        if let Some(l) = &msg.link { text.push_str(&format!("\n[Open]({l})")); }
        let body = TgBody {
            chat_id: &self.cfg.chat_id,
            text,
            parse_mode: "Markdown",
            disable_web_page_preview: true,
        };
        let resp = crate::http_client().post(&url).json(&body).send().await?;
        if !resp.status().is_success() {
            anyhow::bail!("telegram status {}: {}", resp.status(), resp.text().await.unwrap_or_default());
        }
        Ok(())
    }
}

fn md_escape(s: &str) -> String {
    s.replace('_', "\\_").replace('*', "\\*").replace('[', "\\[").replace('`', "\\`")
}
