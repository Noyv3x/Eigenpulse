use async_trait::async_trait;
use ep_core::{NotifyBusTrait, NotifyEvent, NotifyMessage, Severity};
use sqlx::SqlitePool;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{broadcast, watch, Notify};

#[async_trait]
pub(crate) trait Notifier: Send + Sync + 'static {
    async fn send(&self, msg: &NotifyMessage) -> anyhow::Result<()>;
}

pub struct NotifyBus {
    pub broadcaster: broadcast::Sender<NotifyEvent>,
    pub db: SqlitePool,
    wake: Arc<Notify>,
}

impl NotifyBus {
    pub fn new(db: SqlitePool) -> Self {
        let (tx, _rx) = broadcast::channel(256);
        Self {
            broadcaster: tx,
            db,
            wake: Arc::new(Notify::new()),
        }
    }

    /// Start the durable delivery worker. Call this once, after all global
    /// migrations have completed, and retain the handle for graceful shutdown.
    pub fn start_worker(&self) -> NotifyWorkerHandle {
        let (shutdown_tx, shutdown_rx) = watch::channel(false);
        let join = tokio::spawn(run_worker(
            self.db.clone(),
            Arc::clone(&self.wake),
            shutdown_rx,
        ));
        self.wake.notify_one();
        NotifyWorkerHandle {
            shutdown: shutdown_tx,
            join,
        }
    }
}

pub struct NotifyWorkerHandle {
    shutdown: watch::Sender<bool>,
    join: tokio::task::JoinHandle<()>,
}

impl NotifyWorkerHandle {
    /// Stop claiming new work and allow the current bounded batch to finish.
    pub async fn shutdown(mut self) {
        let _ = self.shutdown.send(true);
        match tokio::time::timeout(Duration::from_secs(15), &mut self.join).await {
            Ok(Ok(())) => {}
            Ok(Err(error)) => tracing::warn!(%error, "notification worker failed during shutdown"),
            Err(_) => {
                tracing::warn!(
                    "notification worker drain timed out; pending leases will recover on restart"
                );
                self.join.abort();
            }
        }
    }
}

#[async_trait]
impl NotifyBusTrait for NotifyBus {
    async fn dispatch(&self, msg: NotifyMessage) -> anyhow::Result<i64> {
        let msg = normalize_message(msg);

        // Persist the in-app notification and every eligible external delivery
        // intent atomically. A process crash after commit cannot lose fan-out.
        let mut tx = self.db.begin().await?;
        let id: i64 = sqlx::query_scalar(
            "INSERT INTO notification (severity, source, title, body, link)
             VALUES (?1, ?2, ?3, ?4, ?5) RETURNING id",
        )
        .bind(msg.severity.as_str())
        .bind(msg.source.as_deref())
        .bind(&msg.title)
        .bind(msg.body.as_deref())
        .bind(msg.link.as_deref())
        .fetch_one(&mut *tx)
        .await?;

        let channels: Vec<(i64, String, String)> =
            sqlx::query_as("SELECT id, kind, min_severity FROM notify_channel WHERE enabled = 1")
                .fetch_all(&mut *tx)
                .await?;
        for (channel_id, kind, minimum) in channels {
            let Some(minimum) = Severity::try_parse(&minimum) else {
                tracing::warn!(channel_id, %kind, "notification channel has invalid min_severity; skipping");
                continue;
            };
            if !msg.severity.passes(minimum) {
                continue;
            }
            sqlx::query("INSERT INTO notify_outbox(notification_id, channel_id) VALUES (?1, ?2)")
                .bind(id)
                .bind(channel_id)
                .execute(&mut *tx)
                .await?;
        }
        tx.commit().await?;

        // Realtime state is emitted only after the durable commit is visible.
        let _ = self.broadcaster.send(NotifyEvent { id, message: msg });
        self.wake.notify_one();

        Ok(id)
    }

    fn subscribe(&self) -> broadcast::Receiver<NotifyEvent> {
        self.broadcaster.subscribe()
    }
}

const DELIVERY_CONCURRENCY: usize = 4;
const DELIVERY_LEASE_SECONDS: i64 = 90;
const MAX_DELIVERY_ATTEMPTS: i64 = 4;

