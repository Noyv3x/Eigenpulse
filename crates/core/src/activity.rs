use sqlx::SqlitePool;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TodayActivityOrder {
    Asc,
    Desc,
}

#[derive(Clone, Debug, PartialEq)]
pub struct TodayActivity {
    pub date: String,
    pub rows: Vec<TodayActivityRow>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct TodayActivityRow {
    pub occurred_at: i64,
    pub module: String,
    pub doc_id: String,
    pub summary: String,
    pub amount: Option<f64>,
    pub status: Option<String>,
    pub link_doc: Option<String>,
}

type ActivityRow = (
    i64,
    String,
    String,
    String,
    Option<f64>,
    Option<String>,
    Option<String>,
);

pub async fn load_today_activity(
    pool: &SqlitePool,
    order: TodayActivityOrder,
    limit: Option<u32>,
) -> sqlx::Result<TodayActivity> {
    let date: String = sqlx::query_scalar("SELECT date('now','localtime')")
        .fetch_one(pool)
        .await?;

    // "Today" follows local wall-clock time. SQLite modifiers compose
    // left-to-right: shift to localtime, round to local midnight, then convert
    // back to UTC epoch seconds for comparison against stored unix timestamps.
    let order_sql = match order {
        TodayActivityOrder::Asc => "ASC",
        TodayActivityOrder::Desc => "DESC",
    };
    let limit_sql = limit
        .map(|n| format!(" LIMIT {}", n.max(1)))
        .unwrap_or_default();
    let sql = format!(
        "SELECT occurred_at, module, doc_id, summary, amount, status, link_doc
           FROM activity
          WHERE occurred_at >= unixepoch('now','localtime','start of day','utc')
          ORDER BY occurred_at {order_sql}{limit_sql}"
    );
    let rows: Vec<ActivityRow> = sqlx::query_as(&sql).fetch_all(pool).await?;

    Ok(TodayActivity {
        date,
        rows: rows
            .into_iter()
            .map(
                |(occurred_at, module, doc_id, summary, amount, status, link_doc)| {
                    TodayActivityRow {
                        occurred_at,
                        module,
                        doc_id,
                        summary,
                        amount,
                        status,
                        link_doc,
                    }
                },
            )
            .collect(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn pool_with_activity() -> SqlitePool {
        let pool = SqlitePool::connect("sqlite::memory:").await.expect("pool");
        sqlx::query(
            "CREATE TABLE activity (
                occurred_at INTEGER NOT NULL,
                module TEXT NOT NULL,
                doc_id TEXT NOT NULL,
                summary TEXT NOT NULL,
                amount REAL,
                status TEXT,
                link_doc TEXT
            )",
        )
        .execute(&pool)
        .await
        .expect("schema");
        pool
    }

    #[tokio::test]
    async fn load_today_activity_orders_and_limits_rows() {
        let pool = pool_with_activity().await;
        for (ts, doc_id) in [
            ("unixepoch('now') - 20", "FIN-1"),
            ("unixepoch('now') - 10", "FIT-1"),
        ] {
            sqlx::query(&format!(
                "INSERT INTO activity (occurred_at, module, doc_id, summary, amount, status, link_doc)
                 VALUES ({ts}, 'FIN', '{doc_id}', 'summary', -1.0, NULL, NULL)"
            ))
            .execute(&pool)
            .await
            .expect("activity");
        }

        let asc = load_today_activity(&pool, TodayActivityOrder::Asc, None)
            .await
            .expect("asc");
        assert_eq!(asc.rows[0].doc_id, "FIN-1");
        assert_eq!(asc.rows[1].doc_id, "FIT-1");

        let desc = load_today_activity(&pool, TodayActivityOrder::Desc, Some(1))
            .await
            .expect("desc");
        assert_eq!(desc.rows.len(), 1);
        assert_eq!(desc.rows[0].doc_id, "FIT-1");
    }
}
