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
        let msg = normalize_message(msg);

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

        // 3) Fan-out to enabled external channels OFF the request path. A slow
        //    or unreachable SMTP/webhook endpoint must not block the caller
        //    (finance txn creation, open-API). The notification row is already
        //    persisted and broadcast above, which determines the return value;
        //    external delivery is fire-and-forget and runs concurrently inside
        //    the spawned task (total latency = max(channel), not sum(channel)).
        let pool = self.db.clone();
        tokio::spawn(async move {
            deliver_external(pool, id, msg).await;
        });

        Ok(id)
    }

    fn subscribe(&self) -> broadcast::Receiver<NotifyMessage> {
        self.broadcaster.subscribe()
    }
}

/// Fan-out delivery to all enabled external notification channels, run
/// concurrently. Per-channel failures are logged, sanitized via
/// [`safe_delivery_error`], recorded in `notify_delivery`, and never abort
/// sibling channels. Channels whose severity is below their `min_severity`
/// (or whose `min_severity` is unparseable) are skipped.
///
/// `dispatch` spawns this as a detached task; tests call it directly for
/// deterministic assertions.
pub(crate) async fn deliver_external(pool: SqlitePool, notification_id: i64, msg: NotifyMessage) {
    let channels: Vec<(i64, String, String, String)> = match sqlx::query_as(
        "SELECT id, kind, config_json, min_severity
           FROM notify_channel
          WHERE enabled = 1",
    )
    .fetch_all(&pool)
    .await
    {
        Ok(channels) => channels,
        Err(e) => {
            tracing::warn!(error = %e, "failed to load notification channels");
            return;
        }
    };

    let mut tasks: tokio::task::JoinSet<()> = tokio::task::JoinSet::new();
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
        let pool = pool.clone();
        let msg = msg.clone();
        tasks.spawn(async move {
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
            .bind(notification_id)
            .bind(ch_id)
            .bind(ok as i64)
            .bind(err)
            .execute(&pool)
            .await
            {
                tracing::warn!(
                    notification_id = notification_id,
                    channel_id = ch_id,
                    error = %e,
                    "failed to record notification delivery"
                );
            }
        });
    }

    // Drain the set so every channel's delivery row is written before the
    // task ends. Individual channel panics are logged, not propagated.
    while let Some(joined) = tasks.join_next().await {
        if let Err(e) = joined {
            tracing::warn!(
                notification_id = notification_id,
                error = %e,
                "notification delivery task failed to join"
            );
        }
    }
}

