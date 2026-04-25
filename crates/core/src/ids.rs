use sqlx::{Sqlite, Transaction};

#[derive(Clone, Debug)]
pub enum DocIdShape {
    /// `{CODE}-{YY}{NNNNN}` — e.g. `FIN-26092`
    YearSerial5,
    /// `{CODE}-{TYPE}-{NNNN}` — e.g. `FIT-S-0412`
    TypeSerial4 { kind: &'static str },
}

/// Generate the next document id atomically.
/// Must be called inside a transaction so the seq increment commits with the row insert.
pub async fn next_doc_id(
    tx: &mut Transaction<'_, Sqlite>,
    module_code: &str,
    shape: DocIdShape,
) -> sqlx::Result<String> {
    let kind_key = match &shape {
        DocIdShape::YearSerial5 => {
            let yy = current_yy();
            format!("doc:y{:02}", yy)
        }
        DocIdShape::TypeSerial4 { kind } => format!("type:{}", kind),
    };

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

    Ok(match shape {
        DocIdShape::YearSerial5 => {
            let yy = current_yy();
            format!("{module_code}-{yy:02}{next:05}")
        }
        DocIdShape::TypeSerial4 { kind } => {
            format!("{module_code}-{kind}-{next:04}")
        }
    })
}

fn current_yy() -> u32 {
    let now = time::OffsetDateTime::now_utc();
    (now.year() as u32) % 100
}
