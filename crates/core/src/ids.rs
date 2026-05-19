use sqlx::{Sqlite, Transaction};

#[derive(Clone, Debug)]
pub enum DocIdShape {
    /// `{CODE}-{YY}{NNN}` — e.g. `FIN-26092`
    YearSerial5,
    /// `{CODE}-{TYPE}-{NNNN}` — e.g. `FIT-S-0412`
    TypeSerial4 { kind: &'static str },
}

impl DocIdShape {
    fn sequence_kind(&self, yy: u32) -> String {
        match self {
            Self::YearSerial5 => format!("doc:y{yy:02}"),
            Self::TypeSerial4 { kind } => format!("type:{kind}"),
        }
    }

    fn format_doc_id(&self, module_code: &str, yy: u32, serial: i64) -> String {
        match self {
            Self::YearSerial5 => format!("{module_code}-{yy:02}{serial:03}"),
            Self::TypeSerial4 { kind } => format!("{module_code}-{kind}-{serial:04}"),
        }
    }
}

/// Generate the next document id atomically.
/// Must be called inside a transaction so the seq increment commits with the row insert.
pub async fn next_doc_id(
    tx: &mut Transaction<'_, Sqlite>,
    module_code: &str,
    shape: DocIdShape,
) -> sqlx::Result<String> {
    let yy = current_yy();
    let kind_key = shape.sequence_kind(yy);

    let next: i64 = sqlx::query_scalar(
        r#"
        INSERT INTO seq(module, kind, last_value) VALUES (?1, ?2, 1)
        ON CONFLICT(module, kind) DO UPDATE SET last_value = last_value + 1
        RETURNING last_value
        "#,
    )
    .bind(module_code)
    .bind(&kind_key)
    .fetch_one(&mut **tx)
    .await?;

    Ok(shape.format_doc_id(module_code, yy, next))
}

fn current_yy() -> u32 {
    let now = time::OffsetDateTime::now_utc();
    now.year().rem_euclid(100) as u32
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn seq_pool() -> sqlx::Result<sqlx::SqlitePool> {
        let pool = sqlx::SqlitePool::connect("sqlite::memory:").await?;
        sqlx::query(
            "CREATE TABLE seq (
                module TEXT NOT NULL,
                kind TEXT NOT NULL,
                last_value INTEGER NOT NULL,
                PRIMARY KEY (module, kind)
            )",
        )
        .execute(&pool)
        .await?;
        Ok(pool)
    }

    #[test]
    fn doc_id_shape_builds_sequence_keys_and_ids() {
        assert_eq!(DocIdShape::YearSerial5.sequence_kind(26), "doc:y26");
        assert_eq!(
            DocIdShape::YearSerial5.format_doc_id("FIN", 26, 92),
            "FIN-26092"
        );
        assert_eq!(
            DocIdShape::TypeSerial4 { kind: "S" }.sequence_kind(26),
            "type:S"
        );
        assert_eq!(
            DocIdShape::TypeSerial4 { kind: "S" }.format_doc_id("FIT", 26, 413),
            "FIT-S-0413"
        );
    }

    #[tokio::test]
    async fn next_doc_id_increments_inside_transaction() -> sqlx::Result<()> {
        let pool = seq_pool().await?;
        let mut tx = pool.begin().await?;

        let first = next_doc_id(&mut tx, "FIT", DocIdShape::TypeSerial4 { kind: "S" }).await?;
        let second = next_doc_id(&mut tx, "FIT", DocIdShape::TypeSerial4 { kind: "S" }).await?;
        tx.commit().await?;

        assert_eq!(first, "FIT-S-0001");
        assert_eq!(second, "FIT-S-0002");
        Ok(())
    }

    #[tokio::test]
    async fn next_doc_id_keeps_year_serials_per_year() -> sqlx::Result<()> {
        let pool = seq_pool().await?;
        let yy = current_yy();
        let mut tx = pool.begin().await?;

        let doc = next_doc_id(&mut tx, "FIN", DocIdShape::YearSerial5).await?;
        tx.commit().await?;

        assert_eq!(doc, format!("FIN-{yy:02}001"));
        let key: String = sqlx::query_scalar("SELECT kind FROM seq WHERE module = 'FIN'")
            .fetch_one(&pool)
            .await?;
        assert_eq!(key, format!("doc:y{yy:02}"));
        Ok(())
    }
}
