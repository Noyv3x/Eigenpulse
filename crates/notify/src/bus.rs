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
        Self {
            broadcaster: tx,
            db,
        }
    }
}

#[async_trait]
impl NotifyBusTrait for NotifyBus {
    async fn dispatch(&self, msg: NotifyMessage) -> anyhow::Result<i64> {
        // 1) Persist into `notification`.
        let id: i64 = sqlx::query_scalar(
            "INSERT INTO notification (severity, module, title, body, link, doc_ref)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6) RETURNING id",
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
        let channels: Vec<(i64, String, String, String)> = match sqlx::query_as(
            "SELECT id, kind, config_json, min_severity
               FROM notify_channel
              WHERE enabled = 1",
        )
        .fetch_all(&self.db)
        .await
        {
            Ok(channels) => channels,
            Err(e) => {
                tracing::warn!(error = %e, "failed to load notification channels");
                Vec::new()
            }
        };

        for (ch_id, kind, cfg, min_sev) in channels {
            let Some(min) = Severity::try_parse(&min_sev) else {
                tracing::warn!(
                    channel_id = ch_id,
                    kind = %kind,
                    min_severity = %min_sev,
                    "notification channel has invalid min_severity; skipping"
                );
                continue;
            };
            if !msg.severity.passes(min) {
                continue;
            }
            let result: anyhow::Result<()> = match build_notifier(&kind, &cfg) {
                Ok(n) => n.send(&msg).await,
                Err(e) => Err(e),
            };
            let ok = result.is_ok();
            let err = result.err().map(|e| {
                tracing::warn!(
                    channel_id = ch_id,
                    kind = %kind,
                    error = %e,
                    "notification channel delivery failed"
                );
                safe_delivery_error(&kind)
            });
            if let Err(e) = sqlx::query(
                "INSERT INTO notify_delivery (notification_id, channel_id, ok, error)
                 VALUES (?1, ?2, ?3, ?4)",
            )
            .bind(id)
            .bind(ch_id)
            .bind(ok as i64)
            .bind(err)
            .execute(&self.db)
            .await
            {
                tracing::warn!(
                    notification_id = id,
                    channel_id = ch_id,
                    error = %e,
                    "failed to record notification delivery"
                );
            }
        }
        Ok(id)
    }

    fn subscribe(&self) -> broadcast::Receiver<NotifyMessage> {
        self.broadcaster.subscribe()
    }
}

fn safe_delivery_error(kind: &str) -> String {
    let kind = match kind {
        "inapp" | "smtp" | "bark" | "telegram" | "discord" => kind,
        _ => "unknown",
    };
    format!("{kind} 通道投递失败 · 详细错误已记录")
}

pub fn build_notifier(kind: &str, config_json: &str) -> anyhow::Result<Box<dyn Notifier>> {
    let v: serde_json::Value =
        serde_json::from_str(config_json).map_err(|e| anyhow::anyhow!("bad config json: {e}"))?;
    Ok(match kind {
        "inapp" => Box::new(crate::inapp::InappNotifier),
        "smtp" => Box::new(crate::smtp::SmtpNotifier::from_value(v)?),
        "bark" => Box::new(crate::bark::BarkNotifier::from_value(v)?),
        "telegram" => Box::new(crate::telegram::TelegramNotifier::from_value(v)?),
        "discord" => Box::new(crate::discord::DiscordNotifier::from_value(v)?),
        other => anyhow::bail!("unknown notifier kind: {other}"),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::sqlite::SqlitePoolOptions;

    async fn test_pool() -> SqlitePool {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .expect("open in-memory sqlite");

        sqlx::query(
            "CREATE TABLE notification (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                severity TEXT NOT NULL,
                module TEXT,
                title TEXT NOT NULL,
                body TEXT,
                link TEXT,
                doc_ref TEXT
            )",
        )
        .execute(&pool)
        .await
        .expect("create notification table");
        sqlx::query(
            "CREATE TABLE notify_channel (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                kind TEXT NOT NULL,
                name TEXT NOT NULL,
                enabled INTEGER NOT NULL DEFAULT 1,
                config_json TEXT NOT NULL,
                min_severity TEXT NOT NULL DEFAULT 'info'
            )",
        )
        .execute(&pool)
        .await
        .expect("create notify_channel table");
        sqlx::query(
            "CREATE TABLE notify_delivery (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                notification_id INTEGER NOT NULL,
                channel_id INTEGER NOT NULL,
                ok INTEGER NOT NULL,
                error TEXT
            )",
        )
        .execute(&pool)
        .await
        .expect("create notify_delivery table");

        pool
    }

    #[tokio::test]
    async fn dispatch_records_sanitized_delivery_errors() {
        let pool = test_pool().await;
        sqlx::query(
            "INSERT INTO notify_channel (kind, name, config_json, min_severity)
             VALUES ('telegram', 'tg', ?1, 'info')",
        )
        .bind(r#"{"bot_token":"SECRET_TOKEN"}"#)
        .execute(&pool)
        .await
        .expect("insert channel");

        let bus = NotifyBus::new(pool.clone());
        let id = bus
            .dispatch(NotifyMessage::info("probe"))
            .await
            .expect("dispatch notification");

        let row: (i64, String) =
            sqlx::query_as("SELECT ok, error FROM notify_delivery WHERE notification_id = ?1")
                .bind(id)
                .fetch_one(&pool)
                .await
                .expect("delivery row");

        assert_eq!(row.0, 0);
        assert_eq!(row.1, "telegram 通道投递失败 · 详细错误已记录");
        assert!(!row.1.contains("SECRET_TOKEN"));
    }

    #[tokio::test]
    async fn dispatch_skips_channels_with_invalid_min_severity() {
        let pool = test_pool().await;
        sqlx::query(
            "INSERT INTO notify_channel (kind, name, config_json, min_severity)
             VALUES ('inapp', 'bad severity', '{}', 'urgent')",
        )
        .execute(&pool)
        .await
        .expect("insert channel");

        let bus = NotifyBus::new(pool.clone());
        let id = bus
            .dispatch(NotifyMessage::crit("probe"))
            .await
            .expect("dispatch notification");

        let delivery_count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM notify_delivery WHERE notification_id = ?1")
                .bind(id)
                .fetch_one(&pool)
                .await
                .expect("count deliveries");

        assert_eq!(delivery_count, 0);
    }

    #[test]
    fn safe_delivery_error_does_not_echo_unknown_kind() {
        let err = safe_delivery_error("custom-webhook-with-secret-token");
        assert_eq!(err, "unknown 通道投递失败 · 详细错误已记录");
    }
}
