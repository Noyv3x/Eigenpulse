use crate::Notifier;
use async_trait::async_trait;
use ep_core::{html_escape, NotifyMessage};
use lettre::message::header::ContentType;
use lettre::transport::smtp::authentication::Credentials;
use lettre::{AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor};
use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct SmtpConfig {
    pub host: String,
    #[serde(default = "default_port")]
    pub port: u16,
    pub username: String,
    pub password: String,
    pub from: String,
    pub to: String,
    #[serde(default = "default_starttls")]
    pub starttls: bool,
}
fn default_port() -> u16 {
    587
}
fn default_starttls() -> bool {
    true
}

pub struct SmtpNotifier {
    cfg: SmtpConfig,
}

impl SmtpNotifier {
    pub fn from_value(v: serde_json::Value) -> anyhow::Result<Self> {
        let mut cfg: SmtpConfig = serde_json::from_value(v)?;
        cfg.host = cfg.host.trim().to_string();
        cfg.username = cfg.username.trim().to_string();
        cfg.from = cfg.from.trim().to_string();
        cfg.to = cfg.to.trim().to_string();
        for (field, value) in [
            ("host", cfg.host.as_str()),
            ("username", cfg.username.as_str()),
            ("password", cfg.password.as_str()),
            ("from", cfg.from.as_str()),
            ("to", cfg.to.as_str()),
        ] {
            if value.trim().is_empty() {
                anyhow::bail!("smtp config `{field}` is required");
            }
        }
        cfg.from
            .parse::<lettre::message::Mailbox>()
            .map_err(|e| anyhow::anyhow!("smtp config `from` is invalid: {e}"))?;
        cfg.to
            .parse::<lettre::message::Mailbox>()
            .map_err(|e| anyhow::anyhow!("smtp config `to` is invalid: {e}"))?;
        Ok(Self { cfg })
    }
}

#[async_trait]
impl Notifier for SmtpNotifier {
    fn kind(&self) -> &'static str {
        "smtp"
    }
    async fn send(&self, msg: &NotifyMessage) -> anyhow::Result<()> {
        let subject = format!(
            "[Eigenpulse · {}] {}",
            msg.severity.as_str().to_uppercase(),
            msg.title
        );
        let mut html = format!(
            "<h2 style=\"font-family:sans-serif\">{}</h2>",
            html_escape(&msg.title)
        );
        if let Some(b) = &msg.body {
            html.push_str(&format!(
                "<p style=\"font-family:sans-serif;color:#4b5563\">{}</p>",
                html_escape(b)
            ));
        }
        if let Some(d) = &msg.doc_ref {
            html.push_str(&format!(
                "<p style=\"font-family:monospace;color:#9ca3af\">ref · {}</p>",
                html_escape(d)
            ));
        }
        let email = Message::builder()
            .from(self.cfg.from.parse()?)
            .to(self.cfg.to.parse()?)
            .subject(subject)
            .header(ContentType::TEXT_HTML)
            .body(html)?;

        let creds = Credentials::new(self.cfg.username.clone(), self.cfg.password.clone());
        let mailer: AsyncSmtpTransport<Tokio1Executor> = if self.cfg.starttls {
            AsyncSmtpTransport::<Tokio1Executor>::starttls_relay(&self.cfg.host)?
                .port(self.cfg.port)
                .credentials(creds)
                .build()
        } else {
            AsyncSmtpTransport::<Tokio1Executor>::relay(&self.cfg.host)?
                .port(self.cfg.port)
                .credentials(creds)
                .build()
        };
        mailer.send(email).await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_value_trims_connection_and_address_fields() {
        let notifier = SmtpNotifier::from_value(serde_json::json!({
            "host": " smtp.example.com ",
            "port": 587,
            "username": " user ",
            "password": " pass with boundary spaces ",
            "from": " ops@example.com ",
            "to": " owner@example.com ",
            "starttls": true
        }))
        .expect("valid smtp config");

        assert_eq!(notifier.cfg.host, "smtp.example.com");
        assert_eq!(notifier.cfg.username, "user");
        assert_eq!(notifier.cfg.password, " pass with boundary spaces ");
        assert_eq!(notifier.cfg.from, "ops@example.com");
        assert_eq!(notifier.cfg.to, "owner@example.com");
    }
}
