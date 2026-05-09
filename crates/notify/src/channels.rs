use ep_core::{NotifyMessage, Severity};
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;

pub const MAX_CHANNEL_NAME_CHARS: usize = 64;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotifyChannelSummary {
    pub id: i64,
    pub kind: String,
    pub name: String,
    pub enabled: bool,
    pub min_severity: String,
    pub created_at: i64,
}

pub async fn list_channels(pool: &SqlitePool) -> anyhow::Result<Vec<NotifyChannelSummary>> {
    let rows: Vec<(i64, String, String, i64, String, i64)> = sqlx::query_as(
        "SELECT id, kind, name, enabled, min_severity, created_at
           FROM notify_channel
          ORDER BY created_at DESC",
    )
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(|r| NotifyChannelSummary {
            id: r.0,
            kind: r.1,
            name: r.2,
            enabled: r.3 != 0,
            min_severity: r.4,
            created_at: r.5,
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
    let input = normalize_channel_fields(kind, name, config_json, min_severity)?;
    let id: i64 = sqlx::query_scalar(
        "INSERT INTO notify_channel (kind, name, enabled, config_json, min_severity)
         VALUES (?1, ?2, 1, ?3, ?4) RETURNING id",
    )
    .bind(&input.kind)
    .bind(&input.name)
    .bind(&input.config_json)
    .bind(&input.min_severity)
    .fetch_one(pool)
    .await?;
    Ok(id)
}

struct ChannelInput {
    kind: String,
    name: String,
    config_json: String,
    min_severity: String,
}

fn normalize_channel_fields(
    kind: &str,
    name: &str,
    config_json: &str,
    min_severity: &str,
) -> anyhow::Result<ChannelInput> {
    let kind = kind.trim().to_ascii_lowercase();
    let name = name.trim().to_string();
    if name.is_empty() {
        anyhow::bail!("channel name is required");
    }
    if name.chars().count() > MAX_CHANNEL_NAME_CHARS {
        anyhow::bail!("channel name must be at most {MAX_CHANNEL_NAME_CHARS} characters");
    }
    let min_severity = Severity::try_parse(min_severity)
        .ok_or_else(|| anyhow::anyhow!("unknown notification severity: {min_severity}"))?
        .as_str()
        .to_string();
    let config_json = config_json.trim().to_string();
    crate::build_notifier(&kind, &config_json)?;
    Ok(ChannelInput {
        kind,
        name,
        config_json,
        min_severity,
    })
}

pub async fn delete_channel(pool: &SqlitePool, id: i64) -> anyhow::Result<bool> {
    let res = sqlx::query("DELETE FROM notify_channel WHERE id = ?1")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(res.rows_affected() > 0)
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

#[cfg(test)]
mod tests {
    use super::{normalize_channel_fields, MAX_CHANNEL_NAME_CHARS};

    #[test]
    fn normalize_channel_fields_canonicalizes_valid_input() {
        let input = normalize_channel_fields(" INAPP ", "  Bell  ", " {} ", "WARNING")
            .expect("valid channel");

        assert_eq!(input.kind, "inapp");
        assert_eq!(input.name, "Bell");
        assert_eq!(input.config_json, "{}");
        assert_eq!(input.min_severity, "warn");
    }

    #[test]
    fn normalize_channel_fields_rejects_invalid_values() {
        assert!(normalize_channel_fields("inapp", " ", "{}", "info").is_err());
        assert!(normalize_channel_fields(
            "inapp",
            &"x".repeat(MAX_CHANNEL_NAME_CHARS + 1),
            "{}",
            "info"
        )
        .is_err());
        assert!(normalize_channel_fields("inapp", "Bell", "{}", "urgent").is_err());
        assert!(normalize_channel_fields("telegram", "Ops", "{}", "info").is_err());
        assert!(normalize_channel_fields(
            "telegram",
            "Ops",
            r#"{"bot_token":"","chat_id":"123"}"#,
            "info"
        )
        .is_err());
        assert!(normalize_channel_fields(
            "bark",
            "Phone",
            r#"{"base_url":"notaurl","device_key":"key"}"#,
            "info"
        )
        .is_err());
        assert!(normalize_channel_fields(
            "discord",
            "Ops",
            r#"{"webhook_url":"ftp://example.com/hook"}"#,
            "info"
        )
        .is_err());
        assert!(
            normalize_channel_fields(
                "smtp",
                "Mail",
                r#"{"host":"smtp.example.com","username":"u","password":"p","from":"bad","to":"ops@example.com"}"#,
                "info"
            )
            .is_err()
        );
    }
}