fn normalize_message(mut msg: NotifyMessage) -> NotifyMessage {
    msg.link = msg
        .link
        .as_deref()
        .and_then(ep_core::safe_in_app_path)
        .map(str::to_string);
    msg.doc_ref = msg
        .doc_ref
        .as_deref()
        .and_then(ep_core::safe_doc_id)
        .map(str::to_string);
    msg
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

    // External delivery is fire-and-forget in `dispatch` (spawned), so the
    // delivery-side assertions call `deliver_external` directly to stay
    // deterministic. `dispatch_*` tests below cover the synchronous
    // persist+broadcast path and the stable return value.

    #[tokio::test]
    async fn deliver_external_records_sanitized_delivery_errors() {
        let pool = test_pool().await;
        sqlx::query(
            "INSERT INTO notify_channel (kind, name, config_json, min_severity)
             VALUES ('telegram', 'tg', ?1, 'info')",
        )
        .bind(r#"{"bot_token":"SECRET_TOKEN"}"#)
        .execute(&pool)
        .await
        .expect("insert channel");

        deliver_external(pool.clone(), 1, NotifyMessage::info("probe")).await;

        let row: (i64, String) =
            sqlx::query_as("SELECT ok, error FROM notify_delivery WHERE notification_id = ?1")
                .bind(1_i64)
                .fetch_one(&pool)
                .await
                .expect("delivery row");

        assert_eq!(row.0, 0);
        assert_eq!(row.1, "telegram 通道投递失败 · 详细错误已记录");
        assert!(!row.1.contains("SECRET_TOKEN"));
    }

    /// End-to-end through `deliver_external` against a loopback bark endpoint
    /// that returns 401 (no real network). The notifier's URL carries the
    /// device key in its path; this proves that even when the request actually
    /// reaches an endpoint and fails, the row written to `notify_delivery.error`
    /// is the sanitized generic string — never the device key or the URL.
    #[tokio::test]
    async fn deliver_external_records_sanitized_error_for_reachable_failing_endpoint() {
        let server = crate::test_server::RecordingServer::start(
            "401 Unauthorized",
            r#"{"message":"unauthorized"}"#,
        )
        .await;

        let pool = test_pool().await;
        let cfg = serde_json::json!({
            "base_url": server.base_url,
            "device_key": "DEVICE_SECRET_KEY",
        })
        .to_string();
        sqlx::query(
            "INSERT INTO notify_channel (kind, name, config_json, min_severity)
             VALUES ('bark', 'phone', ?1, 'info')",
        )
        .bind(&cfg)
        .execute(&pool)
        .await
        .expect("insert channel");

        deliver_external(pool.clone(), 1, NotifyMessage::info("probe")).await;

        let row: (i64, String) =
            sqlx::query_as("SELECT ok, error FROM notify_delivery WHERE notification_id = ?1")
                .bind(1_i64)
                .fetch_one(&pool)
                .await
                .expect("delivery row");
        assert_eq!(row.0, 0, "401 endpoint marks delivery failed");
        assert_eq!(row.1, "bark 通道投递失败 · 详细错误已记录");
        assert!(
            !row.1.contains("DEVICE_SECRET_KEY"),
            "device key must never reach notify_delivery.error: {}",
            row.1
        );
        assert!(
            !row.1.contains("127.0.0.1"),
            "request URL must never reach notify_delivery.error: {}",
            row.1
        );

        // The endpoint really was hit with the device key in the path.
        let req = server.captured().await;
        assert_eq!(req.method, "POST");
        assert_eq!(req.path, "/DEVICE_SECRET_KEY");
    }

    #[tokio::test]
    async fn deliver_external_skips_channels_with_invalid_min_severity() {
        let pool = test_pool().await;
        sqlx::query(
            "INSERT INTO notify_channel (kind, name, config_json, min_severity)
             VALUES ('inapp', 'bad severity', '{}', 'urgent')",
        )
        .execute(&pool)
        .await
        .expect("insert channel");

        deliver_external(pool.clone(), 1, NotifyMessage::crit("probe")).await;

        let delivery_count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM notify_delivery WHERE notification_id = ?1")
                .bind(1_i64)
                .fetch_one(&pool)
                .await
                .expect("count deliveries");

        assert_eq!(delivery_count, 0);
    }

    #[tokio::test]
    async fn dispatch_persists_and_returns_id_without_awaiting_delivery() {
        let pool = test_pool().await;
        let bus = NotifyBus::new(pool.clone());
        let mut rx = bus.subscribe();

        let id = bus
            .dispatch(NotifyMessage::info("probe"))
            .await
            .expect("dispatch notification");

        // Row is persisted synchronously before dispatch returns.
        let stored: String = sqlx::query_scalar("SELECT title FROM notification WHERE id = ?1")
            .bind(id)
            .fetch_one(&pool)
            .await
            .expect("notification row");
        assert_eq!(stored, "probe");

        // Broadcast happened synchronously too.
        let broadcast = rx.try_recv().expect("broadcast message");
        assert_eq!(broadcast.title, "probe");
    }

    #[tokio::test]
    async fn dispatch_sanitizes_message_link_and_doc_ref() {
        let pool = test_pool().await;
        let bus = NotifyBus::new(pool.clone());

        let id = bus
            .dispatch(
                NotifyMessage::warn("unsafe")
                    .link(" https://evil.example/path ")
                    .doc_ref(" FIN-26092<script> "),
            )
            .await
            .expect("dispatch notification");

        let stored: (Option<String>, Option<String>) =
            sqlx::query_as("SELECT link, doc_ref FROM notification WHERE id = ?1")
                .bind(id)
                .fetch_one(&pool)
                .await
                .expect("notification row");

        assert_eq!(stored, (None, None));
    }

    #[tokio::test]
    async fn dispatch_trims_safe_message_link_and_doc_ref() {
        let pool = test_pool().await;
        let bus = NotifyBus::new(pool.clone());

        let id = bus
            .dispatch(
                NotifyMessage::info("safe")
                    .link(" /finance?month=2026-05 ")
                    .doc_ref(" FIN-26092 "),
            )
            .await
            .expect("dispatch notification");

        let stored: (Option<String>, Option<String>) =
            sqlx::query_as("SELECT link, doc_ref FROM notification WHERE id = ?1")
                .bind(id)
                .fetch_one(&pool)
                .await
                .expect("notification row");

        assert_eq!(
            stored,
            (
                Some("/finance?month=2026-05".into()),
                Some("FIN-26092".into())
            )
        );
    }

    #[test]
    fn safe_delivery_error_does_not_echo_unknown_kind() {
        let err = safe_delivery_error("custom-webhook-with-secret-token");
        assert_eq!(err, "unknown 通道投递失败 · 详细错误已记录");
    }
}
