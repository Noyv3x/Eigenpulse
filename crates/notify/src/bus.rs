use async_trait::async_trait;
use ep_core::{NotifyBusTrait, NotifyMessage, Severity};
use sqlx::SqlitePool;
use tokio::sync::broadcast;

#[async_trait]
pub trait Notifier: Send + Sync + 'static {
    fn kind(&self) -> &'static str;
    async fn send(&self, msg: &NotifyMessage) -> anyhow::Result<()>;
}

pub struct NotifyBus {
    pub broadcaster: broadcast::Sender<NotifyMessage>,
    pub db: SqlitePool,
}

impl NotifyBus {
    pub fn new(db: SqlitePool) -> Self {
        let (tx, _rx) = broadcast::channel(256);
        Self { broadcaster: tx, db }
    }
}

#[async_trait]
impl NotifyBusTrait for NotifyBus {
    async fn dispatch(&self, msg: NotifyMessage) -> anyhow::Result<i64> {
        // 1) Persist into `notification`.
        let id: i64 = sqlx::query_scalar(
            "INSERT INTO notification (severity, module, title, body, link, doc_ref)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6) RETURNING id"
        )
        .bind(msg.severity.as_str())
        .bind(msg.module.as_deref())
        .bind(&msg.title)
        .bind(msg.body.as_deref())
        .bind(msg.link.as_deref())
        .bind(msg.doc_ref.as_deref())
        .fetch_one(&self.db)
        .await?;

        // 2) Realtime broadcast to SSE subscribers.
        let _ = self.broadcaster.send(msg.clone());

        // 3) Fan-out to enabled channels.
        let channels: Vec<(i64, String, String, String)> = sqlx::query_as(
            "SELECT id, kind, config_json, min_severity
               FROM notify_channel
              WHERE enabled = 1"
        )
        .fetch_all(&self.db)
        .await
        .unwrap_or_default();

        for (ch_id, kind, cfg, min_sev) in channels {
            let min = Severity::parse(&min_sev);
            if !msg.severity.passes(min) { continue; }
            let result: anyhow::Result<()> = match build_notifier(&kind, &cfg) {
                Ok(n) => n.send(&msg).await,
                Err(e) => Err(e),
            };
            let ok = result.is_ok();
            let err = result.err().map(|e| e.to_string());
            let _ = sqlx::query(
                "INSERT INTO notify_delivery (notification_id, channel_id, ok, error)
                 VALUES (?1, ?2, ?3, ?4)"
            )
            .bind(id).bind(ch_id).bind(ok as i64).bind(err)
            .execute(&self.db)
            .await;
        }
        Ok(id)
    }

    fn subscribe(&self) -> broadcast::Receiver<NotifyMessage> {
        self.broadcaster.subscribe()
    }
}

pub fn build_notifier(kind: &str, config_json: &str) -> anyhow::Result<Box<dyn Notifier>> {
    let v: serde_json::Value = serde_json::from_str(config_json)
        .map_err(|e| anyhow::anyhow!("bad config json: {e}"))?;
    Ok(match kind {
        "inapp" => Box::new(crate::inapp::InappNotifier),
        "smtp" => Box::new(crate::smtp::SmtpNotifier::from_value(v)?),
        "bark" => Box::new(crate::bark::BarkNotifier::from_value(v)?),
        "telegram" => Box::new(crate::telegram::TelegramNotifier::from_value(v)?),
        "discord" => Box::new(crate::discord::DiscordNotifier::from_value(v)?),
        other => anyhow::bail!("unknown notifier kind: {other}"),
    })
}
