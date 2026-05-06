use ep_core::{NotifyMessage, Severity};
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotifyChannelRow {
    pub id: i64,
    pub kind: String,
    pub name: String,
    pub enabled: bool,
    pub config_json: String,
    pub min_severity: String,
    pub created_at: i64,
}

pub async fn list_channels(pool: &SqlitePool) -> anyhow::Result<Vec<NotifyChannelRow>> {
    let rows: Vec<(i64, String, String, i64, String, String, i64)> = sqlx::query_as(
        "SELECT id, kind, name, enabled, config_json, min_severity, created_at
           FROM notify_channel
          ORDER BY created_at DESC",
    )
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(|r| NotifyChannelRow {
            id: r.0,
            kind: r.1,
            name: r.2,
            enabled: r.3 != 0,
            config_json: r.4,
            min_severity: r.5,
            created_at: r.6,
        })
        .collect())
}

pub async fn create_channel(
    pool: &SqlitePool,
    kind: &str,
    name: &str,
    config_json: &str,
    min_severity: &str,
) -> anyhow::Result<i64> {
    let id: i64 = sqlx::query_scalar(
        "INSERT INTO notify_channel (kind, name, enabled, config_json, min_severity)
         VALUES (?1, ?2, 1, ?3, ?4) RETURNING id",
    )
    .bind(kind)
    .bind(name)
    .bind(config_json)
    .bind(min_severity)
    .fetch_one(pool)
    .await?;
    Ok(id)
}

pub async fn update_channel(
    pool: &SqlitePool,
    id: i64,
    enabled: bool,
    name: &str,
    config_json: &str,
    min_severity: &str,
) -> anyhow::Result<()> {
    sqlx::query(
        "UPDATE notify_channel
            SET enabled = ?1, name = ?2, config_json = ?3, min_severity = ?4
          WHERE id = ?5",
    )
    .bind(enabled as i64)
    .bind(name)
    .bind(config_json)
    .bind(min_severity)
    .bind(id)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn delete_channel(pool: &SqlitePool, id: i64) -> anyhow::Result<()> {
    sqlx::query("DELETE FROM notify_channel WHERE id = ?1")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn test_channel(kind: &str, config_json: &str) -> anyhow::Result<()> {
    let n = crate::build_notifier(kind, config_json)?;
    let msg = NotifyMessage {
        severity: Severity::Info,
        module: Some("CFG".into()),
        title: "Eigenpulse · Channel test".into(),
        body: Some(format!(
            "If you see this message, the {kind} channel is working."
        )),
        link: None,
        doc_ref: None,
    };
    n.send(&msg).await
}