async fn run_worker(pool: SqlitePool, wake: Arc<Notify>, mut shutdown: watch::Receiver<bool>) {
    let mut next_cleanup = tokio::time::Instant::now();
    loop {
        if *shutdown.borrow() {
            return;
        }

        if tokio::time::Instant::now() >= next_cleanup {
            if let Err(error) = cleanup_retained_rows(&pool).await {
                tracing::warn!(%error, "notification retention cleanup failed");
            }
            next_cleanup = tokio::time::Instant::now() + Duration::from_secs(24 * 60 * 60);
        }

        match claim_due(&pool).await {
            Ok(claimed) if !claimed.is_empty() => {
                let mut tasks = tokio::task::JoinSet::new();
                for (outbox_id, attempt_count) in claimed {
                    let pool = pool.clone();
                    tasks.spawn(async move {
                        deliver_claim(&pool, outbox_id, attempt_count).await;
                    });
                }
                while let Some(result) = tasks.join_next().await {
                    if let Err(error) = result {
                        tracing::warn!(%error, "notification delivery worker task panicked");
                    }
                }
                continue;
            }
            Ok(_) => {}
            Err(error) => tracing::warn!(%error, "failed to claim notification outbox work"),
        }

        tokio::select! {
            _ = wake.notified() => {},
            changed = shutdown.changed() => {
                if changed.is_err() || *shutdown.borrow() {
                    return;
                }
            },
            _ = tokio::time::sleep(Duration::from_secs(1)) => {},
        }
    }
}

async fn cleanup_retained_rows(pool: &SqlitePool) -> sqlx::Result<()> {
    let delivery_days = retention_days("EP_DELIVERY_RETENTION_DAYS", 30);
    let read_notification_days = retention_days("EP_READ_NOTIFICATION_RETENTION_DAYS", 365);
    let mut tx = pool.begin().await?;
    sqlx::query("DELETE FROM notify_delivery WHERE attempted_at < unixepoch() - (?1 * 86400)")
        .bind(delivery_days)
        .execute(&mut *tx)
        .await?;
    sqlx::query(
        "DELETE FROM notify_outbox
          WHERE status IN ('sent','failed','skipped')
            AND updated_at < unixepoch() - (?1 * 86400)",
    )
    .bind(delivery_days)
    .execute(&mut *tx)
    .await?;
    sqlx::query(
        "DELETE FROM notification
          WHERE read_at IS NOT NULL
            AND read_at < unixepoch() - (?1 * 86400)",
    )
    .bind(read_notification_days)
    .execute(&mut *tx)
    .await?;
    tx.commit().await
}

fn retention_days(name: &str, default: i64) -> i64 {
    std::env::var(name)
        .ok()
        .and_then(|value| value.parse::<i64>().ok())
        .filter(|days| (1..=3650).contains(days))
        .unwrap_or(default)
}

