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

pub struct TelegramNotifier {
    cfg: TelegramConfig,
}

impl TelegramNotifier {
    pub fn from_value(v: serde_json::Value) -> anyhow::Result<Self> {
        fn require_non_empty(value: &str, field: &str) -> anyhow::Result<()> {
            if value.trim().is_empty() {
                anyhow::bail!("telegram config `{field}` is required");
            }
            Ok(())
        }

        let mut cfg: TelegramConfig = serde_json::from_value(v)?;
        cfg.bot_token = cfg.bot_token.trim().to_string();
        cfg.chat_id = cfg.chat_id.trim().to_string();
        require_non_empty(&cfg.bot_token, "bot_token")?;
        require_non_empty(&cfg.chat_id, "chat_id")?;
        Ok(Self { cfg })
    }
}

#[async_trait]
impl Notifier for TelegramNotifier {
    fn kind(&self) -> &'static str {
        "telegram"
    }
    async fn send(&self, msg: &NotifyMessage) -> anyhow::Result<()> {
        let url = format!(
            "https://api.telegram.org/bot{}/sendMessage",
            self.cfg.bot_token
        );
        let text = render_text(msg);
        let body = TgBody {
            chat_id: &self.cfg.chat_id,
            text,
            parse_mode: "Markdown",
            disable_web_page_preview: true,
        };
        let resp = crate::http_client().post(&url).json(&body).send().await?;
        if !resp.status().is_success() {
            anyhow::bail!(
                "telegram status {}: {}",
                resp.status(),
                resp.text().await.unwrap_or_default()
            );
        }
        Ok(())
    }
}

fn render_text(msg: &NotifyMessage) -> String {
    let mut text = format!("*{}*", md_escape(&msg.title));
    if let Some(b) = &msg.body {
        text.push_str(&format!("\n{}", md_escape(b)));
    }
    if let Some(d) = &msg.doc_ref {
        text.push_str(&format!("\n`{}`", md_escape(d)));
    }
    if let Some(l) = &msg.link {
        text.push_str(&format!("\nOpen: {}", md_escape(l)));
    }
    text
}

fn md_escape(s: &str) -> String {
    s.replace('_', "\\_")
        .replace('*', "\\*")
        .replace('[', "\\[")
        .replace(']', "\\]")
        .replace('(', "\\(")
        .replace(')', "\\)")
        .replace('`', "\\`")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_value_trims_boundary_whitespace() {
        let notifier = TelegramNotifier::from_value(serde_json::json!({
            "bot_token": " TOKEN ",
            "chat_id": " 123 "
        }))
        .expect("valid telegram config");

        assert_eq!(notifier.cfg.bot_token, "TOKEN");
        assert_eq!(notifier.cfg.chat_id, "123");
    }

    #[test]
    fn render_text_escapes_markdown_control_chars() {
        let msg = NotifyMessage::info("Budget_[Q2]")
            .body("Use *cash* `(safe)`")
            .doc_ref("FIN-26001")
            .link("/finance?next=(month)");

        let text = render_text(&msg);

        assert!(text.contains("*Budget\\_\\[Q2\\]*"));
        assert!(text.contains("Use \\*cash\\* \\`\\(safe\\)\\`"));
        assert!(text.contains("`FIN-26001`"));
        assert!(text.contains("Open: /finance?next=\\(month\\)"));
        assert!(!text.contains("[Open]("));
    }
}
