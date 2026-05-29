use sqlx::{Sqlite, Transaction};

/// Document-id shapes.
///
/// The numeric width baked into each variant name (`5`, `4`) is a **nominal
/// minimum**, not a hard cap. The serial is zero-padded to that width with
/// `{serial:0N}`, which only sets a *floor*: once the running serial rolls past
/// the nominal width (`>999` per year for [`YearSerial5`], `>9999` per type for
/// [`TypeSerial4`]) the id simply grows one extra digit (`FIN-261000`,
/// `FIT-S-10000`). This widening is intentional — it lets a module keep minting
/// ids indefinitely instead of failing the 1000th record of a year. The serial
/// comes from a monotonically-increasing `seq` row, so widened ids remain unique
/// and ordered, and they still satisfy [`crate::safe_doc_id`]'s length/charset
/// window.
#[derive(Clone, Debug)]
pub enum DocIdShape {
    /// `{CODE}-{YY}{NNN}` — e.g. `FIN-26092` (serial width is a minimum; see
    /// the enum-level note on rollover widening).
    YearSerial5,
    /// `{CODE}-{TYPE}-{NNNN}` — e.g. `FIT-S-0412` (serial width is a minimum;
    /// see the enum-level note on rollover widening).
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

    /// The `{serial:0N}` padding is a minimum width: once the serial rolls past
    /// the nominal width the id widens by a digit rather than truncating or
    /// wrapping. Verify both shapes widen at the boundary while staying unique.
    #[test]
    fn format_doc_id_widens_past_nominal_width() {
        // YearSerial5: 3-digit nominal, widens at 1000.
        assert_eq!(
            DocIdShape::YearSerial5.format_doc_id("FIN", 26, 999),
            "FIN-26999"
        );
        assert_eq!(
            DocIdShape::YearSerial5.format_doc_id("FIN", 26, 1000),
            "FIN-261000"
        );
        // TypeSerial4: 4-digit nominal, widens at 10000.
        assert_eq!(
            DocIdShape::TypeSerial4 { kind: "S" }.format_doc_id("FIT", 26, 9999),
            "FIT-S-9999"
        );
        assert_eq!(
            DocIdShape::TypeSerial4 { kind: "S" }.format_doc_id("FIT", 26, 10000),
            "FIT-S-10000"
        );
    }

    /// Driving the sequence well past the nominal width must keep producing
    /// unique, monotonically-increasing ids that all validate via `safe_doc_id`.
    #[tokio::test]
    async fn next_doc_id_unique_and_monotonic_across_rollover() -> sqlx::Result<()> {
        let pool = seq_pool().await?;
        // Seed the seq row just below the rollover so the test stays fast while
        // still crossing the 999 -> 1000 (widening) boundary.
        let yy = current_yy();
        sqlx::query("INSERT INTO seq(module, kind, last_value) VALUES (?1, ?2, ?3)")
            .bind("FIN")
            .bind(format!("doc:y{yy:02}"))
            .bind(997_i64)
            .execute(&pool)
            .await?;

        let mut ids = Vec::new();
        for _ in 0..6 {
            let mut tx = pool.begin().await?;
            let id = next_doc_id(&mut tx, "FIN", DocIdShape::YearSerial5).await?;
            tx.commit().await?;
            ids.push(id);
        }

        // Crosses the nominal-width boundary: 998, 999 (still 3-digit), then
        // 1000, 1001, 1002, 1003 (widened to 4-digit serials).
        assert_eq!(
            ids,
            vec![
                format!("FIN-{yy:02}998"),
                format!("FIN-{yy:02}999"),
                format!("FIN-{yy:02}1000"),
                format!("FIN-{yy:02}1001"),
                format!("FIN-{yy:02}1002"),
                format!("FIN-{yy:02}1003"),
            ]
        );

        // Uniqueness.
        let unique: std::collections::BTreeSet<&String> = ids.iter().collect();
        assert_eq!(unique.len(), ids.len(), "ids must be unique");

        // Monotonic by serial: parse the trailing digits after the `YY` prefix.
        let serials: Vec<i64> = ids
            .iter()
            .map(|id| {
                let tail = id.rsplit('-').next().unwrap();
                tail[2..].parse::<i64>().unwrap()
            })
            .collect();
        assert!(
            serials.windows(2).all(|w| w[1] > w[0]),
            "serials must strictly increase: {serials:?}"
        );

        // Every widened id is still accepted by the shared validator.
        for id in &ids {
            assert_eq!(
                crate::safe_doc_id(id),
                Some(id.as_str()),
                "widened id should validate: {id}"
            );
        }

        Ok(())
    }

    /// A widened `TypeSerial4` id (5-digit serial) must also pass `safe_doc_id`.
    #[test]
    fn safe_doc_id_accepts_widened_type_serial() {
        let id = DocIdShape::TypeSerial4 { kind: "S" }.format_doc_id("FIT", 26, 12345);
        assert_eq!(id, "FIT-S-12345");
        assert_eq!(crate::safe_doc_id(&id), Some(id.as_str()));
    }
}