async fn claim_due(pool: &SqlitePool) -> sqlx::Result<Vec<(i64, i64)>> {
    let mut tx = pool.begin_with("BEGIN IMMEDIATE").await?;
    let claimed: Vec<(i64, i64)> = sqlx::query_as(&format!(
        "UPDATE notify_outbox
            SET status = 'running',
                attempt_count = attempt_count + 1,
                lease_until = unixepoch() + {DELIVERY_LEASE_SECONDS},
                updated_at = unixepoch()
          WHERE id IN (
                SELECT id FROM notify_outbox
                 WHERE (status = 'pending' AND next_attempt_at <= unixepoch())
                    OR (status = 'running' AND lease_until <= unixepoch())
                 ORDER BY next_attempt_at, id
                 LIMIT {DELIVERY_CONCURRENCY}
          )
        RETURNING id, attempt_count"
    ))
    .fetch_all(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(claimed)
}

type ClaimedDelivery = (
    i64,
    String,
    String,
    i64,
    String,
    Option<String>,
    String,
    Option<String>,
    Option<String>,
);

async fn deliver_claim(pool: &SqlitePool, outbox_id: i64, attempt_count: i64) {
    let row: sqlx::Result<Option<ClaimedDelivery>> = sqlx::query_as(
        "SELECT o.channel_id, c.kind, c.config_json, c.enabled,
                n.severity, n.source, n.title, n.body, n.link
           FROM notify_outbox o
           JOIN notify_channel c ON c.id = o.channel_id
           JOIN notification n ON n.id = o.notification_id
          WHERE o.id = ?1",
    )
    .bind(outbox_id)
    .fetch_optional(pool)
    .await;

    let (channel_id, kind, config, enabled, severity, source, title, body, link) = match row {
        Ok(Some(row)) => row,
        Ok(None) => return,
        Err(error) => {
            tracing::warn!(outbox_id, %error, "failed to load claimed notification delivery");
            release_claim_after_internal_error(pool, outbox_id, attempt_count).await;
            return;
        }
    };

    if enabled == 0 {
        if let Err(error) = sqlx::query(
            "UPDATE notify_outbox
                SET status = 'skipped', lease_until = NULL, updated_at = unixepoch()
              WHERE id = ?1 AND status = 'running' AND attempt_count = ?2",
        )
        .bind(outbox_id)
        .bind(attempt_count)
        .execute(pool)
        .await
        {
            tracing::warn!(outbox_id, %error, "failed to skip disabled notification channel");
        }
        return;
    }

    let result = match Severity::try_parse(&severity) {
        Some(severity) => {
            let message = NotifyMessage {
                severity,
                source,
                title,
                body,
                link,
            };
            match build_notifier(&kind, &config) {
                Ok(notifier) => notifier.send(&message).await,
                Err(error) => Err(error),
            }
        }
        None => Err(anyhow::anyhow!("invalid persisted notification severity")),
    };

    let ok = result.is_ok();
    let safe_error = result.err().map(|error| {
        tracing::warn!(outbox_id, channel_id, %kind, %error, "notification channel delivery failed");
        safe_delivery_error(&kind)
    });
    let (status, next_delay) = if ok {
        ("sent", 0)
    } else if attempt_count >= MAX_DELIVERY_ATTEMPTS {
        ("failed", 0)
    } else {
        let delay = match attempt_count {
            1 => 30,
            2 => 5 * 60,
            _ => 30 * 60,
        };
        ("pending", delay)
    };

    let mut tx = match pool.begin().await {
        Ok(tx) => tx,
        Err(error) => {
            tracing::warn!(outbox_id, %error, "failed to record notification delivery result");
            return;
        }
    };
    let outcome = async {
        sqlx::query(
            "INSERT INTO notify_delivery(notification_id, channel_id, ok, error)
             SELECT notification_id, channel_id, ?2, ?3
               FROM notify_outbox
              WHERE id = ?1 AND status = 'running' AND attempt_count = ?4",
        )
        .bind(outbox_id)
        .bind(ok as i64)
        .bind(safe_error.as_deref())
        .bind(attempt_count)
        .execute(&mut *tx)
        .await?;
        sqlx::query(
            "UPDATE notify_outbox
                SET status = ?2,
                    next_attempt_at = unixepoch() + ?3,
                    lease_until = NULL,
                    last_error = ?4,
                    updated_at = unixepoch()
              WHERE id = ?1 AND status = 'running' AND attempt_count = ?5",
        )
        .bind(outbox_id)
        .bind(status)
        .bind(next_delay)
        .bind(safe_error.as_deref())
        .bind(attempt_count)
        .execute(&mut *tx)
        .await?;
        tx.commit().await
    }
    .await;
    if let Err(error) = outcome {
        tracing::warn!(outbox_id, %error, "failed to persist notification delivery result");
    }
}

async fn release_claim_after_internal_error(pool: &SqlitePool, outbox_id: i64, attempt_count: i64) {
    if let Err(error) = sqlx::query(
        "UPDATE notify_outbox
            SET status = 'pending', next_attempt_at = unixepoch() + 30,
                lease_until = NULL, updated_at = unixepoch()
          WHERE id = ?1 AND status = 'running' AND attempt_count = ?2",
    )
    .bind(outbox_id)
    .bind(attempt_count)
    .execute(pool)
    .await
    {
        tracing::warn!(outbox_id, %error, "failed to release notification outbox lease");
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
#[cfg(test)]
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
    msg.source = msg
        .source
        .and_then(|source| {
            let source = source.trim().to_ascii_lowercase();
            ((2..=32).contains(&source.len())
                && source
                    .bytes()
                    .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-'))
            .then_some(source)
        })
        .or_else(|| Some("system".to_string()));
    msg
}

fn safe_delivery_error(kind: &str) -> String {
    let kind = match kind {
        "inapp" | "smtp" | "bark" | "telegram" | "discord" => kind,
        _ => "unknown",
    };
    format!("{kind} 通道投递失败 · 详细错误已记录")
}

pub(crate) fn build_notifier(kind: &str, config_json: &str) -> anyhow::Result<Box<dyn Notifier>> {
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
                source TEXT,
                title TEXT NOT NULL,
                body TEXT,
                link TEXT
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
                attempted_at INTEGER NOT NULL DEFAULT (unixepoch()),
                ok INTEGER NOT NULL,
                error TEXT
            )",
        )
        .execute(&pool)
        .await
        .expect("create notify_delivery table");
        sqlx::query(
            "CREATE TABLE notify_outbox (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                notification_id INTEGER NOT NULL,
                channel_id INTEGER NOT NULL,
                status TEXT NOT NULL DEFAULT 'pending',
                attempt_count INTEGER NOT NULL DEFAULT 0,
                next_attempt_at INTEGER NOT NULL DEFAULT (unixepoch()),
                lease_until INTEGER,
                last_error TEXT,
                created_at INTEGER NOT NULL DEFAULT (unixepoch()),
                updated_at INTEGER NOT NULL DEFAULT (unixepoch()),
                UNIQUE(notification_id, channel_id)
            )",
        )
        .execute(&pool)
        .await
        .expect("create notify_outbox table");

        pool
    }

    // Provider-focused assertions call `deliver_external` directly. Dispatch
    // tests cover the durable notification + outbox + broadcast boundary.

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
        assert_eq!(broadcast.id, id);
        assert_eq!(broadcast.message.title, "probe");
    }

    #[tokio::test]
    async fn dispatch_enqueues_only_channels_at_or_below_message_severity() {
        let pool = test_pool().await;
        sqlx::query(
            "INSERT INTO notify_channel(kind,name,config_json,min_severity) VALUES
             ('inapp','info','{}','info'),
             ('inapp','critical','{}','crit')",
        )
        .execute(&pool)
        .await
        .expect("channels");
        let bus = NotifyBus::new(pool.clone());
        let id = bus
            .dispatch(NotifyMessage::warn("warning"))
            .await
            .expect("dispatch");

        let queued: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM notify_outbox WHERE notification_id = ?1")
                .bind(id)
                .fetch_one(&pool)
                .await
                .expect("queued count");
        assert_eq!(queued, 1);
    }

    #[tokio::test]
    async fn expired_running_lease_is_reclaimed_and_completed() {
        let pool = test_pool().await;
        sqlx::query(
            "INSERT INTO notify_channel(kind,name,config_json,min_severity)
             VALUES ('inapp','in app','{}','info')",
        )
        .execute(&pool)
        .await
        .expect("channel");
        sqlx::query("INSERT INTO notification(severity,title) VALUES ('info','recover')")
            .execute(&pool)
            .await
            .expect("notification");
        sqlx::query(
            "INSERT INTO notify_outbox(notification_id,channel_id,status,attempt_count,lease_until)
             VALUES (1,1,'running',1,unixepoch()-1)",
        )
        .execute(&pool)
        .await
        .expect("outbox");

        let claimed = claim_due(&pool).await.expect("claim");
        assert_eq!(claimed, vec![(1, 2)]);
        deliver_claim(&pool, 1, 2).await;
        let status: String = sqlx::query_scalar("SELECT status FROM notify_outbox WHERE id = 1")
            .fetch_one(&pool)
            .await
            .expect("status");
        assert_eq!(status, "sent");
    }

    #[tokio::test]
    async fn stale_lease_attempt_cannot_overwrite_reclaimed_work() {
        let pool = test_pool().await;
        sqlx::query(
            "INSERT INTO notify_channel(kind,name,config_json,min_severity)
             VALUES ('inapp','in app','{}','info')",
        )
        .execute(&pool)
        .await
        .expect("channel");
        sqlx::query("INSERT INTO notification(severity,title) VALUES ('info','reclaimed')")
            .execute(&pool)
            .await
            .expect("notification");
        sqlx::query(
            "INSERT INTO notify_outbox(
                notification_id,channel_id,status,attempt_count,lease_until
             ) VALUES (1,1,'running',2,unixepoch()+90)",
        )
        .execute(&pool)
        .await
        .expect("outbox");

        // Attempt 1 finished after attempt 2 reclaimed the expired lease.
        deliver_claim(&pool, 1, 1).await;

        let state: (String, i64) =
            sqlx::query_as("SELECT status, attempt_count FROM notify_outbox WHERE id = 1")
                .fetch_one(&pool)
                .await
                .expect("state");
        assert_eq!(state, ("running".into(), 2));
        let deliveries: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM notify_delivery WHERE notification_id = 1")
                .fetch_one(&pool)
                .await
                .expect("deliveries");
        assert_eq!(deliveries, 0);
    }

    #[tokio::test]
    async fn durable_worker_delivers_and_drains_on_shutdown() {
        let pool = test_pool().await;
        sqlx::query(
            "INSERT INTO notify_channel(kind,name,config_json,min_severity)
             VALUES ('inapp','in app','{}','info')",
        )
        .execute(&pool)
        .await
        .expect("channel");
        let bus = NotifyBus::new(pool.clone());
        let worker = bus.start_worker();
        let id = bus
            .dispatch(NotifyMessage::info("durable"))
            .await
            .expect("dispatch");

        tokio::time::timeout(Duration::from_secs(2), async {
            loop {
                let status: String = sqlx::query_scalar(
                    "SELECT status FROM notify_outbox WHERE notification_id = ?1",
                )
                .bind(id)
                .fetch_one(&pool)
                .await
                .expect("status");
                if status == "sent" {
                    break;
                }
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("worker completes delivery");
        worker.shutdown().await;

        let attempts: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM notify_delivery WHERE notification_id = ?1")
                .bind(id)
                .fetch_one(&pool)
                .await
                .expect("attempts");
        assert_eq!(attempts, 1);
    }

    #[tokio::test]
    async fn dispatch_sanitizes_message_link_and_source() {
        let pool = test_pool().await;
        let bus = NotifyBus::new(pool.clone());

        let id = bus
            .dispatch(
                NotifyMessage::warn("unsafe")
                    .link(" https://evil.example/path ")
                    .source(" finance<script> "),
            )
            .await
            .expect("dispatch notification");

        let stored: (Option<String>, Option<String>) =
            sqlx::query_as("SELECT link, source FROM notification WHERE id = ?1")
                .bind(id)
                .fetch_one(&pool)
                .await
                .expect("notification row");

        assert_eq!(stored, (None, Some("system".into())));
    }

    #[tokio::test]
    async fn dispatch_trims_safe_message_link_and_source() {
        let pool = test_pool().await;
        let bus = NotifyBus::new(pool.clone());

        let id = bus
            .dispatch(
                NotifyMessage::info("safe")
                    .link(" /finance?month=2026-05 ")
                    .source(" Finance "),
            )
            .await
            .expect("dispatch notification");

        let stored: (Option<String>, Option<String>) =
            sqlx::query_as("SELECT link, source FROM notification WHERE id = ?1")
                .bind(id)
                .fetch_one(&pool)
                .await
                .expect("notification row");

        assert_eq!(
            stored,
            (
                Some("/finance?month=2026-05".into()),
                Some("finance".into())
            )
        );
    }

    #[test]
    fn safe_delivery_error_does_not_echo_unknown_kind() {
        let err = safe_delivery_error("custom-webhook-with-secret-token");
        assert_eq!(err, "unknown 通道投递失败 · 详细错误已记录");
    }
}
